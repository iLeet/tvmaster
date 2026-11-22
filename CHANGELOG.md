# Changelog

## 1.6.0

Looks, new themes, and a cable box and liquid glass for the guide, the screens and the TV.

- **Looks.** A whole appearance in one click, on the Appearance page: the colours and style, the stand-by and break screens, and how the guides are laid out, with a picture of the guide in each. Eight come with the station: Classic cable, **Cable box, 2000s** (blue cells with the one on now in orange, a light channel column, the live picture top right with a cream info card beside it), **Liquid glass**, Kids' TV, Night modern, Printed listing, Terminal and Sunset VHS. Choose which parts to change (colours, screens, guide layouts). **Save how the station looks now** as a look of your own, **export** one as a file and **import** one.
- **14 more themes** for the guide and every screen: Cable box, Glass, Ember, Teal, Sakura, Sand, Royal, Vapor, Noir, Terminal, Dusk, Lime, Ice and Rosewood.
- **Frosted glass for the guide:** see-through cells, channel names and cards with a bright edge, over a new **Soft colour** background that drifts slowly. A new switch in the guide's appearance (web and Mac), best with smooth type.
- **Light cards are readable:** the info card beside the guide's picture now uses dark words when its colour is light (it did not on the light themes).
- **Samsung TV app 1.6.0 follows the station.** It reads the station's theme from the server (so every theme above works on the TV, and Green steps through them), and takes its layout from the guide channel: the **Cable box** layout (info card at the left, picture at the right, light channel column, orange for what is on now, a button bar) and **Liquid glass** (frosted panels over drifting colour; a little less clear on older TVs that cannot blur). Without a server it keeps its three built-in themes.
- `GET /api/v1/theme/palettes`, `GET /api/v1/looks` and `POST /api/v1/looks[/import|/:id/apply]`: see `docs/THEMES.md`.

## 1.5.0

Screens you can design.

