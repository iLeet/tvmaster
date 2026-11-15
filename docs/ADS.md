# Commercials and bumpers

How TVMaster turns a 22-minute episode in a 30-minute slot into real-feeling television: breaks at natural spots, commercial pods that fill exactly, and station bumpers like "Up next" and "Later today".

## The idea

A show is shorter than its slot. The difference is the **ad budget**. TVMaster splits the show at natural break points, then fills every break to the second:

```
[ act 1 ] [we'll be right back] [commercials … station ID] [back to <show>] [ act 2 ] …
… [ last act ] [commercials … station ID] [up next / later today]
```

Every piece has a fixed length, so each slot ends exactly on the grid. All of it is part of the schedule, so the guide, the XMLTV feed and playout always agree.

## Turning it on

Add an `ads` section to a channel's lineup. Without one, shows run clean with plain padding.

```json
"lineup": {
  "grid_minutes": 30,
  "ads": {
    "library_ids": [2],
    "bumper_library_ids": [3],
    "pod_seconds": 120,
    "years": [1985, 1989],
    "bumpers": { "later_today_every": 3 }
  },
  "blocks": [ ... ]
}
```

| Setting | Meaning |
|---|---|
| `library_ids` | Libraries of kind `commercials` to draw ads from. |
| `bumper_library_ids` | Libraries of kind `bumpers` with your own ready-made clips (see below). |
| `commercials` | `false` = bumpers only, no ads. Default `true`. |
| `minutes_per_hour` | Ad time per hour of programming on a channel **without** a time grid. On a grid, ad time is simply what is left of each slot. Default 8. |
| `pod_seconds` | Target length of one break. Default 120. Longer budgets get more breaks. |
| `max_breaks` | Most breaks inside one show (the final break after the show is extra). Default 4. |
| `mid_breaks` | `false` = all ads after the show instead of inside it. |
| `no_repeat` | Do not repeat a commercial within this many plays. Default 12. |
| `years` | `[from, to]`: only air commercials from these years (a "time machine" for ads). |
| `bumpers` | `brb`, `back_to`, `up_next`, `station_id` toggles (all on by default) and `later_today_every` (every Nth slot shows "later today" instead of "up next"; 0 = never). |

Create the libraries with `kind` set to `commercials` or `bumpers`, scan them like any other, and name them in `ads`. Commercials do not need any particular folder layout. A year in a folder or file name (`ads/1987/Coke.mp4`, `Coke (1987).mp4`) is picked up for the `years` filter; commercials with no year are skipped when `years` is set.

## Where breaks go

For each show, TVMaster looks for natural break points in this order:

1. **Chapter markers** in the file (free, read during the scan).
2. **Black frames**: the black gaps broadcasters left between acts. Found by a background scan that starts automatically after a `shows` library is scanned (two files at a time; `POST /api/v1/libraries/{id}/analyze` re-runs it). Progress is shown in `GET /api/v1/libraries`. New breaks are picked up by a re-roll when it finishes.
3. **An act structure**: evenly spaced, with a little deterministic jitter.

Breaks are never in the first or last 90 seconds, never closer than two minutes, and are chosen as close as possible to evenly spaced real candidates. A 22-minute show with 8 minutes of ad time gets three mid-show breaks and a final one after the show.

## How a break is filled

Each break has a fixed budget. Inside it:

1. **Opening:** "We'll be right back", or one of your own bumper clips if you supplied any that fit.
2. **Commercials:** chosen from the pool to fill the remaining time as exactly as possible (a knapsack over the candidate ads), never repeating within `no_repeat` plays, never overshooting.
3. **Odd seconds:** a few seconds stretch the closing bumper; anything more becomes a station ID card.
4. **Closing:** "Back to <show>" after a mid-show break; "Up next" (or "Later today" every Nth slot) after the last one.

If there are no commercials (or `commercials` is off), the same structure is filled with bumpers and station IDs instead.

## The bumpers

Drawn by TVMaster's own renderer: chunky pixel text, scrolling stripes and scanlines, in colours derived from the channel so each channel has its own look. They need no font files or downloads, and look identical on every platform.

| Bumper | Says |
|---|---|
| We'll be right back | "WE'LL BE RIGHT BACK / AFTER THIS: <show>" |
| Back to the show | "WELCOME BACK TO <show>" |
| Up next | "UP NEXT: <program> AT 8:30 PM" |
| Later today / Coming up tonight | The next few programmes with their times (switches to "coming up tonight" from 5pm). |
| Station ID | "YOU'RE WATCHING <channel>" |

The words are filled in **when the bumper airs**, from the live schedule, so "Up next" is always correct even after a re-roll. Times are in the channel's time zone.

### Your own bumper clips

Put clips (old channel IDs, network stings, whatever you own) in a `bumpers` library and name it in `bumper_library_ids`. They open breaks in place of "we'll be right back" when they fit. TVMaster does not ship any third-party material; the built-in graphics are original.

## Re-rolls and autopilot

Everything above is generated by the same deterministic scheduler as the shows. A re-roll cuts at a show boundary, so a break is never split, and the episode cursors only advance for the first act of a show. A top-up looks at the ads already scheduled, so a commercial does not repeat across the seam.

## Limits today

- Break detection needs real black gaps or chapters to land on natural spots; without them the act structure is a good guess, not a perfect cut. Very short test clips (under about 5 minutes) get fewer breaks than long ones because of the 90-second margins.
- Commercials are not volume-normalised yet.
- No channel bug or "now playing" lower third yet (planned with the guide channel).
- Bumpers are silent; audio jingles are on the wish list.
