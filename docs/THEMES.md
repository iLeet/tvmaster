# Themes

One look for everything the station draws itself: the guide, the station card and the cards around
breaks ("we'll be right back", "up next"...), the stand-by and sign-off screens, alerts, the
now-playing card, the channel logo and the clock.

## Choosing

**Appearance** (web admin and Mac app) sets the station's theme. A channel can choose its own under its
settings ("Look"); otherwise it follows the station.

- **Theme:** Automatic (colours made from each channel, so every channel has its own look), Prevue, Amber,
  Phosphor, Neon, Sunset, Ocean, Slate, Paper, Midnight, Graphite, Snow, Forest, Berry, Mono, or Custom
  (a main colour, a highlight and optionally a background).
- **Style:** *Retro* is chunky pixel type with scanlines at 640x360, scaled up. *Modern* is smooth type,
  soft shapes and gentle motion at 1280x720.

A guide whose own theme is "follow the station" (the default) takes the station's theme and style. Pick a
theme on the guide to give that guide its own colours. Alerts offer "Match the station theme" (the
default), or any of their own looks.

## API

- `GET /api/v1/theme` - the chosen theme (`station`), the themes (`themes`: id, name, about, three swatch
  colours) and the styles.
- `PUT /api/v1/theme` - `{ "theme": "midnight", "colors": {"primary": null, "accent": null, "background": null}, "style": "modern" }`.
  Channels that follow it pick it up within seconds.
- `POST /api/v1/theme/preview` - one graphic as a PNG. `what` is `card`, `bumper` (with `scene`:
  `brb`, `back_to`, `up_next`, `later_today`, `station_id`), `guide`, `alert` or `bar`; `theme` is a theme
  as above; `id`, `name` and `number` make a channel's own card.
- A channel's own theme is `options.theme` (absent or null = follow the station).
