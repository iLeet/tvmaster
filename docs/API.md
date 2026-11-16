# TVMaster API (v0)

Base URL: `http://<host>:8787`. JSON unless noted.

## Authentication

By default the server is open on the network. The access token is optional and can be switched on, off or replaced while the server runs (`PUT /api/v1/security`, below); `tvmaster serve --token <token>` (or `TVMASTER_TOKEN`; `tvmaster token` prints a random one) fixes it instead. When it is on, every request must carry the token, except the admin UI's static files, the web player shell, `/api/v1/health` and the auth endpoints. Send it as:

- `Authorization: Bearer <token>` (scripts),
- `?token=<token>` (IPTV apps that cannot set headers: playlists, tuner lineups and HLS playlists include it in the URLs they hand out), or
- the `tvmaster_token` cookie that `POST /api/v1/auth/login` sets (browsers; HTTP-only, same-site).

| Method | Path | Notes |
|---|---|---|
| GET | `/api/v1/auth` | `{required, authenticated}`. |
| POST | `/api/v1/auth/login` | `{"token": "…"}`: sets the cookie. Wrong tokens get 401 after a short delay. |
| POST | `/api/v1/auth/logout` | Clears the cookie. |
| GET | `/api/v1/security` | `{enabled, locked, token}`. `locked` means the token was given on the command line (`--token`, `--token-file`), so it cannot be changed here. Needs the token itself when one is on. |
| PUT | `/api/v1/security` | `{"enabled": true}` makes a random token, `{"enabled": true, "token": "…"}` sets yours, `{"enabled": true, "rotate": true}` replaces it, `{"enabled": false}` switches it off. Returns the new state and sets (or clears) the caller's cookie. 409 when locked, 400 for a token that is not 8-128 characters of `A-Za-z0-9-_.~`. The choice is kept in the database across restarts. `tvmaster serve --initial-token-file F` starts with the token in `F` the first time only (what the packages do), after which the saved choice wins. |

Tokens are 8-128 characters of letters, digits, `-`, `_`, `.`, `~`. Use HTTPS (a reverse proxy, or Tailscale) before exposing the server beyond your home network.

Errors are `{"error": "message"}` with a suitable status code.

### Pictures

| Method | Path | Notes |
|---|---|---|
| POST | `/api/v1/assets?name=logo.png` | The picture as the request body (png, jpg, gif or webp, up to 12 MB; recognised by its first bytes, not its claimed type). Returns `{id, path, url, bytes}`. `path` is where it is on the server: put it in a channel's `options.off_air.image`, `options.overlay.bug.image` or a guide's `header.logo`. The same bytes uploaded twice are one file. |
| GET | `/api/v1/assets` | What has been uploaded, newest first. |
| GET | `/api/v1/assets/{id}` | The picture itself. |
| DELETE | `/api/v1/assets/{id}` | Remove it (a channel still pointing at it falls back to its station card). |

Uploads are kept in `<data dir>/assets/`.

### Monitoring

| Method | Path | Notes |
|---|---|---|
| GET | `/api/v1/monitor` | `{settings, alerts, rescans, curl}`: the settings below, the problems right now, when each library was last re-read and what changed, and whether `curl` (used for the webhook) is installed. |
| PUT | `/api/v1/monitor` | `{"watch_minutes": 15, "alerts": true, "webhook": "https://ntfy.sh/topic", "webhook_text": false, "notify_recovery": true}`. `watch_minutes` is 0 (only on request) or 5, 10, 15, 30, 60, 120, 360. |
| POST | `/api/v1/monitor/rescan` | Re-read every library's folder now: `{added, removed, back, unreachable}`. Only files that are new, gone or back are touched; a new file waits until it has not changed for a minute; a folder that cannot be reached, or comes up empty, is left alone. |
| POST | `/api/v1/monitor/test` | Send a test message to the webhook. |