- **New Screens page** (web admin and Mac app, under Station). The stand-by screen, the sign-off screen and the screens around breaks (Up next, Later today, We'll be right back, Welcome back, Station ID) are each a design you choose from a gallery of 22 and change: the words (with fill-ins for the channel, show, time and clock), their colours and sizes, the background (the station theme, soft moving colour, a gradient, one colour, a moving pattern, colour bars or a picture of your own) and the speed of the motion. A live preview plays the animation and has a scrubber. Use a design on the whole station or on a single channel.
- **Animated.** Words and shapes arrive by fading, sliding, zooming, typing themselves out or wiping across, then keep moving: breathing, floating, drifting around the screen like a screensaver, fading in and out, or scrolling. The stand-by screen can show a ticking clock, a drifting name or a test card.
- **Liquid glass.** Frosted panels that blur the soft, moving colour behind them, in every screen.
- **Everything is editable.** Under More options, every layer's position, size, colour, opacity, entrance (and how long it takes and when it starts) and motion, fading out, and the design as JSON. Words, shapes, clocks, programme lists and pictures (a logo keeps its see-through parts).
- Nothing changes until you choose a design: **Classic** is the built-in screen. A channel set to colour bars, snow, black or a picture keeps its stand-by choice.
- Per-channel designs are kept when a channel is saved from the web admin.
- See `docs/SCREENS.md`. Not designed yet: the now-playing banner and alerts, and a design per day part.

## 1.4.0

Picture & sound and Settings, tidied the same way as Programming.

- **Picture & sound opens with the basics:** picture quality, when to show subtitles, and "even out loud and quiet", then the on-screen extras (channel logo, now-playing card, clock, ticker). Audio languages, subtitle style, frame rate, bitrate, screen shape, look, the channel's own colours and per-show settings are under **More options**. The tab was called "Sound & picture".
- **Settings is five pages instead of one long list:** *Station* (time zone, how often to look for new episodes), *Connect devices* (the addresses for IPTV apps, smart TVs, Plex/Jellyfin, browsers and phones), *Alerts* (problem alerts and the web address to send them to), *Backups*, and *Server* (access token, HTTPS, version and restart). The Mac app's Settings window has the same pages, plus *This Mac* for the server it starts itself and its log.
- Nothing was removed: every setting is still there, one click from where it was.

## 1.3.0

A simpler Programming tab, and the title card where you asked for it.

- **Fixed: empty time on a weekly schedule filled with episodes.** A channel with a weekly schedule and nothing chosen for the gaps carried on playing shows between slots. There is now one plain question for it, **When nothing is scheduled**: *Show the off-air screen*, *Keep playing shows*, *Play something random* or *Play these shows*. A new weekly channel starts on the title card. (A slot can also be set to the off-air screen itself.)
- **A short break between shows, on any channel.** A new ready-made choice on the Breaks tab, *Short break between shows*: no breaks inside the show, about a minute of commercials after each one. It works on chaos and shuffle channels, which have no time grid.
- **One Programming tab.** *How it plays* (in order, shuffle, chaos, weekly schedule), then *Your week*, then *When nothing is scheduled*, *When a show runs out of episodes*, *The off-air screen* and a line for the commercial breaks, with the General tab showing a one-line summary. Surprises, start times, sign-off, *Seasonal changes* (the API's "seasons"), *Holidays and specials* ("events") and the lineup JSON are under **More options**. The off-air screen and sign-off hours moved here from Sound & picture, so each has one home. Same in the web admin and the Mac app.
- **Plainer words.** The run-out choices are *Start over*, *Move to the next show*, *Play something random* and *Show the off-air screen*; the old names are in the tooltips and in the API, which is unchanged.
- `Source` gains `{"type":"off_air"}`, usable as the lineup's `fill` or as a slot's source.

## 1.2.0

Easier to set up, with every option still there.

- **New TV channel guide:** "New channel → TV channel" is now six plain steps (name and number, what it plays, how it plays, commercials, look, ready) with a summary at the end and a button straight to the schedule. The full editor is one click away ("Use the full editor instead"). Same steps in the web admin and the Mac app.
- **Commercials made simple:** the Breaks tab starts with one question, "How much advertising?" (No breaks, Station cards only, Light, Classic TV, Heavy, After the show), the sentence saying what that means for a 30-minute slot, and where the commercials come from (every commercials library by default, with buttons to add some or make some). Timing numbers, years, station cards and your own clips are under **More options**.
- **More options everywhere it matters:** a fold for the less common settings, and a **Show all options** switch (bottom of the sidebar) that opens every fold at once, remembered on each device.
- **Sidebar in groups:** Watch, Programming, Content and Station.
- **Guide video window: Commercials only.** The window plays commercials one after another, in random order, from the commercials libraries you choose, and the card beside it says "Ad break". No channel has to be watched for it, so it is light on the server. The station makes a channel called Commercial Break for it, which stays out of the guide listing. See `docs/GUIDE.md`.

## 1.1.1

- **Fixed:** a show whose season folders are named `S01`, `S2` and so on was listed under the folder name ("S01") instead of the show's name. Season folders written that way, and in French, German and Spanish ("Saison 2", "Staffel 1", "Temporada 3"), are now recognised. If a library already has such a show, rescan it (or set a name under Shows).

## 1.1.0

- **Smooth streaming on small servers.** The background scan that looks for ad-break points in new files used to run two full-speed FFmpeg jobs at normal priority, which could starve a live channel of processor time and make the picture play for a few seconds, stop, and repeat. It now runs one file at a time, on one thread, at the lowest priority, so a channel that is being watched always comes first.
- Every build now carries its own version number (this one is 1.1.0); the Mac app, the server and the installers agree.

## 1.0.0

The first stable release.

- **Day parts:** named stretches of the broadcast day (Kids' morning, After school, Family evening, Late night) with the strongest rating that may air, no commercial breaks if you like, and a look of their own for the station's graphics and alerts. Shows you have rated are kept out of hours they do not suit. New **Day parts** page in the web admin and the Mac app. See `docs/DAYPARTS.md`.
- **Kids look and gentle alerts:** a bright Kids theme, and a "gentle alerts" option for any theme: soft colours and friendlier words ("Heads up", "Important message"), a soft chime, nothing flashing. Plus eight more colour themes (Bubblegum, Lavender, Mint, Arctic, Cherry, Gold, Coffee, and Kids). See `docs/THEMES.md`.
- **Info channels:** channels with nothing to schedule that draw their own pages: notices you write, headlines from news feeds, what is on tonight, a clock and pictures, with a ticker, a clock and music. They show on the guide with what they are, and in the XMLTV listing. See `docs/INFO-CHANNELS.md`.
- **A clearer channel creator:** "New channel" now starts with what kind it is (TV channel, guide, a kind of info channel, test pattern), each with a name, the next free number and settings that work; the channel list is sectioned the same way, and the guide picks TV and info channels together.
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
