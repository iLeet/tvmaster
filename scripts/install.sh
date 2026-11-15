#!/usr/bin/env bash
# Installs the TVMaster daemon on macOS or Linux, from an unpacked release.
#   ./install.sh                     install to ~/.local/bin
#   ./install.sh --service           also run it at login/boot (launchd or systemd, as you)
#   ./install.sh --prefix /usr/local --data-dir /srv/tvmaster --bind 0.0.0.0:8787
#   ./install.sh --dry-run           show what would happen
# It never needs root unless you point --prefix somewhere root owns.
set -euo pipefail

PREFIX="${HOME}/.local"
DATA_DIR=""
BIND="0.0.0.0:8787"
SERVICE=0
DRY=0
HERE="$(cd "$(dirname "$0")" && pwd)"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix)   PREFIX="$2"; shift 2 ;;
    --data-dir) DATA_DIR="$2"; shift 2 ;;
    --bind)     BIND="$2"; shift 2 ;;
    --service)  SERVICE=1; shift ;;
    --dry-run)  DRY=1; shift ;;
    -h|--help)  sed -n '2,8p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

OS="$(uname -s)"
case "$OS" in
  Darwin) DATA_DIR="${DATA_DIR:-$HOME/Library/Application Support/TVMaster}" ;;
  Linux)  DATA_DIR="${DATA_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/tvmaster}" ;;
  *) echo "This installer is for macOS and Linux. On Windows use install.ps1." >&2; exit 1 ;;
esac

run() { if [[ $DRY == 1 ]]; then echo "+ $*"; else "$@"; fi; }

BIN_SRC="$HERE/tvmaster"
[[ -x "$BIN_SRC" ]] || { echo "tvmaster not found next to this script ($HERE)" >&2; exit 1; }
BIN="$PREFIX/bin/tvmaster"

if ! command -v ffmpeg >/dev/null 2>&1 || ! command -v ffprobe >/dev/null 2>&1; then
  echo "Warning: ffmpeg and ffprobe were not found on your PATH. TVMaster needs them to stream."
  case "$OS" in
    Darwin) echo "         Install them with: brew install ffmpeg" ;;
    Linux)  echo "         Install them with your package manager, e.g. sudo apt install ffmpeg" ;;
  esac
fi

run mkdir -p "$PREFIX/bin" "$DATA_DIR"
run install -m 755 "$BIN_SRC" "$BIN"
echo "Installed $BIN"

if [[ $SERVICE == 1 ]]; then
  # A token keeps the rest of the network out; generate one the first time.
  TOKEN_FILE="$DATA_DIR/token"
  if [[ ! -f "$TOKEN_FILE" ]]; then
    if [[ $DRY == 1 ]]; then echo "+ write a new token to $TOKEN_FILE"; else
      ( umask 077; "$BIN" token > "$TOKEN_FILE" )
    fi
  fi
  case "$OS" in
    Darwin)
      PLIST="$HOME/Library/LaunchAgents/com.tvmaster.daemon.plist"
      run mkdir -p "$HOME/Library/LaunchAgents"
      if [[ $DRY == 1 ]]; then echo "+ write $PLIST"; else
        "$BIN" service launchd --bin "$BIN" --data-dir "$DATA_DIR" --bind "$BIND" --token-file "$TOKEN_FILE" > "$PLIST"
      fi
      run launchctl unload "$PLIST" 2>/dev/null || true
      run launchctl load "$PLIST"
      echo "Started with launchd: $PLIST"
      ;;
    Linux)
      UNIT_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
      run mkdir -p "$UNIT_DIR"
      if [[ $DRY == 1 ]]; then echo "+ write $UNIT_DIR/tvmaster.service"; else
        "$BIN" service systemd --bin "$BIN" --data-dir "$DATA_DIR" --bind "$BIND" --token-file "$TOKEN_FILE" \
          | sed 's/multi-user.target/default.target/' > "$UNIT_DIR/tvmaster.service"
      fi
      run systemctl --user daemon-reload
      run systemctl --user enable --now tvmaster.service
      echo "Started with systemd (user): tvmaster.service"
      echo "To keep it running when you are logged out: loginctl enable-linger \"$USER\""
      ;;
  esac
  echo
  echo "Open http://localhost:${BIND##*:}/admin/"
  echo "Everything on the network needs the access token in $TOKEN_FILE (the admin page asks for it)."
else
  echo
  echo "Run it with:  $BIN serve --data-dir \"$DATA_DIR\""
  echo "Run it at login/boot with:  $0 --service"
fi