Alerts (`{id, severity, title, detail, since_ms}`) are raised when a condition has held for two checks 30 seconds apart: a library whose folder is unreachable (`library:<id>:offline`), a channel whose libraries have nothing playable (`channel:<id>:empty`), a channel whose playout failed three times in ten minutes (`channel:<id>:failing`), FFmpeg missing (`system:ffmpeg`, immediately). The WebSocket sends `alert.raised` (with `alert`), `alert.cleared` (with `id`) and `library.changed` (`library`, `added`, `removed`, `back`). The webhook gets a POST: JSON with `title`, `message`, `severity`, `id` and Slack's `text` and Discord's `content`, or with `webhook_text` the message as plain text and the title in a `Title` header (ntfy).

### HTTPS

`tvmaster serve --tls` adds an HTTPS listener (default `0.0.0.0:8788`) next to the HTTP one; both serve the same API. Over HTTPS the login cookie is `Secure`, and links the server writes (M3U, HDHomeRun, XMLTV) say `https://`. Behind a reverse proxy that sets `X-Forwarded-Proto: https` the same happens. `GET /api/v1/system` reports `network`: `hostname`, `hosts` (names then IPv4 addresses, for building links), `http_port`, `https_port` and `tls` (`enabled`, `self_signed`, `fingerprint` as `AA:BB:...` SHA-256 of the certificate).

## Streams (TVs and IPTV clients)

