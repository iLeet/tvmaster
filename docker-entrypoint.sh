#!/bin/sh
# Starts TVMaster inside the container.
#  - Runs as the user in PUID/PGID (default 1000:1000) so the files it writes to /data and
#    /media (uploads, the database) belong to you on the host, not to root.
#  - On the first start it makes an access token and prints it, so a server you put on your
#    network is never open by accident. Switch it off or replace it in Settings, or set
#    TVMASTER_TOKEN yourself (fixed) or TVMASTER_NO_FIRST_TOKEN=1 to start open.
#  - Any other command (`docker run tvmaster token`, `doctor`) is passed straight to tvmaster.
set -eu

if [ "${1:-}" != "serve" ]; then
  exec tvmaster "$@"
fi
shift

PUID="${PUID:-1000}"
PGID="${PGID:-1000}"
DATA="${TVMASTER_DATA_DIR:-/data}"
mkdir -p "$DATA"

run() {
  if [ "$(id -u)" = 0 ]; then
    chown "$PUID:$PGID" "$DATA" 2>/dev/null || true
    # Only walk the tree when something in it is not ours (a fresh volume is quick either way).
    find "$DATA" -not -user "$PUID" -exec chown "$PUID:$PGID" {} + 2>/dev/null || true
    exec setpriv --reuid="$PUID" --regid="$PGID" --clear-groups env HOME="$DATA" "$@"
  fi
  exec "$@"
}

ARGS=""
if [ -z "${TVMASTER_TOKEN:-}" ] && [ -z "${TVMASTER_TOKEN_FILE:-}" ] && [ -z "${TVMASTER_NO_FIRST_TOKEN:-}" ]; then
  FILE="$DATA/first-run-token"
  if [ ! -s "$FILE" ]; then
    ( umask 077; tvmaster token > "$FILE" )
    chown "$PUID:$PGID" "$FILE" 2>/dev/null || true
    echo "================================================================"
    echo " TVMaster access token (first run): $(cat "$FILE")"
    echo " Sign in with it, or turn the token off in Settings."
    echo " It is also in $FILE inside the data volume."
    echo "================================================================"
  fi
  ARGS="--initial-token-file $FILE"
fi

# shellcheck disable=SC2086
run tvmaster serve $ARGS "$@"
