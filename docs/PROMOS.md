# Promos and commercials of your own

**Promos** (web admin and Mac app) makes spots as video files and puts them in your commercials, so they
air in breaks like any commercial.

## Make a spot

- **Sponsor or advert:** a big name, a tagline and a contact line ("Mel's Diner / Best pie in town / Call 555-0134").
- **Programme promo:** "Coming up", the show's name and when it is on.
- **Slides:** a few lines of text, one after another.

Choose a length (10-60 seconds), a picture to drift slowly behind the words, the station's theme or a look of
its own, **Make it look old** (VHS tape, tube TV, old film, black and white, sepia), and a sound: a little
jingle made on the spot, music from your music library, or silence. A preview redraws as you type.

Spots are saved in the station's commercials under `Station promos/`. Give a spot a **year** and it is filed in
`Station promos/<year>/`, so a channel that only airs ads from certain years (an era) can use it. A spot with
no year is skipped by an era filter. Delete a spot under **Made here**; it leaves the schedule too.

## Old commercials

- Upload your own files (Libraries, your commercials library, Upload), by drag and drop too.
- Or give the server a **direct link to a video file** and it downloads it into a commercials library
  (`POST /api/v1/libraries/:id/fetch`, `{ "url": "https://..." }`, http and https only, up to 4 GB; needs `curl`
  on the server). The Prelinger Archives and the Internet Archive have public-domain commercials and station
  breaks from the 1950s to the 80s; use a file's direct download link.
- Put a year in the folder name (`Commercials/1986/`) to let channels pick an era.
- To make anything look like it came off a tape, set the channel's **Look** to VHS or Tube TV under Sound & picture.

## API

- `GET /api/v1/promos/options`, `GET /api/v1/promos/music`, `GET /api/v1/promos`
- `POST /api/v1/promos/preview` (a PNG) and `POST /api/v1/promos` (make it) with
  `{ "kind": "sponsor|coming_up|slides", "title", "tagline", "contact", "slides": [], "seconds": 15, "picture": null, "theme": null, "look": null, "sound": "jingle|music|none", "music": null, "year": null }`
- `DELETE /api/v1/promos/:id`
