#!/usr/bin/env bash
# Builds the Docker image and checks it the way a person would use it.
#   scripts/docker-test.sh            (needs Docker, curl and python3; run from the repo root)
# Exit status is the number of failed checks. Removes everything it makes.
set -uo pipefail
command -v docker >/dev/null && command -v curl >/dev/null && command -v python3 >/dev/null || { echo "docker, curl and python3 needed" >&2; exit 2; }

IMG="tvmaster:test-$$"; C="tvmaster-test-$$"; V="tvmaster-test-data-$$"
W="$(mktemp -d)"; MEDIA="$W/media"; mkdir -p "$MEDIA"
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); printf '  ok    %s\n' "$1"; }
bad() { FAIL=$((FAIL+1)); printf '  FAIL  %s\n' "$1"; [[ -n "${2:-}" ]] && printf '        %s\n' "$2"; }
cleanup() { docker rm -f "$C" "$C-b" "$C-c" >/dev/null 2>&1; docker volume rm "$V" "$V-b" >/dev/null 2>&1; docker rmi "$IMG" >/dev/null 2>&1; docker run --rm -v "$W":/w alpine:3 rm -rf /w/media >/dev/null 2>&1; rm -rf "$W"; }
trap cleanup EXIT
json() { python3 -c "import json,sys; d=json.load(sys.stdin); print(eval(sys.argv[1]))" "$1"; }
wait_up() { for _ in $(seq 1 80); do curl -fsS "http://127.0.0.1:$1/api/v1/health" >/dev/null 2>&1 && return 0; sleep 0.5; done; return 1; }
port_of() { docker port "$1" 8787/tcp | head -1 | sed 's/.*://'; }

echo "Building the image"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
if docker build -q --build-arg VERSION="$VERSION" -t "$IMG" . >"$W/build.log" 2>&1; then ok "the image builds"; else bad "the image builds" "$(tail -5 "$W/build.log")"; exit 1; fi
[[ "$(docker image inspect "$IMG" --format '{{index .Config.Labels "org.opencontainers.image.version"}}')" == "$VERSION" ]] && ok "it is labelled with version $VERSION" || bad "image version label"
docker image inspect "$IMG" --format '{{.Size}}' | awk '{printf "  info  image size %.0f MB\n", $1/1048576}'

echo; echo "First start (the user's own uid and gid, a fresh data volume)"
docker run --rm --entrypoint ffmpeg -v "$MEDIA":/m "$IMG" -hide_banner -loglevel error -y -f lavfi -i "testsrc=size=320x180:rate=25:duration=3" -f lavfi -i "sine=frequency=500:duration=3" -c:v libx264 -pix_fmt yuv420p -c:a aac -shortest /m/bump.mp4
docker run -d --name "$C" -p 127.0.0.1::8787 -e PUID="$(id -u)" -e PGID="$(id -g)" -e TZ=America/Toronto -v "$V":/data -v "$MEDIA":/media "$IMG" >/dev/null
P="$(port_of "$C")"; H="http://127.0.0.1:$P"
wait_up "$P" && ok "the server answers on its port" || { bad "the server did not start" "$(docker logs "$C" 2>&1 | tail -5)"; exit 1; }
TOKEN="$(docker logs "$C" 2>&1 | sed -n 's/.*access token (first run): \([0-9a-f]\{32\}\).*/\1/p' | head -1)"
[[ -n "$TOKEN" ]] && ok "it printed a first-run access token" || bad "no token in the logs" "$(docker logs "$C" 2>&1 | head -8)"
[[ "$(curl -s -o /dev/null -w '%{http_code}' "$H/api/v1/channels")" == 401 ]] && ok "so it is not open to the network by accident" || bad "an unprotected server"
auth=(-H "Authorization: Bearer $TOKEN")
[[ "$(curl -s -o /dev/null -w '%{http_code}' "${auth[@]}" "$H/api/v1/channels")" == 200 ]] && ok "the token works" || bad "token refused"
[[ "$(curl -s "${auth[@]}" "$H/api/v1/security" | json "d['locked']")" == False ]] && ok "and can be turned off from Settings" || bad "token is locked"
SRV_UID="$(docker exec "$C" sh -c 'for p in /proc/[0-9]*; do [ "$(cat "$p/comm" 2>/dev/null)" = tvmaster ] && awk "/^Uid:/ {print \$2}" "$p/status"; done' | head -1)"
[[ "$SRV_UID" == "$(id -u)" ]] && ok "the server runs as your user, not root" || bad "the server runs as uid '$SRV_UID'"
curl -fsS "$H/admin/" | grep -qi '<div id="root"' && ok "the web admin is served" || bad "admin UI"

