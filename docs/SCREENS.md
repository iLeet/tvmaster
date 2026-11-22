# Screens

The screens a station draws itself, each of which can be a design you choose and change:

| Screen | On air |
|---|---|
| **Stand-by** | When nothing is: time nothing is scheduled (if you chose the off-air screen for it), a finished show, a file that cannot be played. Replaces the station card; a channel set to colour bars, snow, black or a picture keeps those. |
| **Sign-off** | During sign-off hours, when the sign-off screen is the station card. Without a design of its own it uses the stand-by one. |
| **Up next** | Between the last break and the next programme. |
| **Later today** | The next few programmes, as a list. |
| **We'll be right back** | At the start of a commercial break. |
| **Welcome back** | At the end of a break inside a show. |
| **Station ID** | A short reminder of which channel this is. |

Not designed yet (planned): the now-playing banner and alerts.

## Choosing

**Screens** (web admin and Mac app). Pick a screen, pick a design from its gallery, change the words,
colours, background and speed, watch it play in the preview, and **Use this on the whole station** or on
one channel (switch *Applies to* to *One channel*). A channel follows the station until it has its own.
**Classic** is the built-in screen, as before; nothing changes until you choose a design.

Under **More options**: every layer (position, size, colour, opacity, how it arrives, what it keeps
doing), fading out, and the whole design as JSON.

## What a design is

A backdrop and a stack of layers, drawn at 1280x720 with smooth type. All numbers are whole numbers:
positions are thousandths of the screen (`x: 500` is the middle), times are milliseconds, speeds are
percent.

- **Backdrop:** `theme` (the station theme's colours with a drifting glow), `solid`, `gradient`, `aurora`
  (soft, moving colour: the liquid look), `bars`, `pattern` (dots, grid, stripes) or `picture`.
- **Layers:** `text`, `clock`, `shape` (a bar, card, pill, circle, or **frosted glass** that blurs what is
  behind it), `list` (the next programmes) and `image` (a logo or photo; see-through parts stay see-through).
- **Arrives** (`enter`): none, fade, slide up/down/left/right, zoom, type (letters appear one by one), wipe.
- **Then** (`motion`): still, pulse, float, drift (bounces around the screen), blink, marquee (scrolls across).
- **Colours** are the theme's (`primary`, `accent`, `accent_text`, `primary_text`, `ink`, `dark`, `bottom`,
  `on_accent`, `white`, `black`) or `#rrggbb`, so a design follows the station's theme and any day part's look.
- **Words** can use `{channel}`, `{number}`, `{show}`, `{title}`, `{time}`, `{message}`, `{clock}`, `{date}`;
  the clock ticks while the screen is up.

## Cost

A designed screen is drawn live, frame by frame, only while a channel is airing it, like an info
channel. Measured on the 4-thread (Core i3) server with software encoding: a channel showing the classic
card used about 1.9 cores in total, and the same channel showing the animated "Liquid glass" stand-by
about 2.3, so a moving design costs roughly another 0.4 of a core while someone is watching. Pick **Keep
it still after it arrives** for a design whose motion you do not need.

## API

- `GET /api/v1/screens[?channel=ID]` - the screens, each with its gallery (`presets`, designs written
  out in full), what is chosen (`current`) and where it comes from (`source`: `none`, `station`, `channel`).
- `PUT /api/v1/screens/:kind[?channel=ID]` - `{ "preset": "glass" }` for a gallery design, or
  `{ "preset": "custom", "design": {...} }`. Screens on air restart to pick it up.
- `DELETE /api/v1/screens/:kind[?channel=ID]` - back to classic (or, for a channel, to the station's).
- `POST /api/v1/screens/preview` - `{ "kind", "design", "t", "channel" }` returns one frame as a PNG.

A channel's own designs are kept in its `options.screens`.
