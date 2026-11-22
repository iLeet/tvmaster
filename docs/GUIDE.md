# The guide channel

A guide channel (`kind: "guide"`) is a live TV listing that TVMaster draws itself: it reads the same schedule as everything else, so it is always right, and it needs no libraries. Make one in the web admin (**Channels**, **New channel**, **Guide channel**) or the Mac app; both show a **live preview** of the settings as you change them (`POST /api/v1/guide/preview` draws one frame without saving or airing anything). Everything below is a field of the channel's `guide` object; fields you leave out keep their defaults, so a guide saved by an older version looks exactly as it did.

## Colours

`theme`: `auto` (colours made from the channel, the original look), `prevue` (cable blue and yellow), `amber`, `phosphor` (green screen), `neon`, `sunset`, `ocean`, `slate`, `paper` (light, like a printed listing) or `custom`, which builds a look from `colors.primary` and `colors.accent` (`#rrggbb`). `layout.scanlines` turns the CRT texture on or off.

## Style

`look` chooses how the guide is drawn, apart from its colours:

| Field | |
|---|---|
| `look.typeface` | `pixel` (the chunky retro letters, drawn at 640x360 and scaled up) or `smooth` (a modern typeface drawn at full 1280x720, bundled so it looks the same everywhere). Smooth text draws capitals and lower case, Latin and accented letters; anything else shows as `?`. |
| `look.corners` | `square` or `rounded`. |
| `look.text_size` | `small`, `medium`, `large`. The pixel font only has whole sizes, so Large doubles it. |
| `look.background` | `style`: `gradient`, `solid`, `stripes` or `image` (with `image`, a picture on the server, and `dim` 0-90, how much it is darkened). The picture is fitted to fill the screen. |
| `look.header_style` | `bar` (a coloured band) or `minimal` (just the words with a thin line). |
| `look.ticker_position` | `bottom` or `top`. |
| `look.cell_times` | Each programme's start and end under its name when there is room. |
| `look.now_marker` | The vertical line at "now" on the time grid. |

The editors also have one-click looks (Retro, Modern dark, Clean white, Minimal) that set colours and style together. More colours: themes `midnight`, `graphite`, `snow`, `forest`, `berry`, `mono`, and for `custom` a `colors.background` as well as `primary` and `accent`.

## Layout

| Field | |
|---|---|
| `layout.style` | `grid` (channels down the side, programmes across a time line) or `now_next` (one big row per channel: what is on and what is next, easy to read from the couch). |
| `layout.rows` | Channels on screen at once, 3-10. The picture makes room: a video window across the top leaves fewer rows. |
| `layout.scroll` | `page` (hold a page of channels, then slide to the next) or `smooth` (roll continuously, like the old cable guides). |
| `layout.page_seconds` | How long each page stays, 4-30. |
| `hours` | How much schedule the grid covers, 2-6. |
| `layout.time_step` | Minutes between marks on the time line: 15, 30 or 60. Labels thin out by themselves when they would collide. |
| `layout.channel_numbers`, `layout.channel_names` | Show them or not. |
| `layout.progress` | A thin bar showing how far through its programme each airing channel is. |
| `channels` | Only these channels (ids), in this order. Empty = every programming channel by number. |

## Heading and ticker

`title` (blank = "<name> GUIDE"), `header.title`, `header.clock`, `header.date` to show or hide each, `header.hour12` for `9:30 PM` or `21:30` (applies to the clock, the time line and the programme times), `header.logo` for a logo picture at the left of the heading (upload one in the editor; the guide uses the file's own transparency).

`ticker_options.enabled` turns the scrolling line off (the listing gets its room), `speed` is `slow`, `normal` or `fast`, and `now_on` says whether "NOW ON 4: Cheers" follows your own `ticker` lines.

## The video window

A live picture-in-picture of another channel inside the guide:

```json
"pip": { "mode": "rotate", "channels": ["retro", "toons"], "seconds": 30,
         "placement": "side_right", "size": "medium", "audio": "music", "info": true }
```

- `mode`: `off`, `channel` (always `pip.channel`), `commercials` (only commercials, see below), `mix` (commercials and channels in turn, see below) or `rotate` (flip through `pip.channels`, or every channel in the guide when that is empty, `seconds` each, 10-300).
- **Commercials only** (`mode: "commercials"`, `pip.libraries`: which commercials libraries, empty = all of them): the window plays your commercials in a fixed shuffled order, round and round. They come from a small, light internal stream (480p at 15 frames a second) that the guide starts while it needs it: it is **not a channel** (no number, nothing stored, listed nowhere). Saving the guide is refused with a way out if there is no commercials library yet.
- **Channels and commercials** (`mode: "mix"`): the window takes turns, commercials for `pip.commercial_seconds` (10-300), then a channel for `pip.seconds`, then commercials, then the next channel (`pip.channels`, or every channel in the guide).
- **The card beside a commercial window:** `pip.ad_title` (blank = "AD BREAK"), `pip.ad_lines` (up to 40 lines of up to 120 letters, shown two at a time, a new pair every `pip.ad_rotate_seconds`, 2-60; blank = "Commercials, one after another" / "Back to your show in a moment") with `{clock}` and `{date}` filled in. Use it for promos, sponsors or announcements.
- **The guide lists itself** in its own guide, like any channel (and is skipped when the window rotates).
- `placement`: `side_right`, `side_left` (docked beside the listing, which narrows) or `top_left`, `top_right` (a band across the top with the video and a card; the listing gets fewer rows). `size`: `small`, `medium`, `large`. The window is 16:9; a channel in another shape is fitted with bars.
- `audio`: `music` keeps the guide's music playing; `channel` plays the channel in the window instead.
- `info`: a card beside the window with the channel's number and name and what is on and next.

How it works: the guide channel starts the channel it is showing (as a viewer would) and keeps it on air while the window shows it, then reads that channel's own HLS stream. The window needs that channel to be on air, so showing a channel costs one more encode while the guide is watched, and a guide that is `always_on` keeps its window's channel on air too. A guide cannot show a guide. If the channel cannot start or its stream stalls, the window says NO SIGNAL, the guide carries on, and that channel is tried again after thirty seconds. When rotating, each change of channel is a short restart of the guide's encoder; the stream's timestamps and the music carry on across it (the music picks up where it was, not from the first track).

## Music

`music_library_ids`: libraries of kind `music`, looped under the guide.

## What is verified

Real FFmpeg: a video window that rotates between a red and a green channel shows each in turn with no gaps in the video or the audio across the joins; a window whose channel is missing shows a dark window and the guide carries on; the music list that carries on from the stream's position (concat `inpoint`) was checked with FFmpeg by decoding the pitch of the result (the end of one tone, then the next). The renderer is checked at every placement and size so the window never overlaps the listing or the ticker, in every look and with any number of channels, and that every option changes the picture.

## Known limits

The retro looks are drawn at 640x360 and scaled up (the same chunky look as the bumpers); the smooth looks are drawn at 1280x720 and scaled to the channel's size. Drawing a smooth frame takes about a millisecond on a laptop, so it costs nothing noticeable. A guide's settings are read when it starts: saving them restarts the guide.
