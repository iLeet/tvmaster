# Scheduling and Autopilot

How a channel decides what to air, and how it keeps airing with nobody watching the controls.

## The idea

A channel is described by a **strategy** plus an optional **lineup**. The scheduler turns that description into a concrete list of items (the *schedule*) and stores it in the database about 72 hours ahead. The guide, the XMLTV feed and the playout engine all read that same list, so the guide is always what actually airs. The scheduler is deterministic: the same lineup, library and seed always produce the same schedule.

## Strategies (the simple way)

Set `strategy` on the channel and leave `lineup` out.

| Strategy | What airs |
|---|---|
| `sequential` | Episodes in order, shows interleaved one after another, forever. |
| `shuffle` | A shuffle bag: every episode once per pass, in random order, then reshuffled. |
| `chaos` | Anything, random. |
| `block_grid` | Follow the lineup below; anything unscheduled falls back to sequential. |

`chaos` (0-100) on any channel swaps that percentage of picks for a random one, so a structured channel can drift towards chaos with a single dial.

## Lineups (the real-TV way)

A lineup has these parts. Everything is optional.

```json
{
  "timezone": "America/Toronto",
  "grid_minutes": 30,
  "blocks": [ ... ],
  "seasons": [ ... ],
  "events": [ ... ],
  "fill": { "type": "pool", "shows": [], "order": "sequential" },
  "catch_up": "resume"
}
```

- **timezone**: IANA name. All times below are local to it, so 8pm stays 8pm across daylight saving. If you leave it blank the server's own time zone is used.
- **grid_minutes**: 5, 10, 15, 20, 30 or 60. Every show starts on the grid. A 22-minute episode in a 30-minute grid is followed by 8 minutes of filler (the station slate today; commercials and bumpers from M4). With no blocks and no grid, items simply play back to back.
- **fill**: what airs when no block covers the time. Defaults to the strategy's own behaviour.

### Sources: where a slot's programming comes from

| `type` | Meaning |
|---|---|
| `show` | One show's episodes in order. A cursor remembers where it is. |
| `pool` | A list of shows (empty = all). `order` is `sequential` (round-robin, episodes in order) or `shuffle` (a bag). |
| `chaos` | Anything, random. |
| `rotation` | The slot's programming changes over time. `advance` is `{"weeks": N}` (swap every N weeks) or `"finale"` (swap when the current show finishes a full pass). This is how you introduce a new show, rotate a lineup, or retire a show. |

### Blocks (dayparts and weekly slots)

```json
{ "name": "Saturday morning cartoons", "days": [5], "start": "07:00", "end": "12:00",
  "source": { "type": "pool", "shows": ["Looney Tunes", "Tom and Jerry"], "order": "shuffle" } }
```

`days` is 0 = Monday to 6 = Sunday (empty = every day). `end` may be `"24:00"`. An end before the start runs overnight (the days refer to the day it starts).

A block decides what *starts* inside it. A show that starts near the end of a block finishes even if it runs past the end; the next block's first show starts on the next grid time.

### Seasons

```json
{ "name": "Fall lineup", "from": "09-01", "to": "05-31", "blocks": [ ... ] }
```

While a season is active its blocks replace the base blocks (anything it does not cover falls through to the base blocks). `from` and `to` are `MM-DD` and may wrap the year. Use seasons for fall/winter vs summer reruns, or midseason premieres.

### Events

```json
{ "name": "Christmas marathon", "start": "2026-12-25T08:00", "end": "2026-12-25T22:00",
  "yearly": true, "source": { "type": "show", "show": "A Christmas Story" } }
```

Events override everything while they run, including seasons and blocks. `yearly` repeats them every year (holiday specials). Events keep their **own episode cursors**: a marathon starts at the first episode and does not disturb where the regular rotation is.

### Resolution order

At each slot start: **events, then the active season's blocks, then the base blocks, then fill.**

## Autopilot: running with nobody there

