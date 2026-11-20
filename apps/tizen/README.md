# TVMaster Guide (Samsung Tizen TV app)

A cable-box style TV app for [TVMaster](../../README.md): live channels, channel surfing, and an interactive
2006-era blue grid guide with the live picture in a window. It is a plain HTML/CSS/JS web app (no build step, no
dependencies except the bundled hls.js) that talks only to the TVMaster API, so the server stays the owner of
channels and schedules, like every other TVMaster client.

Tested target: Samsung UN50DU6900FXZC, Tizen 8.0 (should work on any Tizen 6+ TV in Developer Mode).

## Using it

| Key | In live TV | In the guide |
|---|---|---|
| CH ▲ / CH ▼, ▲ / ▼ | next / previous channel | page up / down |
| 0-9 | type a channel number (goes after 1.5 s, or OK) | jump to that channel's row |
| OK | open the guide | watch the highlighted channel |
| ◀ | previous channel | earlier |
| ▶ / Info | show what is on now and next | later |
| Back | hide the banner, then exit the app | close the guide |
| Red | settings (server address) | settings |
| Green | change theme (Classic blue, Crimson, Night) | same |
| Yellow | on-screen log (for debugging) | same |
| Blue | demo lineup on/off (when the server has no channels) | - |

Channel surfing waits 0.3 s after the last press before tuning, so flicking through channels does not start a
stream for every channel passed.

**Server address.** The address is built in when you install (`--server`), and can be changed on the TV any time:
press **Red**, type it (OK on the field opens Samsung's on-screen keyboard; "10.0.0.251" is enough, port 8787 is
assumed), then **Save & connect**. It is tested against the server before it is kept. A server access token can be
entered there too, if you turn the token on (`open_viewing` in TVMaster avoids needing one). An address typed on the
TV is remembered, but only for the build it was typed into: reinstalling with a different `--server` uses that one.

**How fresh is it?**

| What | Updates |
|---|---|
| What is on now / next (banner, guide) | pushed by the server over a WebSocket the moment a programme changes |
| Channel list (new, removed or renamed channels) | every 30 s (every 5 s while the server is unreachable) |
| The schedule (the guide grid) | the whole lineup, about 24 h, in one request to `/iptv/guide.xml`; refreshed after 10 min, and at once when the server re-rolls a schedule |
| Open guide | redraws itself when any of the above change; the selection stays on the same channel |

## Install on a TV (first time, or on a new computer)

1. On the TV: Apps panel, type `12345`, switch **Developer mode** on, and enter this computer's IP address as the
   **Host PC IP**. Restart the TV.
2. `scripts/setup-tools.sh` downloads Samsung's Tizen CLI (about 270 MB) into `~/tizen-studio` and adds the TV
   extension and Certificate Manager. It changes nothing else on the computer.
3. Create the certificate (once a year, see below): run Certificate Manager
   (`open ~/tizen-studio/tools/certificate-manager/certificate-manager.app`), create a **Samsung** certificate
   profile for a **TV**, sign in with your Samsung account, and add your TV's DUID (`sdb shell 0 getduid`).
   Name the profile `TVDev`. **Quit Certificate Manager afterwards**: while it is open it holds the TV's only
   debug connection and `sdb connect` fails.
4. `scripts/build-install.sh --tv <TV IP> --server http://<TVMaster IP>:8787` packages, signs, installs and starts it.

Updating the app later is only step 4 (about 5 seconds).

## Certificate: how long does it last?

The Samsung certificate made by Certificate Manager is valid for **one year** (checked: both the author and
distributor certificates run from the day they were made to the same date next year; this install's expire
**2027-11-20**). The app is not "permanently" signed. Samsung's developer forum also reports, with no official
documentation, that:

- a TV firmware update, or sometimes power-cycling the TV, can remove apps installed in Developer Mode, or turn
  Developer Mode off (re-enable it and run `build-install.sh` again; the app and its settings are cheap to put back);
- an expired certificate stops new installs, and renewing is not always self-service: create a new profile in
  Certificate Manager, and if it still hands out an expired or rejected certificate, Samsung's TV developer
  support has to issue one.

So: expect to re-run the install after a TV update, and put the renewal date (about 2027-11-01) in a calendar.
Whether an already-installed app keeps launching after its certificate expires was not verifiable from
documentation; plan on reinstalling.

## Troubleshooting

- `sdb connect` fails though the TV answers on port 26101: Certificate Manager (or another tool) holds the
  connection. Quit it, then `sdb kill-server` and connect again. Also check the TV's Developer Mode "Host PC IP"
  is this computer's current address (it changes if the computer gets a new IP).
- `Failed to install` with no reason: package file names with spaces fail on the TV (the script already avoids that).
  A certificate message means the profile is missing, expired, or the TV's DUID is not in it.
- The guide grid is empty but channels tune: the app reads the schedule from `/iptv/guide.xml` because TVMaster's
  *open viewing* mode covers that and the channel list but **not** `/api/v1/channels/{id}/schedule`. If the server
  requires a token, enter it in Settings (Red key).
- A desktop browser cannot reach the real server: TVMaster sends no CORS headers; the TV app is exempt through
  `<access origin="*">` in `config.xml`. To see the layout on a computer use demo data: serve `src/` and open
  `?demo=1` (`python3 -m http.server -d src 8099`), or put a small CORS-adding proxy in front of the server.
- The TV's system log and remote inspector are disabled on retail TVs, so use the Yellow key's on-screen log.

## Layout

```
src/            the app (what is packaged onto the TV)
  config.xml    Tizen manifest: id, privileges (internet, remote keys, AVPlay), network access
  index.html    the screens: banner, number entry, guide, settings, status
  css/app.css   the 2006 look and the three themes (colour variables at the top)
  js/           config, api (REST + WebSocket), data (channels, schedules, refresh), player (AVPlay / hls.js),
                keys, ui (banner, number entry, themes), settings, guide, main
tools/make-icon.swift  redraws src/icon.png (512x423 tile with the name in it); run `swift tools/make-icon.swift`
scripts/
  setup-tools.sh      install the Tizen CLI, TV extension and Certificate Manager
  build-install.sh    package, sign, install, launch
  cleanup-tools.sh    remove the app, the tools and (only if you say so) the certificates
```

Everything is drawn on a 1920x1080 stage that scales to the screen. On a TV the video is drawn by Samsung's
AVPlay *behind* the web page, so the page is transparent and the guide has a hole where the picture shows; in a
browser the same picture comes from `<video>` + hls.js.
