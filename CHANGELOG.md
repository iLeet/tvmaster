# Changelog

## 1.0.0

The first stable release.

- **Guide channel:** many more options. Smooth modern lettering at 1280x720 next to the retro pixel look, rounded or square cells, text size, a picture or striped or solid background, a minimal heading, a ticker at the top or bottom, times on programmes, six more colour themes and a custom background colour, one-click looks, a live video window (one channel or flipping through several), and a live preview in the web admin and the Mac app.
- **Upload videos to the server** (bumpers, commercials, shows, music) from the web admin or the Mac app, with progress, into a library folder, streamed to disk and added to the schedule at once. Make the library's folder from the app too, so a remote server needs no shell access.
- **Docker:** a real image and compose file with FFmpeg, a non-root user that matches yours, a first-run access token, health checks and clean shutdown; see `docs/DOCKER.md`.
- **Optional access token,** switched on, off or replaced while the server runs, with a separate choice to let anyone tune in without it (watching open, managing protected).
- **Time zone is a setting** with automatic detection: the server honours `TZ`, the admin UIs notice when the device is in another zone and offer one click to move the whole station (schedules keep their clock times). This fixes schedules running hours off on a server in a container.
- **Station controls:** restart, stop or start a channel's stream, re-plan its schedule, start every show from episode 1, and reset picture/sound or the guide's look to defaults; the screen shown in gaps and when a show runs out (colour bars and tone included) is on the channel's Programming tab.
- **Live control:** take the whole station, or single channels, to the test screen, off air, black, snow or an emergency alert (EAS style, with the attention tone, a crawl and sample alerts), and give them back; start, stop and restart streams, and restart the server. Web admin and Mac app. `tvmaster access` shows the current access token, and `docs/HEADLESS.md` covers running without a screen.
- **Alerts you can tune:** full screen or a bar across the bottom of the programme, a retro or smooth theme (Modern, Midnight, Graphite, Snow, Forest, Berry, Mono), red, amber or blue, and a sound or none (attention tone, information chime, urgent warble, beeps), with a preview.
- **Put it on the plan now:** Refresh now, Air now (a show, a particular episode, anything) and editing a stretch of the schedule, in the web admin and the Mac app. A plan saved while a channel is on its stand-by screen airs within about half a minute.
- **Mac app:** seasons and rotations are editable too.
- **One episode per slot:** a time slot can air one episode at its start and leave the rest to the gap policy (the test screen), and the guide no longer shows a show carrying on through off-air time.
- **Mac app:** the access token is kept in a private file instead of the Keychain (no more password prompts), a cancelled request is no longer reported as "cannot reach the server", and it has the web admin's lineup, specials, commercials and Shows settings.
- **"Test screen" when a show runs out:** a new run-out choice. Each episode airs once, then the slot (and any time outside your time slots) shows the channel's off-air screen, colour bars and tone included.
- **Servers are found on the network:** the Mac app scans for TVMaster servers and connects with one click.
- **Picture uploads** for off-air screens, channel logos and guide backgrounds.
- **Smart TV player** with remote-control keys; IPTV and tuner routes documented.
- **Watched folders** and health alerts, with webhooks.
- Earlier work: scheduling, ads and bumpers, HTTPS, audio tracks and subtitles, per-show settings, test-pattern channels, installers for macOS, Windows and Linux.
