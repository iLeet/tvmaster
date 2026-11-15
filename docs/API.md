# TVMaster API (v0)

Base URL: `http://<host>:8787`. JSON unless noted. Authentication is not implemented yet (LAN-open); a bearer token and remote-access guidance come with the web UI milestone.

Errors are `{"error": "message"}` with a suitable status code.

## Streams (TVs and IPTV clients)

| Method | Path | Status | Notes |
|---|---|---|---|
| GET | `/stream/{channel}/index.m3u8` | done | HLS. First request puts the channel on air; many clients share one encode. |
| GET | `/stream/{channel}/seg_{n}.ts` | done | HLS segments. |
| GET | `/iptv/playlist.m3u` | done | All channels with `tvg-id`, `tvg-chno`; uses the request's Host. |
| GET | `/iptv/guide.xml` | done | XMLTV, next 24 hours. |
| GET | `/discover.json`, `/lineup.json`, `/lineup_status.json` | planned (M5) | HDHomeRun emulation for Plex/Jellyfin/Emby. |
| GET | `/stream/{channel}.ts` | planned | Raw MPEG-TS. |
| GET | `/tv/{tv_id}.m3u8` | planned | Per-TV URL that follows that TV's channel. |

## Libraries

| Method | Path | Notes |
|---|---|---|
| GET | `/api/v1/libraries` | List with item counts. |
| POST | `/api/v1/libraries` | `{"path": "/media/tv", "kind": "shows"}`. Kinds: `shows`, `commercials`, `bumpers`, `music`. The path must exist on the server. |
| POST | `/api/v1/libraries/{id}/scan` | Probes every video. Returns `{found, indexed, skipped}`. Files that vanished are flagged missing, not deleted. Channels using the library rebuild on next tune-in. |

Only `shows` libraries feed channels today; commercials and bumpers are stored for later milestones.

## Channels and guide

| Method | Path | Notes |
|---|---|---|
| GET | `/api/v1/channels` | List with `on_air`, `now` and `next`. |
| POST | `/api/v1/channels` | Create or update. Body: `id` (a-z0-9-_), `number`, `name`, optional `strategy` (`sequential`, `shuffle`, `chaos`, `block_grid`), `run_out`, `autopilot`, `chaos`, `seed`, `always_on`, `library_ids`. Updating keeps the channel's epoch and seed so the programme does not jump. |
| GET | `/api/v1/channels/{id}` | One channel. |
| DELETE | `/api/v1/channels/{id}` | |
| GET | `/api/v1/channels/{id}/schedule?hours=6` | Upcoming programmes (max 48 h). |

M2 note: `block_grid` currently behaves like `sequential`, and `chaos`/`shuffle` are the same seeded shuffle. Real grids, the chaos slider and run-out policies arrive in M3.

## Virtual TVs and remote

A TV is a registered player with a stable id; the remote changes its channel. Different TVs watch different channels at the same time. State is persisted.

| Method | Path | Notes |
|---|---|---|
| GET | `/api/v1/tvs` | All TVs. |
| POST | `/api/v1/tvs` | Register `{"id": "living-room", "name": "Living Room"}`. Re-registering keeps channel and volume. |
| GET | `/api/v1/tvs/{id}` | `{id, name, channel, power, volume, muted}`. |
| POST | `/api/v1/tvs/{id}/channel` | `{"number": 4}` or `{"step": "up"\|"down"}` (steps through real channels, wraps). Turns the TV on. 404 for unknown channel numbers. |
| POST | `/api/v1/tvs/{id}/power` | `{"on": true}`. |
| WS | `/api/v1/ws` | planned (M5): push `tv.state` / `channel.now` instead of polling. The web player polls every 1.5 s for now. |
| POST | `/api/v1/tvs/{id}/volume` | planned. |

## Web pages

| Path | Purpose |
|---|---|
| `/` | Watch locally: channel list, now/next, arrow keys to change channel, `F` for fullscreen. |
| `/?tv=living-room` | Full-screen TV that registers itself and follows its remote. |
| `/?remote=living-room` | Phone-friendly remote for that TV. |

## Ops

| Method | Path | Notes |
|---|---|---|
| GET | `/api/v1/health` | Status, version, FFmpeg version and encoder, channels currently on air. |
