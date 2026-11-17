# Backup and restore

**Settings → Backup & restore** (web admin and Mac app).

## What is in a backup

Channels and their lineups, picture and sound settings, libraries (their folders and what was found in them,
including ad-break detection), show names and details, episode positions, the station theme and every other
setting, and your uploaded pictures. **Not** your video files, and **not** this server's access token or its
open-viewing choice: those belong to the machine, so a backup can be shared without giving access.

A backup is one `.tar.gz` file in `<data folder>/backups/` (a manifest, a copy of the database and the pictures).

## Back up

- **Back up now**, with an optional name.
- **Every night** (on by default): one is made when the newest is a day old, and the newest 7 are kept (1-60).
- **Download** a backup to keep it somewhere else; **Add a backup file** puts one from elsewhere on the server.

## Restore

**Restore** puts the station back as the backup had it. First it saves what is there now (as `...-before-restore`),
stops the streams, and afterwards plans every schedule again. This server's access token is left alone.

Moving to another machine: install TVMaster there, add the backup file, restore it. Library folders that are
not on the new machine are listed in the result; their files stay in the lists but cannot play until the folder is
there (or you add the library again). Pictures and the station media folder follow the new data folder.

## API

`GET/POST /api/v1/backups`, `PUT /api/v1/backups/settings` (`auto`, `keep`), `GET/DELETE /api/v1/backups/:name`,
`POST /api/v1/backups/upload?name=...` (the body is the file), `POST /api/v1/backups/:name/restore`.