**Endless.** A background loop tops each channel's schedule up to 72 hours ahead, every minute. It never "runs out".

**Cursors that never skip or repeat.** Each show (and pool, and event) has a *reserved* cursor (what is on the schedule) and a *committed* cursor (what has actually started airing). Changing programming re-rolls only the future and rebuilds the reservations from what really aired, so episodes are neither skipped nor repeated. See them at `GET /api/v1/channels/{id}/cursors`.

**When a show runs out** (`run_out` on the channel):

| Policy | After the last episode |
|---|---|
| `keep_going` (default) | Wraps to episode 1 and carries on, forever. |
| `next_in_rotation` | Same as `keep_going` for plain shows; `finale` rotations hand over to the next show. |
| `chaos_fill` | Stops repeating that show; the slot airs random content instead. New episodes added to the library resume the sequence. |
| `hybrid` | `chaos_fill`, and time no block covers also airs chaos instead of the plain fill. |

**Fallback ladder.** If a slot's source has nothing (show deleted, library empty), the scheduler falls back to chaos or the fill programming. If nothing at all is playable it airs filler. At playout time, a file that is missing or fails to encode is covered by the station slate for its scheduled time (the guide stays accurate), flagged missing, and the schedule is re-rolled within seconds so it is not scheduled again. Rescanning the library brings a restored file back.

**Downtime.** The autopilot loop records a heartbeat. After the daemon (or the machine) has been off:

- `catch_up: "resume"` (default): carry on from the last episode that really aired. Nothing is skipped.
- `catch_up: "advance"`: act as if the channel kept airing; episodes advance by the missed airtime (up to 14 days, then it resumes).

**Autopilot vs manual lock.** `autopilot: true` (default): editing a channel's programming re-rolls its future automatically. `autopilot: false`: edits are saved but the already-materialized schedule is kept until you call `POST /api/v1/channels/{id}/reroll`. Either way the schedule is topped up so the channel never runs dry.

**Re-rolls are gentle.** Anything starting within the next five minutes stays (the show on air keeps its padding and what is already encoded behind it is untouched); the rest is regenerated, cutting at a show boundary so the grid stays aligned.

## A complete example

```json
{
  "id": "network", "number": 12, "name": "The Network",
  "strategy": "block_grid", "run_out": "keep_going", "chaos": 5, "library_ids": [1],
  "lineup": {
    "timezone": "America/Toronto",
    "grid_minutes": 30,
    "blocks": [
      { "name": "Saturday morning", "days": [5], "start": "07:00", "end": "12:00",
        "source": { "type": "pool", "shows": ["Looney Tunes", "Tom and Jerry"], "order": "shuffle" } },
      { "name": "Weeknight sitcoms", "days": [0,1,2,3,4], "start": "19:00", "end": "22:00",
        "source": { "type": "rotation", "advance": { "weeks": 4 },
                    "items": [ { "type": "show", "show": "Cheers" }, { "type": "show", "show": "Taxi" } ] } },
      { "name": "Late night", "start": "23:00", "end": "02:00", "source": { "type": "chaos" } }
    ],
    "seasons": [
      { "name": "Summer reruns", "from": "06-01", "to": "08-31",
        "blocks": [ { "start": "19:00", "end": "22:00", "source": { "type": "show", "show": "Cheers" } } ] }
    ],
    "events": [
      { "name": "New Year's marathon", "start": "2026-12-31T12:00", "end": "2027-01-01T02:00",
        "yearly": true, "source": { "type": "show", "show": "Cheers" } }
    ],
    "catch_up": "resume"
  }
}
```

## Limits today

- Commercials and bumpers are not inserted yet (M4). Grid padding is the plain station slate.
- Only whole shows are scheduled (no mid-show breaks yet).
- Show names in `show` sources match the show name parsed from your folders, case-insensitively.
- Rotation `finale` only works with `show` items.
- Shows added to a library shift episode positions for that show; the cursor keeps its numeric position.