| Method | Path | Status | Notes |
|---|---|---|---|
| GET | `/stream/{channel}/index.m3u8` | done | HLS. First request puts the channel on air; many clients share one encode. |
| GET | `/stream/{channel}/seg_{n}.ts` | done | HLS segments. |
| GET | `/stream/{channel}.ts` | done | The channel as one continuous MPEG-TS stream, for tuner-style clients (Plex, Jellyfin, Emby, VLC). Starts a few seconds behind live; the channel stays on air while the connection is open. |
| GET | `/iptv/playlist.m3u` | done | All channels with `tvg-id`, `tvg-chno` and `group-title` (the channel's group, default "TVMaster"); uses the request's Host. |
| GET | `/iptv/guide.xml` | done | XMLTV, next 24 hours. |
| GET | `/discover.json`, `/lineup.json`, `/lineup_status.json`, `/device.xml`, `/lineup.post` | done | HDHomeRun-compatible tuner API, so Plex, Jellyfin and Emby can add TVMaster as a network tuner. `TunerCount` is the number of channels (they share encodes). |
| GET | `/tv/{tv_id}.m3u8` | done | Redirects to the stream of the channel that TV is tuned to *when it is opened* (409 if the TV is off). For plain IPTV clients; to follow a remote live use the web player or the WebSocket API. |

## Libraries

| Method | Path | Notes |
|---|---|---|
| GET | `/api/v1/libraries` | List with item counts. |
| POST | `/api/v1/libraries` | `{"path": "/media/tv", "kind": "shows"}`. Kinds: `shows`, `commercials`, `bumpers`, `music`. The path must exist on the server. |
| POST | `/api/v1/libraries/{id}/analyze` | Find ad-break points (black-frame scan) for a `shows` library's files that have none. Runs in the background; returns `202 {"queued": n}`. Starts automatically after a scan. Progress appears in the library list as `analysis: {total, done, running}`. |
| POST | `/api/v1/libraries/{id}/scan` | Probes every video. Returns `{found, indexed, skipped}`. Files that vanished are flagged missing, not deleted (a restored file comes back on the next scan). Autopilot channels using the library re-roll. |

| PATCH | `/api/v1/libraries/{id}` | `{"enabled": bool}`: turn a library off (its files stop being scheduled) or on. |
| DELETE | `/api/v1/libraries/{id}` | Remove a library and its index (files on disk are untouched). Channels using it are rescheduled. |
| GET | `/api/v1/shows?library_ids=1,2` | Distinct show names with episode counts and airtime (all shows libraries if no ids). |
| GET | `/api/v1/fs/list?path=` | The folders (names only, no files) inside a folder on the server, for the folder picker. Defaults to the home folder. |

Only `shows` libraries feed channels directly. `commercials` and `bumpers` libraries are named in a lineup's `ads` section: see [ADS.md](ADS.md).

## Channels, lineups and the schedule

See [SCHEDULING.md](SCHEDULING.md) for what these mean.

| Method | Path | Notes |
|---|---|---|
| GET | `/api/v1/channels` | List with `on_air`, `now` and `next`. |
| POST | `/api/v1/channels` | Create or update. Body: `id` (a-z0-9-_), `number`, `name`, optional `strategy` (`sequential`, `shuffle`, `chaos`, `block_grid`), `run_out` (`keep_going`, `next_in_rotation`, `chaos_fill`, `hybrid`), `autopilot`, `chaos` (0-100), `seed`, `always_on`, `library_ids`, `lineup`, `group`, `kind` (`programming` or `guide`), `guide`. **Updates are partial:** fields you leave out keep their values. Updating keeps the channel's epoch and seed. A lineup with no `timezone` gets the server's. Invalid lineups are rejected with a reason. |
| GET | `/api/v1/channels/{id}` | One channel. |
| DELETE | `/api/v1/channels/{id}` | |
| GET | `/api/v1/channels/{id}/lineup` | The lineup. |
| PUT | `/api/v1/channels/{id}/lineup` | Replace the lineup. On an autopilot channel the future is re-rolled. |
| POST | `/api/v1/channels/{id}/reroll` | Regenerate the not-yet-aired schedule now (works on manual-lock channels). Returns `{"items": n}`. |
| GET | `/api/v1/channels/{id}/schedule?hours=6` | Programmes (max 72 h). `end_ms` is the slot end (includes ad breaks and padding), `runtime_end_ms` is when the show's last act ends, `label` is the block or event. Add `&detail=items` for the raw timeline: every show act (`in_ms` is where in the file it starts), commercial, bumper (`bumper`: `brb`, `back_to`, `up_next`, `later_today`, `station_id`; none for your own clips) and filler. |
| GET | `/api/v1/channels/{id}/cursors` | Episode cursors: `committed` have aired, `reserved` are on the schedule. |

### Picture, sound and sign-off

A channel's `options` object holds audio-track, subtitle, picture and on-screen-graphics settings, and `lineup.signoff` the off-the-air hours; both are described in [CHANNEL-OPTIONS.md](CHANNEL-OPTIONS.md). `POST /api/v1/channels` replaces `options` as a whole when it is sent and keeps it when it is not (invalid values give 400 with a reason). `GET /api/v1/shows` includes `audio_languages`, `subtitle_languages` (three-letter codes found in the show's files) and `files_with_tracks` (files whose tracks have been read; fewer than `episodes` means a re-scan is due). `PUT /api/v1/show-settings` with `{ "show": "Cowboy Bebop", "settings": { "audio_languages": ["jpn"], "subtitles": "always" } }` sets what follows that show onto every channel (empty `settings` clears it); `GET /api/v1/show-settings` lists them all, and each row of `GET /api/v1/shows` carries its `defaults`. A channel of `kind: "test"` airs only `options.off_air` (`screen`: `card`, `bars`, `static`, `black`, `image`; `message`; `image`; `tone`). `GET /api/v1/system` reports `ffmpeg.has_subtitles` (text subtitles can be drawn) next to `has_drawtext`.

## Groups and the guide channel

- `group` (up to 40 characters) organises channels ("Kids", "Movies"). It becomes the playlist `group-title` and a heading in the web player and remote.
- `kind: "guide"` makes the channel a **live guide** (Channel 3 in the classic setup): a Prevue-style scrolling grid of every programming channel, with a clock, a "now on" ticker and optional music. It is drawn live, needs no libraries, and is not scheduled.

```json
{ "id": "guide", "number": 3, "name": "Retro TV", "kind": "guide",
  "guide": { "hours": 3, "title": "Retro TV Guide",
             "ticker": ["Welcome to Retro TV - all day, all night"],
             "music_library_ids": [4] } }
```

`hours` is 2-6. `music_library_ids` are libraries of kind `music` (audio files are scanned like video; the music loops under the guide, silence if none). The ticker announces what is on now on every channel after your own lines.

