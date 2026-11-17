# Changelog

## 1.0.0

The first stable release.

- **Promos:** make your own commercials and promos in the app (a sponsor spot, a programme promo or slides of text) in the station's look or your own, with a picture, a jingle or your music and an old-tape look; or use old ones: upload them or have the server download a direct link. They become video files in the commercials and air in breaks. See `docs/PROMOS.md`.
- **Ticker:** a scrolling strip of text across any channel: messages, headlines from news feeds, and what is on next, at the top or bottom, with a tab, always on or now and then. Drawn in the station theme. See `docs/CHANNEL-OPTIONS.md`.
- **Backup and restore** of the station (channels, lineups, libraries, show names, positions, settings, pictures) in one file, nightly if you like, downloadable, with a safety copy before every restore; this server's access token is never in a backup. See `docs/BACKUP.md`.
- Debug builds are much smaller on disk.
- **One theme for everything.** A station-wide theme and graphics style (Retro pixels or Modern smooth) dresses the guide, station and break cards, the stand-by and sign-off screens, alerts, the now-playing card, the channel logo and the clock. A channel can choose its own. New **Appearance** page in the web admin and the Mac app, with previews of each graphic. See `docs/THEMES.md`.
- **Show titles and grouping.** "Bleach season 01" is now the show Bleach, season 1 (automatic, switchable). Any show can be renamed, several folders can be put together as one show with a season each, and a show can have a description, rating, tags and poster (used in IPTV guides). Lineups, per-show settings and episode positions follow a rename. See `docs/SHOWS.md`.
- **Media from the app.** One click makes the station's own media folder on the server (shows, commercials, bumpers, music) to upload into from the web admin or the Mac app, by drag and drop too; linked folders keep working beside it.
- **Breaks, rewritten.** A tab called Breaks with ready-made amounts of advertising (none, light, classic, heavy, after the show), a sentence saying what that does to a 30-minute slot, a picture of each station card, and clearer names for everything. The numbers are under Fine-tune.
- **First-run setup** in the web admin and the Mac app: add media, choose a look, make the first channels (a guide and a shuffled mix). The steps live on the server so every app asks the same thing.
- **Mac app, closer to the web admin:** a week grid to drag slots around, the same Breaks and Look sections, the ad years, audio description, frame rate and bitrate settings. `scripts/parity-check.py` fails when a server setting is missing from either app.
- **Fixed:** the Mac app crashed on its own keyboard shortcuts once it had ten sidebar pages.
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
