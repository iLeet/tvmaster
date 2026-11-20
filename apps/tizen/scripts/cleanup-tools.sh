#!/usr/bin/env bash
# Remove what setup-tools.sh and build-install.sh put on this computer and on the TV.
# It asks before each step. Your Samsung certificates are kept unless you say otherwise
# (they are tied to your Samsung account and the TV, and are needed to install again).
#
#   scripts/cleanup-tools.sh [--tv 10.0.0.243]
set -uo pipefail

TIZEN_HOME="${TIZEN_HOME:-$HOME/tizen-studio}"
TV=""
while [ $# -gt 0 ]; do case "$1" in --tv) TV="$2"; shift 2 ;; *) echo "Unknown option: $1"; exit 1 ;; esac; done
export PATH="$TIZEN_HOME/tools:$TIZEN_HOME/tools/ide/bin:$PATH"

ask() { read -r -p "$1 [y/N] " a; [ "$a" = y ] || [ "$a" = Y ]; }

if [ -n "$TV" ] && command -v sdb >/dev/null && ask "Uninstall TVMaster Guide from the TV at $TV?"; then
  sdb connect "$TV" >/dev/null 2>&1
  tizen uninstall -p TVMGuide01 -s "$TV:26101" || true
fi
if command -v sdb >/dev/null; then
  [ -n "$TV" ] && sdb disconnect "$TV" >/dev/null 2>&1
  sdb kill-server >/dev/null 2>&1
fi

if [ -d "$TIZEN_HOME" ] && ask "Delete $TIZEN_HOME ($(du -sh "$TIZEN_HOME" 2>/dev/null | cut -f1))?"; then
  rm -rf "$TIZEN_HOME"
fi
for d in "$HOME/tizen-studio-data" "$HOME/.tizen"; do
  [ -e "$d" ] && ask "Delete $d (tool settings and the security profile list)?" && rm -rf "$d"
done
if [ -d "$HOME/SamsungCertificate" ] && ask "ALSO delete your Samsung certificates in ~/SamsungCertificate? (You would need to create them again to install later.)"; then
  rm -rf "$HOME/SamsungCertificate"
fi
echo "Done. On the TV you can also turn Developer Mode off: Apps panel, type 12345, set Developer mode to Off."