echo; echo "Uploading a video from a remote app"
LIB="$(curl -fsS "${auth[@]}" -H 'content-type: application/json' -d '{"path":"/media/bumpers","kind":"bumpers","create":true}' "$H/api/v1/libraries" | json "d['id']")"
[[ -n "$LIB" ]] && ok "a library folder is made on the server" || bad "create a library"
curl -fsS "${auth[@]}" -XPUT --data-binary "@$MEDIA/bump.mp4" "$H/api/v1/libraries/$LIB/files?path=Retro/Bump%201.mp4" | json "d['indexed']" | grep -q True && ok "the upload is stored and indexed" || bad "upload"
[[ -f "$MEDIA/bumpers/Retro/Bump 1.mp4" ]] && ok "the file is in the mounted folder" || bad "file not on the host"
[[ "$(stat -c %u "$MEDIA/bumpers/Retro/Bump 1.mp4" 2>/dev/null || stat -f %u "$MEDIA/bumpers/Retro/Bump 1.mp4")" == "$(id -u)" ]] && ok "owned by you on the host" || bad "wrong owner"
[[ "$(curl -s "${auth[@]}" "$H/api/v1/libraries" | json "d[0]['items']")" == 1 ]] && ok "the library counts it" || bad "library item count"

echo; echo "A channel streams"
curl -fsS "${auth[@]}" -H 'content-type: application/json' -d '{"id":"bars","number":2,"name":"Bars","kind":"test","options":{"off_air":{"screen":"bars","tone":true}}}' "$H/api/v1/channels" >/dev/null
PL=""; for _ in $(seq 1 30); do PL="$(curl -fsS "${auth[@]}" "$H/stream/bars/index.m3u8" 2>/dev/null)"; echo "$PL" | grep -q EXTINF && break; sleep 1; done
echo "$PL" | grep -q EXTINF && ok "a test-pattern channel streams HLS from inside the container" || bad "no stream" "$(docker logs "$C" 2>&1 | tail -3)"
curl -fsS "$H/iptv/playlist.m3u?token=$TOKEN" | grep -q "^#EXTM3U" && ok "the IPTV playlist is served" || bad "playlist"

echo; echo "Restart keeps everything"
docker restart "$C" >/dev/null; wait_up "$P" || { P="$(port_of "$C")"; H="http://127.0.0.1:$P"; wait_up "$P"; }
[[ "$(curl -s -o /dev/null -w '%{http_code}' "${auth[@]}" "http://127.0.0.1:$P/api/v1/channels")" == 200 ]] && ok "the same token still works" || bad "token lost on restart"
[[ "$(curl -s "${auth[@]}" "http://127.0.0.1:$P/api/v1/libraries" | json "d[0]['items']")" == 1 ]] && ok "the library and its file are still there" || bad "data lost on restart"
[[ "$(docker logs "$C" 2>&1 | grep -c 'access token (first run)')" == 1 ]] && ok "the token is only printed once" || bad "token printed again"
for _ in $(seq 1 40); do [[ "$(docker inspect -f '{{.State.Health.Status}}' "$C")" == healthy ]] && break; sleep 2; done
[[ "$(docker inspect -f '{{.State.Health.Status}}' "$C")" == healthy ]] && ok "Docker reports it healthy" || bad "health: $(docker inspect -f '{{.State.Health.Status}}' "$C")"
docker stop -t 20 "$C" >/dev/null; [[ "$(docker inspect -f '{{.State.ExitCode}}' "$C")" == 0 ]] && ok "it stops cleanly" || bad "unclean stop: exit $(docker inspect -f '{{.State.ExitCode}}' "$C")"

echo; echo "Other ways to start it"
docker run -d --name "$C-b" -p 127.0.0.1::8787 -e TVMASTER_TOKEN=a-fixed-token-12345 -v "$V-b":/data "$IMG" >/dev/null
PB="$(port_of "$C-b")"; wait_up "$PB"
[[ "$(curl -s -H 'Authorization: Bearer a-fixed-token-12345' "http://127.0.0.1:$PB/api/v1/security" | json "d['locked']")" == True ]] && ok "TVMASTER_TOKEN fixes the token (locked)" || bad "fixed token"
docker rm -f "$C-b" >/dev/null
docker run -d --name "$C-c" -p 127.0.0.1::8787 -e TVMASTER_NO_FIRST_TOKEN=1 "$IMG" >/dev/null
PC="$(port_of "$C-c")"; wait_up "$PC"
[[ "$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$PC/api/v1/channels")" == 200 ]] && ok "TVMASTER_NO_FIRST_TOKEN starts it open" || bad "open start"
docker rm -f "$C-c" >/dev/null
TOK2="$(docker run --rm "$IMG" token)"; [[ "$TOK2" =~ ^[0-9a-f]{32}$ ]] && ok "other commands work: docker run tvmaster token" || bad "tvmaster token"
DOC="$(docker run --rm "$IMG" doctor 2>&1)"; echo "$DOC" | grep -q "ffprobe works" && ok "and doctor sees FFmpeg" || bad "doctor"

printf '\n%d passed, %d failed\n' "$PASS" "$FAIL"
exit "$FAIL"
