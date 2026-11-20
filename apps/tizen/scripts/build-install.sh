#!/usr/bin/env bash
# Build, sign, install and start TVMaster Guide on a Samsung TV in developer mode.
#
#   scripts/build-install.sh --tv 10.0.0.243 [--server http://10.0.0.251:8787] [--profile TVDev] [--no-launch]
#
# --tv       the TV's IP address (its Developer Mode "Host PC IP" must be this computer's address)
# --server   TVMaster address built into the app as its default (it can also be changed on the TV with the Red key)
# --profile  the security profile made in Certificate Manager (default TVDev)
#
# Environment: TIZEN_HOME (default ~/tizen-studio), TV, SERVER, PROFILE can be used instead of the flags.
set -euo pipefail

HERE="$(cd "$(dirname "$0")/.." && pwd)"
TIZEN_HOME="${TIZEN_HOME:-$HOME/tizen-studio}"
TV="${TV:-}"
SERVER="${SERVER:-}"
PROFILE="${PROFILE:-TVDev}"
LAUNCH=1
APP_ID="TVMGuide01.Guide"

while [ $# -gt 0 ]; do
  case "$1" in
    --tv) TV="$2"; shift 2 ;;
    --server) SERVER="$2"; shift 2 ;;
    --profile) PROFILE="$2"; shift 2 ;;
    --no-launch) LAUNCH=0; shift ;;
    -h|--help) sed -n '2,12p' "$0"; exit 0 ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

[ -n "$TV" ] || { echo "Give the TV's address: --tv 10.0.0.243"; exit 1; }
export PATH="$TIZEN_HOME/tools:$TIZEN_HOME/tools/ide/bin:$PATH"
command -v tizen >/dev/null || { echo "Tizen tools not found in $TIZEN_HOME. Run scripts/setup-tools.sh first."; exit 1; }

if pgrep -f certificate-manager.app >/dev/null 2>&1; then
  echo "Certificate Manager is open: it holds the TV's debug connection. Quit it (Cmd+Q) and run this again."; exit 1
fi

echo "Connecting to $TV ..."
sdb connect "$TV" >/dev/null 2>&1 || true
DEV="$(sdb devices | awk -v ip="$TV" '$1 ~ "^" ip ":" && $2 == "device" {print $1}')"
[ -n "$DEV" ] || { echo "Could not connect to $TV over sdb. Check: TV on, Developer Mode on, its Host PC IP is this computer, Certificate Manager closed."; exit 1; }
echo "Connected: $DEV"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir "$work/app" "$work/out"
cp -R "$HERE/src/." "$work/app/"
if [ -n "$SERVER" ]; then
  # Rewrites the built-in default address in config.js (the line carries a /*SERVER*/ marker).
  perl -pi -e "s#const DEFAULT_SERVER = '[^']*'; /\*SERVER\*/#const DEFAULT_SERVER = '$SERVER'; /*SERVER*/#" "$work/app/js/config.js"
  grep -q "DEFAULT_SERVER = '$SERVER'" "$work/app/js/config.js" || { echo "Could not set the server address"; exit 1; }
fi

echo "Packaging and signing with profile $PROFILE ..."
(cd "$work" && tizen package -t wgt -s "$PROFILE" -o "$work/out" -- "$work/app" >/dev/null)
SRC_WGT="$(cd "$work/out" && ls *.wgt | head -1)"
[ -n "$SRC_WGT" ] || { echo "Packaging failed"; exit 1; }
# The TV's installer fails on file names with spaces ("TVMaster Guide.wgt"), so use a plain name.
cp "$work/out/$SRC_WGT" "$work/out/tvmguide.wgt"

echo "Installing ..."
if ! log="$(tizen install -n tvmguide.wgt -s "$DEV" -- "$work/out" 2>&1)"; then
  echo "$log"; echo "Install failed. If the message mentions a certificate, see README.md (Certificate)."; exit 1
fi
echo "$log" | tail -2

if [ "$LAUNCH" = 1 ]; then
  echo "Starting ..."
  tizen run -p "$APP_ID" -s "$DEV" | tail -2
fi
echo "Done. Built-in server: ${SERVER:-(unchanged: see src/js/config.js)}"
