# Themes

One look for everything the station draws itself: the guide, the station card and the cards around
breaks ("we'll be right back", "up next"...), the stand-by and sign-off screens, alerts, the
now-playing card, the channel logo and the clock.

## Choosing

**Appearance** (web admin and Mac app) sets the station's theme. A channel can choose its own under its
settings ("Look"); otherwise it follows the station.

- **Theme:** Automatic (colours made from each channel, so every channel has its own look), Prevue, Amber,
  Phosphor, Neon, Sunset, Ocean, Slate, Paper, Midnight, Graphite, Snow, Forest, Berry, Mono, Kids, Bubblegum,
  Lavender, Mint, Arctic, Cherry, Gold, Coffee, or Custom
  (a main colour, a highlight and optionally a background).
- **Style:** *Retro* is chunky pixel type with scanlines at 640x360, scaled up. *Modern* is smooth type,
  soft shapes and gentle motion at 1280x720.

- **Gentle alerts:** soft colours and friendlier words ("Heads up", "Important message", "Good to know"
  instead of "Emergency alert"), a soft chime whatever sound the alert has, and nothing flashing, so a child is
  not frightened. The **Kids** theme is always gentle; any other theme can switch it on
  (`"gentle": true`). Gentle alerts are always drawn in the smooth style.

A theme can also change with the time of day: see [DAYPARTS.md](DAYPARTS.md). The stand-by, sign-off and break screens can be designed as well: see [SCREENS.md](SCREENS.md).

A guide whose own theme is "follow the station" (the default) takes the station's theme and style. Pick a
theme on the guide to give that guide its own colours. Alerts offer "Match the station theme" (the
default), or any of their own looks.

## API

- `GET /api/v1/theme` - the chosen theme (`station`), the themes (`themes`: id, name, about, three swatch
  colours) and the styles.
- `PUT /api/v1/theme` - `{ "theme": "midnight", "colors": {"primary": null, "accent": null, "background": null}, "style": "modern", "gentle": false }`.
  Channels that follow it pick it up within seconds.
- `POST /api/v1/theme/preview` - one graphic as a PNG. `what` is `card`, `bumper` (with `scene`:
  `brb`, `back_to`, `up_next`, `later_today`, `station_id`), `guide`, `alert` or `bar`; `theme` is a theme
  as above; `id`, `name` and `number` make a channel's own card.
- A channel's own theme is `options.theme` (absent or null = follow the station).


## Looks

A **look** is a whole appearance: the station theme (colours, style, gentle alerts), the screens
(stand-by, Up next ... see [SCREENS.md](SCREENS.md)) and how the guides are laid out. They come with the
station: Classic cable, Cable box 2000s, Liquid glass, Kids' TV, Night modern, Printed listing, Terminal and
Sunset VHS. On **Appearance** choose one and which parts to change; save the current appearance as your own;
export a look to a file and import one.

- **Cable box:** the `cable` theme (blue cells, the one on now in orange, a light channel column, a cream
  card) with the guide's picture at the top right and the info card at its left.
- **Liquid glass:** the `glass` theme, the guide's *Frosted glass* switch with the *Soft colour* background,
  smooth type and rounded corners, and the glass design for every screen.
- Applying a look sets the guide channels' layout (they follow the station's colours); a guide you have
  changed by hand is changed again, so untick *Guide layouts* to leave guides alone.

### The guide's own settings

`look.glass` (frosted glass) and `look.background.style: "aurora"` (soft colour) in a guide's settings.

### API

- `GET /api/v1/looks` - `{ looks: [{ id, name, about, builtin, theme, guide, screens, preview_guide }] }`.
- `POST /api/v1/looks/:id/apply` - `{ "theme": true, "screens": true, "guides": true }`.
- `POST /api/v1/looks` - `{ name, about }` saves the current appearance. `POST /api/v1/looks/import` takes
  `{ name, about, theme, guide, screens }` (what an export holds). `DELETE /api/v1/looks/:id` forgets one of yours.
- `GET /api/v1/theme/palettes` - every theme's full set of colours, and the station's own.

## The Samsung TV app

The TV guide follows the station: it uses the station theme's palette (Green steps through every theme the
server has, then back to "the station's") and takes its layout from the guide channel: **Cable box** when the
guide's picture is at the top right with its info card, **Liquid glass** when the guide has frosted glass on.
