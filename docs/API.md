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
| POST | `/api/v1/libraries/{id}/analyze` | Find ad-break points (black-frame scan) for a `shows` library's files that have none. Runs in the background; returns `202 {"queued": n}`. Starts automatically after a scan. Progress appears in the library list as `analysis: {total, done, running}`. |
| POST | `/api/v1/libraries/{id}/scan` | Probes every video. Returns `{found, indexed, skipped}`. Files that vanished are flagged missing, not deleted (a restored file comes back on the next scan). Autopilot channels using the library re-roll. |

Only `shows` libraries feed channels directly. `commercials` and `bumpers` libraries are named in a lineup's `ads` section: see [ADS.md](ADS.md).

## Channels, lineups and the schedule

See [SCHEDULING.md](SCHEDULING.md) for what these mean.

| Method | Path | Notes |
|---|---|---|
| GET | `/api/v1/channels` | List with `on_air`, `now` and `next`. |
| POST | `/api/v1/channels` | Create or update. Body: `id` (a-z0-9-_), `number`, `name`, optional `strategy` (`sequential`, `shuffle`, `chaos`, `block_grid`), `run_out` (`keep_going`, `next_in_rotation`, `chaos_fill`, `hybrid`), `autopilot`, `chaos` (0-100), `seed`, `always_on`, `library_ids`, `lineup`. Updating keeps the channel's epoch and seed. A lineup with no `timezone` gets the server's. Invalid lineups are rejected with a reason. |
| GET | `/api/v1/channels/{id}` | One channel. |
| DELETE | `/api/v1/channels/{id}` | |
| GET | `/api/v1/channels/{id}/lineup` | The lineup. |
| PUT | `/api/v1/channels/{id}/lineup` | Replace the lineup. On an autopilot channel the future is re-rolled. |
| POST | `/api/v1/channels/{id}/reroll` | Regenerate the not-yet-aired schedule now (works on manual-lock channels). Returns `{"items": n}`. |
| GET | `/api/v1/channels/{id}/schedule?hours=6` | Programmes (max 72 h). `end_ms` is the slot end (includes ad breaks and padding), `runtime_end_ms` is when the show's last act ends, `label` is the block or event. Add `&detail=items` for the raw timeline: every show act (`in_ms` is where in the file it starts), commercial, bumper (`bumper`: `brb`, `back_to`, `up_next`, `later_today`, `station_id`; none for your own clips) and filler. |
| GET | `/api/v1/channels/{id}/cursors` | Episode cursors: `committed` have aired, `reserved` are on the schedule. |

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

The daemon also runs an autopilot loop (every minute) that tops up every channel's schedule to 72 hours, commits aired episodes and trims history. There is nothing to call.
