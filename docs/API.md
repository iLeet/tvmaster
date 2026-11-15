# TVMaster API (draft v0)

Base URL: `http://<host>:8787`. JSON everywhere unless noted. Optional bearer-token auth (`Authorization: Bearer <token>`) when enabled; LAN-open by default.

## Streams (consumed by TVs and IPTV clients)

| Method | Path | Notes |
|---|---|---|
| GET | `/stream/{channel}/index.m3u8` | HLS for one channel. Many clients share one encode. |
| GET | `/stream/{channel}.ts` | Raw MPEG-TS (VLC, Plex, Kodi). |
| GET | `/iptv/playlist.m3u` | All channels, with `tvg-id`, `tvg-chno`, logos. |
| GET | `/iptv/guide.xml` | XMLTV EPG. |
| GET | `/discover.json`, `/lineup.json`, `/lineup_status.json` | HDHomeRun emulation. |
| GET | `/tv/{tv_id}.m3u8` | (later) stable per-TV stream that follows that TV's channel. |

## Channels and guide

| Method | Path | Notes |
|---|---|---|
| GET | `/api/v1/channels` | List channels with now/next. |
| GET | `/api/v1/channels/{id}` | Channel detail and config. |
| GET | `/api/v1/channels/{id}/schedule?from=&to=` | Materialized timeline. |
| POST | `/api/v1/channels/{id}/reroll` | Re-roll from now (respects Manual lock). |
| GET | `/api/v1/guide?from=&hours=` | Grid data used by Channel 3 and the web app. |

## Virtual TVs and Remote

A TV is a registered player with a stable id. The remote changes its channel; many TVs can each watch a different channel simultaneously.

| Method | Path | Notes |
|---|---|---|
| GET | `/api/v1/tvs` | Registered TVs, current channel, online state. |
| POST | `/api/v1/tvs` | Register a TV `{id, name}`. |
| GET | `/api/v1/tvs/{id}` | State: `{channel, power, volume, muted}`. |
| POST | `/api/v1/tvs/{id}/channel` | `{number}` or `{id}` or `{step: "up"|"down"}`. |
| POST | `/api/v1/tvs/{id}/power` | `{on: bool}`. |
| POST | `/api/v1/tvs/{id}/volume` | `{level}` or `{muted}`. |
| WS | `/api/v1/ws` | Subscribe to `tv.state`, `channel.now`, `schedule.changed`; players send `tv.hello`/heartbeat. |

## Library, scheduling, ops

| Method | Path | Notes |
|---|---|---|
| GET/POST | `/api/v1/libraries` | Media/commercial/bumper folders. |
| POST | `/api/v1/libraries/{id}/scan` | Trigger scan. |
| GET/POST | `/api/v1/channels` | Create/update channels, strategies, run-out policy, autopilot flag. |
| GET | `/api/v1/health` | Liveness, ffmpeg status, per-channel health. |
| GET | `/api/v1/events` | Server-sent events fallback for non-WS clients. |
