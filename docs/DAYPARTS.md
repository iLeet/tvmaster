# Day parts

A day part is a named stretch of the broadcast day with its own rules. They belong to the station (one list
under **Day parts** in the web admin and the Mac app), and each one covers every TV channel or only the ones you
pick.

| Setting | What it does |
| --- | --- |
| Name, from, until, days | When it applies, in each channel's time zone. An end before the start runs overnight. Days empty = every day. |
| Channels | Which TV channels it covers. Empty = all. |
| Strongest rating | Nothing rated above this airs in these hours. |
| Let unrated shows air | Off by default: a show nobody has rated is not known to be suitable. |
| Commercial breaks | As the channel is set up, or none (shows run clean, padded to the next time slot). |
| A look of its own | The station's graphics in these hours (cards, bumpers, the bug, lower thirds, tickers and alerts) use this theme, style and gentle-alerts setting instead of the channel's. |

The first part that covers a moment, for a channel, is the one in force. Outside them channels carry on as usual.
Overlapping parts are fine: the order in the list is the priority.

## Ratings

Rate a show under **Shows** (the free-text Rating box offers `TV-Y`, `TV-Y7`, `TV-G`, `TV-PG`, `TV-14`, `TV-MA`, `G`,
`PG`, `PG-13`, `R`, `C`, `C8`, `14+`, `18+`). The server reads them into five levels:

`all_ages` (G, TV-Y, TV-G, C) < `kids` (TV-Y7, C8, 7+) < `pg` (PG, TV-PG) < `teen` (PG-13, TV-14, 14+, 14A) < `adult` (R, TV-MA, NC-17, 18+)

A rating that is not recognised counts as unrated. Each day part shows how many shows can air under its limit,
how many are held back and how many have no rating.

How it works: when the schedule is planned, picks for a slot that falls in a part with a limit skip shows above it
(this includes shows a time slot names, random picks and the fill). If nothing suitable is left, the slot is left
to the channel's off-air screen. The part that applies is decided by when a slot starts. Changing a rating or the
parts plans every channel again. Stretches you have scheduled by hand with "Play now" or schedule edits are yours.

A look applies to the next card, bumper or overlay that is drawn, and to alerts when they go on air; a programme
already on screen keeps its own overlays until it ends. Guide and info channels follow the channel's or the
station's look (info channels honour a day part's look; the guide does not change by the hour).

## API

- `GET /api/v1/dayparts` - `parts`, `check` (per part: `shows`, `airing`, `held_back`, `unrated`), the TV `channels`,
  the `ratings`, `presets` (starting points) and the station `timezone`.
- `PUT /api/v1/dayparts` - `{ "parts": [ { "name": "Kids' morning", "days": [], "start": "06:00", "end": "09:00",
  "channels": [], "max_rating": "kids", "allow_unrated": false, "breaks": "normal", "look": { ...a theme... } } ] }`.
  Replaces the list, plans the schedules again and restarts streams that are on air so looks apply.
