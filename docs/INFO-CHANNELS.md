# Info channels

An info channel has nothing to schedule. It draws pages itself, round and round, in the station's look, with a
clock, a ticker and music if you like. Make one from **New channel**: News and headlines, What's on tonight,
Community bulletin, Picture show or Clock (all start from a sensible set of pages).

## Pages

| Page | Shows |
| --- | --- |
| Notice (`text`) | A title, up to twelve lines of your own text and an optional picture beside it. |
| Headlines (`headlines`) | The latest headlines from the channel's news feeds, five to a page. Needs at least one feed. |
| On tonight (`tonight`) | What is on now and what comes next, on every TV channel, five to a page. |
| Clock (`clock`) | The time and date, large. |
| Picture (`photo`) | A picture across the whole screen, with a caption. |

Each page stays up for the channel's time (3-120 s) or its own (3-300 s). The channel also has a heading, a line for
the guide saying what it is ("Local news"), the time and date at the top right (12- or 24-hour), a ticker along the
bottom (your lines, then the headlines) and music from a Music library. Info pages are always drawn smooth at
1280x720; the look follows the station theme, the channel's own, and a day part's.

News feeds are RSS or Atom addresses fetched by the server every fifteen minutes; if a feed cannot be reached the
last headlines stay.

## On the guide

Info channels are listed on guide channels beside the TV channels, by number, each as one long entry saying what
it is. On a guide's settings, choose which channels it lists (both kinds) or leave it empty for all of them. They
also have a programme in the XMLTV listing.

## API

- `POST /api/v1/channels` with `"kind": "info"` and `"info": { "about", "title", "slides": [ { "kind", "title",
  "lines", "picture", "seconds" } ], "seconds", "feeds", "ticker", "ticker_enabled", "clock", "hour12",
  "music_library_ids" }`. Pictures are files on the server (upload them first).
- `GET /api/v1/channel-templates` - the kinds of channel the creator offers, each with `suggested` `id`, `number`
  (the next free one) and `name`, a `body` to start from and what it still `needs`; plus suggested `feeds`.
- `POST /api/v1/info/preview` `{ "info": {...}, "name", "id"?, "theme"?, "page": 0 }` - one page as a PNG;
  the `x-page-count` header says how many pages there are. Nothing is saved.
