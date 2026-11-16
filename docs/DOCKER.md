# Running TVMaster in Docker

The image is the same server as the Mac app's built-in one and the packages: the web admin, the player, the IPTV playlist, tuner emulation and the API, with FFmpeg inside. Your phone, tablet or Mac app connects to it over the network like any other TVMaster server.

## Quick start

```bash
mkdir tvmaster && cd tvmaster          # or use a checkout of the repository
# put docker-compose.yml here (from the repository), edit TZ, PUID and PGID, then:
mkdir -p media                          # your shows, commercials, bumpers and music go in here
docker compose up -d
docker compose logs tvmaster            # the first start prints the access token
```

Open `http://<this machine>:8787/admin/`, sign in with that token (or turn the token off in **Settings**), and add libraries. In the Mac app, **Settings → Server** takes the same address and token.

Without Compose:

```bash
docker build -t tvmaster .
docker run -d --name tvmaster --restart unless-stopped -p 8787:8787 \
  -e TZ=America/Toronto -e PUID=$(id -u) -e PGID=$(id -g) \
  -v tvmaster-data:/data -v "$PWD/media":/media tvmaster
```

## What lives where

| Path in the container | What | Mount it as |
|---|---|---|
| `/data` | the database, caches, uploaded pictures, the first-run token, the HTTPS certificate | a named volume (the default) or a folder you back up |
| `/media` | your libraries. Uploads from the apps are written here | a folder on the host, **read-write** if you want to upload from the apps (read-only if you only point it at files that already exist) |

Libraries are folders as the container sees them: add `/media/shows`, `/media/bumpers` and so on. **Add library → "Make the folder if it does not exist"** creates one, and **Upload** on a library copies videos from the device you are using into it (see [API.md](API.md), `PUT /api/v1/libraries/{id}/files`). Mount more than one folder if your collection lives in several places (`/mnt/tv:/media/tv:ro`, `/mnt/ads:/media/ads:ro`) and add each as a library.

## Settings

Environment variables on the container:

| Variable | |
|---|---|
| `TZ` | Your time zone (`America/Toronto`). Channel schedules and the guide use it. |
| `PUID`, `PGID` | The user and group that own what TVMaster writes, so uploads and the data folder belong to you on the host. Default 1000:1000; `id` tells you yours. The server itself never runs as root. |
| `TVMASTER_TOKEN` | Fix the access token (it cannot then be changed in Settings). Otherwise a token is made on the first start and printed in the logs. |
| `TVMASTER_NO_FIRST_TOKEN=1` | Start with no token. Turn one on in Settings. Only do this on a network you trust. |
| `TVMASTER_TLS=1` | Also serve HTTPS on 8788 (publish that port too). See [RELEASE.md](RELEASE.md). |
| `TVMASTER_ENCODER` | `software` (default for containers on a CPU), or a GPU encoder: see below. |

Any other `tvmaster serve` flag has an environment variable (`TVMASTER_*`) or can be appended to the command (`docker run … tvmaster serve --bind 0.0.0.0:8787 --idle-seconds 120`).

The first-run token is kept in `/data/first-run-token`. Once you have turned the token off or replaced it in Settings, your choice is kept in the database and the file is ignored.

## GPU encoding

FFmpeg in the image is Debian's. With **software** encoding a modern CPU handles several channels (run `docker exec tvmaster tvmaster doctor` to see how fast yours is).

- **NVIDIA:** install the NVIDIA Container Toolkit on the host, give the container the GPU (the `deploy` block in `docker-compose.yml`, or `--gpus all`), and set `TVMASTER_ENCODER=nvenc`. `doctor` then shows whether it works.
- **Intel / AMD (VAAPI):** pass `/dev/dri` (`devices: ["/dev/dri:/dev/dri"]`), add the user to the `render` group (`group_add: ["<render gid>"]`), install the driver for your GPU in a derived image (`intel-media-va-driver` or `mesa-va-drivers`), and set `TVMASTER_ENCODER=vaapi`. This path has unit tests for its FFmpeg arguments but has not been run against a real GPU.

## Updating, backing up

```bash
docker compose pull   # if you push the image to a registry; otherwise:
git pull && docker compose build && docker compose up -d
```

Back up the `tvmaster-data` volume (`docker run --rm -v tvmaster-data:/d -v "$PWD":/b alpine tar czf /b/tvmaster-data.tgz -C /d .`) and your media folders. Data is migrated forward automatically on start.

## What is tested

`scripts/docker-test.sh` builds the image and checks it end to end: the server starts and answers, the first-run token is printed once and keeps working across a restart, the server runs as your uid and not root, a video uploaded through the API lands in the mounted folder owned by you and is indexed, a channel streams, the IPTV playlist is served, Docker reports the container healthy, it stops cleanly, `TVMASTER_TOKEN` and `TVMASTER_NO_FIRST_TOKEN` behave, and `docker run tvmaster token` and `doctor` work. It was run on a Linux x86_64 machine; the image has not been run on ARM or with a GPU.

## Behind a reverse proxy

TVMaster speaks plain HTTP on 8787. A reverse proxy (Caddy, nginx, Traefik) in front can add HTTPS with a real certificate; pass `X-Forwarded-Proto: https` so the links the server writes say `https://`, and allow long uploads (disable the request-size limit and buffering for `/api/v1/libraries/*/files`). `tailscale serve` is the easiest way to reach it from away from home.
