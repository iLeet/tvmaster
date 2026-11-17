# Show titles and grouping

What a folder is called is the show's *detected* name. What the station calls it is its *title*, and that
is what the guide, the schedule, lineups and the apps show.

## Automatic tidy-up

"Bleach season 01", "Bleach - Season 2", "Bleach (Season 3)", "Bleach.Series.4" and "Bleach S5" are the
show **Bleach**, with the season taken from the folder. It is on by default; switch it off on the Shows page
(`PUT /api/v1/shows/auto-clean`) and every folder is its own show again.

## By hand

On **Shows**, *Name & details* renames a show, shows the folders it is made of, and lets you number each
folder's season. Tick several shows and *Put together* to make them one show. Folders without a season
number mix by episode number, so give each its season.

Renaming never touches your files. Lineups, each channel's per-show settings, show settings and episode
positions follow the new name; the future schedule is re-planned.

## Details

A show can have a description, a rating (free text such as TV-PG), tags and a poster. The description,
tags and rating appear in `/iptv/guide.xml`.

## API

- `GET /api/v1/shows` - each show has `name`, `folders` (`raw`, `episodes`, `season`, `custom`) and `info`.
- `PUT /api/v1/shows/folder` - `{ "raw": "Bleach season 01", "title": "Bleach", "season": 1 }`
- `POST /api/v1/shows/folder/reset` - `{ "raw": "..." }` forgets a rule.
- `POST /api/v1/shows/merge` - `{ "shows": ["A", "B"], "title": "AB" }` (one show: a rename).
- `PUT /api/v1/shows/info` - `{ "show": "Bleach", "info": { "description": "", "rating": "", "tags": [], "poster": null } }`
- `GET /api/v1/shows/names`, `PUT /api/v1/shows/auto-clean`

## Media you upload

`POST /api/v1/media/station-folder` makes `<data folder>/media/{shows,commercials,bumpers,music}` and a
library for each; upload into them from the web admin or the Mac app (drag and drop works). Libraries that
link to other folders keep working beside it.
