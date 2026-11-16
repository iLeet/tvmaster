# Watching on a smart TV

TVMaster's channels are ordinary HLS and MPEG-TS streams with an M3U playlist and an XMLTV guide, so a TV can reach them in three ways. They are listed here from the most TV-like to the quickest to try. The **Share** screen of the Mac app and the **Settings** page of the web admin show every address below, ready to copy (with the access token added if you use one).

## 1. An IPTV app on the TV (flip with the real remote)

Apps that take an M3U playlist and an XMLTV guide give you a proper channel list, channel numbers, a programme guide and CH ▲▼ on the remote. Android TV, Google TV and Fire TV have several (TiviMate, IPTV Smarters, Kodi with its IPTV Simple Client, VLC); Samsung and LG have a few (SS IPTV, Smart IPTV, IPTV Smarters); Apple TV has its own. Pick whichever you like and give it:

| | |
|---|---|
| Playlist | `http://<server>:8787/iptv/playlist.m3u` |
| Guide | `http://<server>:8787/iptv/guide.xml` |

If you use an access token the two links in the Share screen already carry it (`?token=…`); the playlist hands the token on to every stream, so the app needs nothing else. Use the **HTTP** addresses: IPTV apps usually refuse the self-signed certificate that `--tls` makes (a certificate from a real authority is fine).

## 2. Plex, Jellyfin or Emby Live TV

All three have TV apps for every platform, and all three can use TVMaster as a network tuner (an HDHomeRun): add a tuner device at `<server>:8787` and use the XMLTV link above for guide data. You then change channels in their Live TV screen, with the remote's channel keys. (This follows the HDHomeRun API those servers speak; it has not been tried against a real Plex, Jellyfin or Emby server.)

## 3. TVMaster's own TV page, in the TV's web browser

Open `http://<server>:8787/?ui=tv` in the TV's browser (Samsung Internet, the LG web browser, Silk on Fire TV, Chrome on Android TV, or any other). The page recognises a TV browser by itself; `?ui=tv` forces it and `?ui=off` turns it off. It fills the screen, remembers the channel you were on, keeps the screen awake, and works with the remote:

| Key | Does |
|---|---|
| **CH ▲ / CH ▼**, **▲ / ▼**, Page Up / Page Down | next / previous channel |
| **0-9** | type a channel number (it tunes after a moment, or press OK to go at once) |
| **OK** or **▶** | open the channel list (▲▼ to choose, OK to watch, Back to close) |
| **◀** | back to the last channel |
| **Info** (or **Yellow**) | what is on now and next |
| **Red** | the channel list |
| **Green** | the guide channel |
| **Play / Pause** | pause or resume |
| **Back** | close the list or the number you are typing |

Bookmark `…/?ui=tv&channel=4` to start on a particular channel. If the server has an access token, open the page once as `…/?ui=tv&token=<token>`: the page remembers it and takes it out of the address bar.

The page plays through hls.js, which TVMaster ships with itself, so it works on a network with no internet. TVs from before about 2016 may not have the media features it needs. Sound can need one press of OK on the first start, because some browsers refuse to play with sound until you have pressed something.

## Which should I use?

For a TV you will sit in front of every night, an IPTV app (1) or Plex/Jellyfin/Emby (2) feels most like a cable box. The browser page (3) needs nothing installed, which makes it the quickest way to see a channel on a TV you do not own the apps for. To control a TV from your phone instead, open `http://<server>:8787/?tv=living-room` on the TV and use the phone remote (`/?remote=living-room`) from the Share screen.
