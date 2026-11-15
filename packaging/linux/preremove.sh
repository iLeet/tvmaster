#!/bin/sh
# deb passes "remove"/"upgrade"; rpm passes the number of copies left (0 = removing).
case "${1:-remove}" in
  upgrade|1|2) exit 0 ;;
esac
if [ -d /run/systemd/system ] && command -v systemctl >/dev/null 2>&1; then
  systemctl disable --now tvmaster.service >/dev/null 2>&1 || true
fi
exit 0
