#!/bin/sh
# Your data (/var/lib/tvmaster) and token (/etc/tvmaster) are left in place on
# purpose: reinstalling picks up where it stopped. Delete them by hand to start fresh.
if [ -d /run/systemd/system ] && command -v systemctl >/dev/null 2>&1; then
  systemctl daemon-reload >/dev/null 2>&1 || true
fi
exit 0