The guide also has themes, layouts, a heading, a ticker, a chosen list of channels and a live video window, all in the same `guide` object and all optional; see [GUIDE.md](GUIDE.md). `POST /api/v1/guide/preview` with `{"id": "guide", "name": "Retro", "guide": {…}, "t": 3}` returns one frame as a PNG without saving anything (the editors use it for their live preview). Changing a guide's settings restarts it so the change shows.

## Virtual TVs and remote

A TV is a registered player with a stable id; the remote changes its channel. Different TVs watch different channels at the same time. State is persisted.

| Method | Path | Notes |
|---|---|---|
| GET | `/api/v1/tvs` | All TVs. |
| POST | `/api/v1/tvs` | Register `{"id": "living-room", "name": "Living Room"}`. Re-registering keeps channel and volume. |
| GET | `/api/v1/tvs/{id}` | `{id, name, channel, power, volume, muted, online}`. `online` is true while a player is connected over the WebSocket. |
| POST | `/api/v1/tvs/{id}/channel` | `{"number": 4}` or `{"step": "up"\|"down"}` (steps through real channels, wraps). Turns the TV on. 404 for unknown channel numbers. |
| POST | `/api/v1/tvs/{id}/power` | `{"on": true}`. |
| DELETE | `/api/v1/tvs/{id}` | Remove a TV (`tv.removed` is pushed). |
| POST | `/api/v1/tvs/{id}/volume` | `{"level": 0-100}` and/or `{"muted": bool}`. Raising the volume un-mutes. |
| WS | `/api/v1/ws` | Live updates and remote control: see below. |

### WebSocket

Connect to `/api/v1/ws`. The server sends a `snapshot` (`tvs`, `channels`) and then pushes events as they happen:

| Event | When |
|---|---|
| `tv.state` `{tv}` | A TV's channel, power, volume or online state changed. |
| `channel.now` `{channel, now, next}` | A channel moved on to its next programme. |
| `schedule.changed` `{channel}` | A channel's schedule was re-rolled. |

Messages you can send:

| Message | Effect |
|---|---|
| `{"type":"hello","tv":"living-room"}` | This socket is that TV's player. The TV counts as online while it stays connected (and is created if it did not exist). |
| `{"type":"tv","id":"living-room","channel":4}` | Control a TV. Fields (any combination): `channel`, `step` (`up`/`down`), `power`, `volume`, `muted`. Errors come back as `{"type":"error","message":…}`. |
| `{"type":"ping"}` | Answered with `{"type":"pong"}`. |

The web player (`/?tv=…` and `/?remote=…`) uses this and falls back to polling the REST API if the socket is unavailable.

## Web pages

| Path | Purpose |
|---|---|
| `/` | Watch locally: channel list, now/next, arrow keys to change channel, `F` for fullscreen. |
| `/?tv=living-room` | Full-screen TV that registers itself and follows its remote. |
| `/?remote=living-room` | Phone-friendly remote for that TV: channel up/down, volume, mute, power, and a list of channels grouped by group. Shows whether the TV's player is connected. |

## The admin UI

`/admin/` is the admin web app (React + TypeScript, compiled into the server binary): dashboard, libraries with a folder picker, channels with a visual lineup editor (blocks, seasons, events, rotations), ads and bumpers, the guide channel, a schedule view with breaks and bumpers, TVs, and settings. `/admin/#/remote` is the installable phone remote. It uses the same API documented here.

## Ops

| Method | Path | Notes |
|---|---|---|
| GET | `/api/v1/health` | Status, version, FFmpeg version and encoder, channels currently on air (public even with a token). |
| GET | `/api/v1/system` | Version, OS/arch, time zone, FFmpeg capabilities (`encoder` is the one in use: `software`, `videotoolbox`, `nvenc`, `qsv`, `vaapi` or `amf`; `encoders` lists every H.264 encoder the build has), whether a token is required. |

The daemon also runs an autopilot loop (every minute) that tops up every channel's schedule to 72 hours, commits aired episodes and trims history. There is nothing to call.
