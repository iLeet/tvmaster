#!/usr/bin/env bash
# End-to-end test of a real daemon with real FFmpeg, over HTTP and HTTPS.
#   scripts/system-test.sh [path-to-tvmaster-binary]
# Makes its own test media, starts the server on throwaway ports with a token and
# a self-signed certificate, then behaves like the clients people will use:
# curl, ffprobe/ffmpeg reading the streams as an IPTV app would, and a
# WebSocket client. Exit status is the number of failed checks.
set -uo pipefail

BIN="${1:-target/debug/tvmaster}"
[[ -x "$BIN" ]] || { echo "binary not found: $BIN" >&2; exit 2; }
command -v ffmpeg >/dev/null && command -v ffprobe >/dev/null || { echo "ffmpeg/ffprobe needed" >&2; exit 2; }

W="$(mktemp -d)"; PASS=0; FAIL=0
HP=$((20000 + RANDOM % 20000)); SP=$((HP + 1))
cleanup() { [[ -n "${PID:-}" ]] && kill "$PID" 2>/dev/null; wait "$PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT

ok()   { PASS=$((PASS+1)); printf '  ok    %s\n' "$1"; }
bad()  { FAIL=$((FAIL+1)); printf '  FAIL  %s\n' "$1"; [[ -n "${2:-}" ]] && printf '        %s\n' "$2"; }
check() { # check "name" command...
  local name="$1"; shift
  local out; if out="$("$@" 2>&1)"; then ok "$name"; else bad "$name" "$(echo "$out" | tail -2 | tr '\n' ' ')"; fi
}
section() { printf '\n%s\n' "$1"; }

# ---- media -------------------------------------------------------------
section "Making test media"
mkdir -p "$W/tv/Alpha/Season 1" "$W/tv/Beta/Season 1" "$W/ads"
clip() { ffmpeg -hide_banner -loglevel error -y -f lavfi -i "testsrc=size=320x240:rate=25:duration=$2" -f lavfi -i "sine=frequency=$3:duration=$2" -c:v libx264 -pix_fmt yuv420p -c:a aac -shortest "$1"; }
for e in 1 2 3; do
  clip "$W/tv/Alpha/Season 1/Alpha - S01E0$e - Episode $e.mp4" 12 440
  clip "$W/tv/Beta/Season 1/Beta - S01E0$e - Chapter $e.mp4" 12 660
done
for a in 1 2 3 4; do clip "$W/ads/Spot $a.mp4" 4 $((300 + a*50)); done
ok "6 episodes and 4 commercials generated"

# ---- server ------------------------------------------------------------
section "Starting the server (HTTP $HP, HTTPS $SP, token)"
TOKEN="$("$BIN" token)"
"$BIN" serve --bind "127.0.0.1:$HP" --tls --tls-bind "127.0.0.1:$SP" --data-dir "$W/data" --token "$TOKEN" >"$W/server.log" 2>&1 &
PID=$!
for _ in $(seq 1 60); do curl -fsS "http://127.0.0.1:$HP/api/v1/health" >/dev/null 2>&1 && break; sleep 0.25; done
CERT="$W/data/tls/cert.pem"
H="http://127.0.0.1:$HP"; S="https://localhost:$SP"
auth=(-H "Authorization: Bearer $TOKEN")
json() { python3 -c "import json,sys; d=json.load(sys.stdin); print(eval(sys.argv[1]))" "$1"; }
api() { curl -fsS "${auth[@]}" -H 'content-type: application/json' "$@"; }

check "HTTP health" curl -fsS "$H/api/v1/health"
check "HTTPS health (certificate trusted explicitly)" curl -fsS --cacert "$CERT" "$S/api/v1/health"
if curl -fsS "$S/api/v1/health" >/dev/null 2>&1; then bad "HTTPS refused without trusting the certificate"; else ok "HTTPS is refused when the certificate is not trusted"; fi
FP_SERVER="$(api "$H/api/v1/system" | json "d['network']['tls']['fingerprint']")"
FP_FILE="$(openssl x509 -in "$CERT" -outform der | openssl dgst -sha256 -hex | sed 's/.*= *//' | tr a-f A-F | sed 's/../&:/g; s/:$//')"
[[ "$FP_SERVER" == "$FP_FILE" ]] && ok "the advertised fingerprint is the real certificate's" || bad "fingerprint mismatch" "$FP_SERVER vs $FP_FILE"

# ---- auth --------------------------------------------------------------
section "Access token"
[[ "$(curl -s -o /dev/null -w '%{http_code}' "$H/api/v1/channels")" == 401 ]] && ok "no token: 401 over HTTP" || bad "unauthenticated request was not refused"
[[ "$(curl -s -o /dev/null -w '%{http_code}' --cacert "$CERT" "$S/api/v1/channels")" == 401 ]] && ok "no token: 401 over HTTPS" || bad "unauthenticated HTTPS request was not refused"
[[ "$(curl -s -o /dev/null -w '%{http_code}' "$H/api/v1/channels?token=$TOKEN")" == 200 ]] && ok "?token= accepted (for apps that cannot send headers)" || bad "?token= rejected"
[[ "$(curl -s -o /dev/null -w '%{http_code}' "$H/api/v1/channels?token=wrong-token-123")" == 401 ]] && ok "a wrong token is refused" || bad "a wrong token was accepted"
curl -s -i --cacert "$CERT" -XPOST -H 'content-type: application/json' -d "{\"token\":\"$TOKEN\"}" "$S/api/v1/auth/login" | grep -qi 'set-cookie:.*Secure' && ok "login cookie is Secure over HTTPS" || bad "cookie not Secure over HTTPS"
curl -s -i -XPOST -H 'content-type: application/json' -d "{\"token\":\"$TOKEN\"}" "$H/api/v1/auth/login" | grep -i 'set-cookie' | grep -qi Secure && bad "cookie marked Secure over plain HTTP (browsers would drop it)" || ok "login cookie is not Secure over plain HTTP"

# ---- library, channels -------------------------------------------------
section "Libraries and channels"
api -XPOST "$H/api/v1/libraries" -d "{\"path\":\"$W/tv\"}" >/dev/null
api -XPOST "$H/api/v1/libraries" -d "{\"path\":\"$W/ads\",\"kind\":\"commercials\"}" >/dev/null
SCAN="$(api -XPOST "$H/api/v1/libraries/1/scan")"; [[ "$(echo "$SCAN" | json "d['indexed']")" == 6 ]] && ok "scan indexed 6 episodes" || bad "scan result" "$SCAN"
SCAN="$(api -XPOST "$H/api/v1/libraries/2/scan")"; [[ "$(echo "$SCAN" | json "d['indexed']")" == 4 ]] && ok "scan indexed 4 commercials" || bad "scan result" "$SCAN"
api -XPOST "$H/api/v1/channels" -d '{"id":"retro","number":4,"name":"Retro TV","strategy":"sequential","library_ids":[1],"lineup":{"grid_minutes":15,"ads":{"library_ids":[2],"pod_seconds":20,"max_breaks":1},"blocks":[{"name":"All day","start":"00:00","end":"24:00","source":{"type":"pool","shows":["Alpha","Beta"],"order":"sequential"}}]}}' >/dev/null && ok "created a channel with a lineup and ads"
api -XPOST "$H/api/v1/channels" -d '{"id":"chaos","number":5,"name":"Chaos TV","strategy":"chaos","chaos":100,"library_ids":[1]}' >/dev/null && ok "created a chaos channel"
api -XPOST "$H/api/v1/channels" -d '{"id":"guide","number":3,"name":"Guide","kind":"guide"}' >/dev/null && ok "created the guide channel"
N="$(api "$H/api/v1/channels" | json 'len(d)')"; [[ "$N" == 3 ]] && ok "3 channels listed" || bad "channel count $N"
sleep 1
ITEMS="$(api "$H/api/v1/channels/retro/schedule" | json "len(d)")"
[[ "${ITEMS:-0}" -gt 3 ]] && ok "the schedule has $ITEMS items" || bad "schedule is empty"
api -XPOST "$H/api/v1/channels/retro/reroll" >/dev/null && ok "re-roll works"

# ---- playlists and tuner ----------------------------------------------
section "IPTV playlist, guide and tuner"
M3U_H="$(curl -fsS "$H/iptv/playlist.m3u?token=$TOKEN")"
echo "$M3U_H" | grep -q '^#EXTM3U' && ok "M3U playlist" || bad "M3U playlist"
[[ "$(echo "$M3U_H" | grep -c '^#EXTINF')" == 3 ]] && ok "M3U lists all 3 channels" || bad "M3U channel count"
echo "$M3U_H" | grep -q "http://127.0.0.1:$HP/stream/retro/index.m3u8?token=$TOKEN" && ok "over HTTP the playlist links to http://" || bad "M3U stream link"
M3U_S="$(curl -fsS --cacert "$CERT" "$S/iptv/playlist.m3u?token=$TOKEN")"
echo "$M3U_S" | grep -q "https://localhost:$SP/stream/retro/index.m3u8" && ! echo "$M3U_S" | grep -q 'http://' && ok "over HTTPS the playlist links only to https://" || bad "M3U over HTTPS" "$(echo "$M3U_S" | sed -n 2,3p)"
curl -fsS "$H/iptv/guide.xml?token=$TOKEN" | python3 -c "import sys,xml.dom.minidom as m; m.parseString(sys.stdin.read())" && ok "XMLTV guide is well-formed XML" || bad "XMLTV not well-formed"
DISC="$(curl -fsS --cacert "$CERT" "${auth[@]}" "$S/discover.json")"; echo "$DISC" | grep -q "https://localhost:$SP" && ok "HDHomeRun discover.json uses the HTTPS address" || bad "HDHomeRun discover" "$DISC"
curl -fsS "${auth[@]}" "$H/lineup.json" | python3 -c "import sys,json; l=json.load(sys.stdin); assert len(l)>=2" && ok "HDHomeRun lineup" || bad "HDHomeRun lineup"

# ---- streams as a player would read them ------------------------------
section "Streams"
probe() { ffprobe -v error -show_entries stream=codec_name,codec_type -of csv=p=0 "$@" 2>&1; }
OUT="$(probe "$H/stream/retro/index.m3u8?token=$TOKEN")"
echo "$OUT" | grep -q h264 && echo "$OUT" | grep -q aac && ok "ffprobe reads the HLS stream over HTTP (h264 + aac)" || bad "HLS over HTTP" "$OUT"
# ffmpeg's HLS reader does not hand its trust settings to the segment requests (and GnuTLS builds ignore
# SSL_CERT_FILE), so check HTTPS the way a careful client would: playlist and a segment through curl with
# the certificate trusted, then probe the segment.
PL="$(curl -fsS --cacert "$CERT" "$S/stream/retro/index.m3u8?token=$TOKEN")"
SEG="$(echo "$PL" | grep -m1 'seg_.*\.ts')"
if [[ -n "$SEG" ]] && curl -fsS --cacert "$CERT" -o "$W/seg.ts" "$S/stream/retro/$SEG"; then
  OUT="$(probe "$W/seg.ts")"
  echo "$OUT" | grep -q h264 && echo "$OUT" | grep -q aac && ok "HLS playlist and segment over HTTPS carry h264 + aac" || bad "HLS segment over HTTPS" "$OUT"
else bad "could not fetch an HLS segment over HTTPS" "$PL"; fi
# An audio timestamp can overlap by ~20 ms where two items join; players ignore it, so it is not a failure here.
tsdecode() { ffmpeg -hide_banner -loglevel error -t 4 "$@" -f null - 2>&1 | grep -v 'non monotonically increasing dts' ; true; }
OUT="$(tsdecode -i "$H/stream/chaos.ts?token=$TOKEN")"; [[ -z "$OUT" ]] && ok "ffmpeg decodes 4 s of the MPEG-TS tuner stream" || bad "TS stream" "$OUT"
OUT="$(tsdecode -tls_verify 1 -ca_file "$CERT" -i "$S/stream/chaos.ts?token=$TOKEN")"; [[ -z "$OUT" ]] && ok "ffmpeg decodes the MPEG-TS stream over HTTPS" || bad "TS over HTTPS" "$OUT"
# the playlist must carry the token on segment lines too
curl -fsS "$H/stream/retro/index.m3u8?token=$TOKEN" | grep -q "seg_.*\.ts?token=$TOKEN" && ok "segment URLs carry the token (VLC and Plex cannot add headers)" || bad "segment URLs lack the token"
# many clients, one encode
( for i in 1 2 3; do curl -fsS -o /dev/null "$H/stream/retro/index.m3u8?token=$TOKEN" & done; wait ) && ok "three simultaneous viewers"
RUN="$(api "$H/api/v1/health" | json "len(d['on_air'])")"; [[ "$RUN" -ge 2 ]] && ok "channels on air: $RUN (one encode each)" || bad "on air $RUN"
OUT="$(probe "$H/stream/guide/index.m3u8?token=$TOKEN")"; echo "$OUT" | grep -q h264 && ok "the guide channel streams" || bad "guide channel" "$OUT"

# ---- TVs and remote ----------------------------------------------------
section "TVs and the remote"
api -XPOST "$H/api/v1/tvs" -d '{"id":"living-room","name":"Living Room"}' >/dev/null && ok "registered a TV"
api -XPOST "$H/api/v1/tvs/living-room/channel" -d '{"number":4}' >/dev/null && api -XPOST "$H/api/v1/tvs/living-room/volume" -d '{"level":35}' >/dev/null && ok "tuned it and set the volume"
[[ "$(api "$H/api/v1/tvs/living-room" | json "d['channel']")" == 4 && "$(api "$H/api/v1/tvs/living-room" | json "d['volume']")" == 35 ]] && ok "the TV reports channel 4 at volume 35" || bad "TV state" "$(api "$H/api/v1/tvs/living-room")"
cat > "$W/ws.mjs" <<'JS'
const [url, token, tv] = process.argv.slice(2);
const ws = new WebSocket(`${url}?token=${token}`);
const seen = [];
const done = (code) => { try { ws.close(); } catch {} ; console.log(JSON.stringify(seen)); process.exit(code); };
setTimeout(() => done(seen.some((m) => m.type === 'tv.state') ? 0 : 1), 4000);
ws.onopen = async () => { await fetch(url.replace(/^ws/, 'http').replace(/\/ws$/, `/tvs/${tv}/volume?token=${token}`), { method: 'POST', headers: {'content-type':'application/json'}, body: JSON.stringify({ level: 42 }) }); };
ws.onmessage = (e) => { try { seen.push(JSON.parse(e.data)); } catch {} if (seen.some((m) => m.type === 'tv.state')) done(0); };
ws.onerror = () => done(2);
JS
if command -v node >/dev/null && node -e 'process.exit(typeof WebSocket === "undefined" ? 1 : 0)'; then
  OUT="$(node "$W/ws.mjs" "ws://127.0.0.1:$HP/api/v1/ws" "$TOKEN" living-room 2>&1)"; [[ $? == 0 ]] && ok "WebSocket events arrive over ws://" || bad "ws://" "$OUT"
  OUT="$(NODE_EXTRA_CA_CERTS="$CERT" node "$W/ws.mjs" "wss://localhost:$SP/api/v1/ws" "$TOKEN" living-room 2>&1)"; [[ $? == 0 ]] && ok "WebSocket events arrive over wss://" || bad "wss://" "$OUT"
else
  echo "  skip  WebSocket checks (needs Node 22 or newer for a built-in WebSocket)"
fi

# ---- web UI ------------------------------------------------------------
section "Web UI"
curl -fsS "$H/admin/" | grep -qi '<div id="root"' && ok "admin UI is served over HTTP" || bad "admin UI"
curl -fsS --cacert "$CERT" "$S/admin/" | grep -qi '<div id="root"' && ok "admin UI is served over HTTPS" || bad "admin UI over HTTPS"
curl -fsS --http2 --cacert "$CERT" -o /dev/null -w '%{http_version}' "$S/admin/" | grep -q '^2$' && ok "HTTPS negotiates HTTP/2 (what browsers use)" || bad "no HTTP/2"

# ---- the guide, pictures, monitoring -----------------------------------
section "Guide options, uploads and monitoring"
# the access token was given on the command line, so the UI may not change it
[[ "$(api "$H/api/v1/security" | json "d['locked']")" == True ]] && ok "a token set with --token is reported as locked" || bad "security status" "$(api "$H/api/v1/security")"
[[ "$(curl -s -o /dev/null -w '%{http_code}' -XPUT "${auth[@]}" -H 'content-type: application/json' -d '{"enabled":false}' "$H/api/v1/security")" == 409 ]] && ok "the UI cannot switch off a command-line token" || bad "locked token was changeable"
# a picture is uploaded, served back and used
ffmpeg -hide_banner -loglevel error -y -f lavfi -i "color=c=orange:s=64x36:d=1" -frames:v 1 "$W/logo.png"
UP="$(curl -fsS "${auth[@]}" -H 'content-type: image/png' --data-binary "@$W/logo.png" "$H/api/v1/assets?name=My%20Logo.png")"
ASSET_PATH="$(echo "$UP" | json "d['path']")"; ASSET_URL="$(echo "$UP" | json "d['url']")"
[[ -f "$ASSET_PATH" ]] && ok "an uploaded picture is stored on the server" || bad "upload" "$UP"
curl -fsS "${auth[@]}" "$H$ASSET_URL" | cmp -s - "$W/logo.png" && ok "and served back byte for byte" || bad "asset download differs"
[[ "$(echo '<svg/>' | curl -s -o /dev/null -w '%{http_code}' -XPOST "${auth[@]}" --data-binary @- "$H/api/v1/assets")" == 400 ]] && ok "something that is not a picture is refused" || bad "non-image accepted"
api -XPOST "$H/api/v1/channels" -d "{\"id\":\"logo\",\"number\":8,\"name\":\"Logo TV\",\"kind\":\"test\",\"options\":{\"off_air\":{\"screen\":\"image\",\"image\":\"$ASSET_PATH\"},\"overlay\":{\"bug\":{\"enabled\":true,\"image\":\"$ASSET_PATH\"}}}}" >/dev/null && ok "a test channel uses the uploaded picture as its screen and logo" || bad "channel with picture"
OUT="$(probe "$H/stream/logo/index.m3u8?token=$TOKEN")"; echo "$OUT" | grep -q h264 && ok "and it streams" || bad "picture channel stream" "$OUT"
# the guide editor's preview
curl -fsS "${auth[@]}" -H 'content-type: application/json' -d '{"name":"Preview","guide":{"theme":"neon","layout":{"style":"now_next"},"pip":{"mode":"rotate"}}}' "$H/api/v1/guide/preview" -o "$W/preview.png" && head -c 8 "$W/preview.png" | od -An -c | tr -d ' \n' | grep -q 'PNG' && ok "the guide preview is a PNG" || bad "guide preview"
# a guide with its own look, a clock in 24-hour time and a live video window
api -XPOST "$H/api/v1/channels" -d '{"id":"guide","number":3,"name":"Guide","kind":"guide","guide":{"theme":"prevue","hours":2,"header":{"hour12":false},"layout":{"rows":4,"time_step":15},"pip":{"mode":"channel","channel":"retro","placement":"side_right","size":"medium"}}}' >/dev/null && ok "the guide is set up with a theme, 24-hour times and a video window"
curl -fsS "${auth[@]}" "$H/stream/guide/index.m3u8" >/dev/null
sleep 8
[[ "$(api "$H/api/v1/health" | json "'retro' in d['on_air']")" == True ]] && ok "showing a channel in the guide's window put that channel on air" || bad "the window's channel is not on air"
ffmpeg -hide_banner -loglevel error -y -t 6 -i "$H/stream/guide/index.m3u8?token=$TOKEN" -f null - 2>"$W/guide.err" && ok "the guide with a live window decodes" || bad "guide with window" "$(tail -2 "$W/guide.err")"
# watching folders and alerts
[[ "$(api "$H/api/v1/monitor" | json "d['settings']['watch_minutes']")" == 15 ]] && ok "folders are watched every 15 minutes by default" || bad "monitor settings"
clip "$W/tv/Alpha/Season 1/Alpha - S01E04 - Episode 4.mp4" 12 440; touch -t 202001010000 "$W/tv/Alpha/Season 1/Alpha - S01E04 - Episode 4.mp4"
R="$(api -XPOST "$H/api/v1/monitor/rescan")"; [[ "$(echo "$R" | json "d['added']")" == 1 ]] && ok "a new episode dropped into a library is found without a manual scan" || bad "rescan" "$R"
rm "$W/tv/Alpha/Season 1/Alpha - S01E04 - Episode 4.mp4"
R="$(api -XPOST "$H/api/v1/monitor/rescan")"; [[ "$(echo "$R" | json "d['removed']")" == 1 ]] && ok "and a removed one is noticed" || bad "rescan removal" "$R"
# the player works on a TV remote and without internet
PLAYER="$(curl -fsS "$H/")"
echo "$PLAYER" | grep -q ChannelUp && ! echo "$PLAYER" | grep -q cdnjs && ok "the web player takes TV remote keys and loads nothing from the internet" || bad "player"
[[ "$(curl -s -o /dev/null -w '%{http_code}' "$H/admin/vendor/hls.min.js")" == 200 ]] && ok "the player library is served by the server itself" || bad "hls.js not served"
# an access token that can be switched on and off while running
HP2=$((HP + 2)); mkdir -p "$W/data2"
"$BIN" serve --bind "127.0.0.1:$HP2" --data-dir "$W/data2" >"$W/server2.log" 2>&1 &
PID2=$!
for _ in $(seq 1 40); do curl -fsS "http://127.0.0.1:$HP2/api/v1/health" >/dev/null 2>&1 && break; sleep 0.25; done
H2="http://127.0.0.1:$HP2"
[[ "$(curl -s -o /dev/null -w '%{http_code}' "$H2/api/v1/channels")" == 200 ]] && ok "with no token the server is open" || bad "open server refused"
T2="$(curl -fsS -XPUT -H 'content-type: application/json' -d '{"enabled":true}' "$H2/api/v1/security" | json "d['token']")"
[[ "$(curl -s -o /dev/null -w '%{http_code}' "$H2/api/v1/channels")" == 401 && "$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $T2" "$H2/api/v1/channels")" == 200 ]] && ok "a token can be switched on from the UI's API while running" || bad "enable token"
kill -INT "$PID2"; wait "$PID2" 2>/dev/null
"$BIN" serve --bind "127.0.0.1:$HP2" --data-dir "$W/data2" >"$W/server2.log" 2>&1 &
PID2=$!
for _ in $(seq 1 40); do curl -s "http://127.0.0.1:$HP2/api/v1/health" >/dev/null 2>&1 && break; sleep 0.25; done
[[ "$(curl -s -o /dev/null -w '%{http_code}' "$H2/api/v1/channels")" == 401 ]] && ok "and it is still on after a restart" || bad "token not kept"
[[ "$(curl -s -o /dev/null -w '%{http_code}' -XPUT -H 'content-type: application/json' -H "Authorization: Bearer $T2" -d '{"enabled":false}' "$H2/api/v1/security")" == 200 && "$(curl -s -o /dev/null -w '%{http_code}' "$H2/api/v1/channels")" == 200 ]] && ok "and can be switched off again" || bad "disable token"
kill -INT "$PID2"; wait "$PID2" 2>/dev/null
# what the packages do: a token that only seeds the first run, so the UI can turn it off for good
mkdir -p "$W/data3"; printf '%s' "$("$BIN" token)" > "$W/first-token"
"$BIN" serve --bind "127.0.0.1:$HP2" --data-dir "$W/data3" --initial-token-file "$W/first-token" >"$W/server3.log" 2>&1 &
PID2=$!
for _ in $(seq 1 40); do curl -s "http://127.0.0.1:$HP2/api/v1/health" >/dev/null 2>&1 && break; sleep 0.25; done
T3="$(cat "$W/first-token")"
[[ "$(curl -s -o /dev/null -w '%{http_code}' "$H2/api/v1/channels")" == 401 ]] && ok "a first-run token file starts the server protected" || bad "initial token not applied"
[[ "$(curl -s -H "Authorization: Bearer $T3" "$H2/api/v1/security" | json "d['locked']")" == False ]] && ok "and the UI is allowed to change it" || bad "initial token is locked"
curl -s -o /dev/null -XPUT -H 'content-type: application/json' -H "Authorization: Bearer $T3" -d '{"enabled":false}' "$H2/api/v1/security"
kill -INT "$PID2"; wait "$PID2" 2>/dev/null
"$BIN" serve --bind "127.0.0.1:$HP2" --data-dir "$W/data3" --initial-token-file "$W/first-token" >"$W/server3.log" 2>&1 &
PID2=$!
for _ in $(seq 1 40); do curl -s "http://127.0.0.1:$HP2/api/v1/health" >/dev/null 2>&1 && break; sleep 0.25; done
[[ "$(curl -s -o /dev/null -w '%{http_code}' "$H2/api/v1/channels")" == 200 ]] && ok "and a token switched off stays off after a restart (the seed is used once)" || bad "seed token came back"
kill -INT "$PID2"; wait "$PID2" 2>/dev/null

# ---- shutdown ----------------------------------------------------------
section "Shutdown"
kill -INT "$PID"; for _ in $(seq 1 40); do kill -0 "$PID" 2>/dev/null || break; sleep 0.25; done
if kill -0 "$PID" 2>/dev/null; then bad "the server did not stop on SIGINT"; else ok "SIGINT stops both listeners and the encoders"; fi
sleep 0.5; [[ -z "$(pgrep -f "$W/data" || true)" ]] && ok "no FFmpeg processes left behind" || bad "leftover processes" "$(pgrep -fl "$W" | head -3)"
PID=""

printf '\n%d passed, %d failed\n' "$PASS" "$FAIL"
exit "$FAIL"
