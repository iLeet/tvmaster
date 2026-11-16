# Running TVMaster headless (no screen, no app)

TVMaster is a server first. The Mac app, the web admin and your phone are just ways to look at it, so it runs happily on a box in a cupboard. This page is how to start it, how to tell whether it is running, how to find the access token, and how to look after it.

Everything below can be done from another computer: open `http://<the server's address>:8787/admin/` (the **web admin**) or point the Mac app at it (**Settings → Server**; the app also finds servers on your network by itself). **Live control** in both takes channels, or the whole station, to the test screen or an emergency alert, and starts, stops and restarts streams. **Settings → Restart the server** restarts it.

## Is it running?

| How you installed it | Check |
|---|---|
| Docker | `docker ps` (look for `tvmaster ... healthy`), or `docker compose ps` in its folder |
| Linux package (.deb, .rpm, Arch) | `systemctl status tvmaster` |
| Windows installer | Task Scheduler → *TVMaster*, or open `http://localhost:8787/admin/` |
| Started by hand | `curl http://localhost:8787/api/v1/health` |

The health address answers without a token. `{"service":"tvmaster","status":"ok",...}` means it is up. It also lists `on_air`, the channels streaming right now (channels stream only while someone watches, unless a channel is set to *Always on*).

## Starting, stopping, restarting

| | Start | Stop | Restart | Logs |
|---|---|---|---|---|
| Docker | `docker compose up -d` | `docker compose stop` | `docker compose restart` | `docker compose logs -f tvmaster` |
| Linux package | `sudo systemctl start tvmaster` | `sudo systemctl stop tvmaster` | `sudo systemctl restart tvmaster` | `journalctl -u tvmaster -f` |
| By hand | `tvmaster serve --data-dir ~/tvmaster-data` | Ctrl-C | stop, start | on screen |

Docker and the Linux package start the server again by themselves after a reboot (`restart: unless-stopped`, `Restart=always`). To start it at boot as well on Linux: `sudo systemctl enable tvmaster` (the package does this for you).

Restarting from the admin (**Settings → Restart the server**, or `POST /api/v1/server/restart`) only works when something will start it again: Docker, systemd or the Windows task all do. A server you started by hand just stops.

## Seeing the access token

The first time the server starts it makes a token so it is never open to your whole network by accident, and the admin asks for it. You can change it, or turn it off, in **Settings → Access token**, and you can let people **watch without a token** while managing still needs one (the same page).

The server saves the token in its data folder. To see what it is **right now**, even if the server is stopped, run `tvmaster access` against that data folder:

| How you installed it | Command |
|---|---|
| Docker | `docker exec tvmaster tvmaster access` |
| Linux package | `sudo tvmaster access --data-dir /var/lib/tvmaster` |
| By hand | `tvmaster access --data-dir <the folder you gave it>` |
| Windows | `"C:\Program Files\TVMaster\tvmaster.exe" access --data-dir "C:\ProgramData\TVMaster"` in an administrator PowerShell |

It prints `Access token: ...`, or says that no token is saved (the server is open). If the server was started with `--token` (or `TVMASTER_TOKEN`), that fixed token is what counts and the admin cannot change it; `tvmaster access` then shows what the admin last saved, which may differ.

The one-time token that Docker and the Linux package make is also kept in a file (`/data/first-run-token`, `/etc/tvmaster/token`). It is only a seed: once you have changed the token in Settings, use `tvmaster access`.

**Lost it?** Stop the server and start it with a new one: `tvmaster token` prints a fresh random token; start the server with `--token <it>` (Docker: `TVMASTER_TOKEN=<it>` in `.env`). Or turn the token off for good by signing in with any working token and switching it off in Settings.

## A first look from another computer

1. Find the server's address: `hostname -I` on Linux, or `docker compose logs tvmaster | head`. It is usually `http://<name or ip>:8787`.
2. Open `<address>/admin/`, or in the Mac app **Settings → Server → Look for servers**.
3. Sign in with the token above.
4. **Libraries** → add the folders your shows are in (inside Docker, the folder you mounted as `/media`).
5. **Channels** → make a channel, or use **Live control** to check the stream.

If a library is on a drive that is not always connected, TVMaster remembers the setup: files on a missing drive are marked missing and come back when the drive does, and the schedule is planned again.

## Taking over the station

**Live control** (web admin and Mac app) is the operator's panel:

- **All to test screen / off air / black / snow**: every channel, the guide included, in one press.
- **Emergency alert**: a flashing alert screen with the event, your message, a crawl, and the two-tone attention signal at the start. Samples are provided (an EAS-style required test, a civil emergency, a tornado warning, a technical-difficulties card); the samples say they are tests. Choose the channels and how long (until cleared, or 5 minutes to an hour).
- **Resume**: back to programming, one channel or all.
- Per channel: start, stop and restart its stream.

A takeover is saved, so a restart does not quietly bring the programming back. From a script:

```bash
# every channel to the test screen
curl -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"channels":["all"],"mode":"test_screen"}' http://server:8787/api/v1/live
# a sample alert on one channel for ten minutes
curl -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"channels":["disney-jr"],"mode":"alert","sample":"eas_test","minutes":10}' http://server:8787/api/v1/live
# put everything back
curl -H "Authorization: Bearer $TOKEN" -X POST http://server:8787/api/v1/live/clear
```

## Backups

Everything that is yours is in the data folder (`/data` in Docker, `/var/lib/tvmaster` for the package): the database (channels, lineups, settings, how far each show has got), uploaded pictures and the HTTPS certificate. Copy it while the server is stopped. Your media files are never changed by TVMaster.
