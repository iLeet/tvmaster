#!/bin/sh
# Runs after the package is installed or upgraded (deb, rpm and Arch).
set -e

if ! getent passwd tvmaster >/dev/null 2>&1; then
  useradd --system --user-group --home-dir /var/lib/tvmaster --no-create-home \
    --shell /usr/sbin/nologin --comment "TVMaster" tvmaster 2>/dev/null \
  || useradd --system --user-group --home-dir /var/lib/tvmaster --no-create-home \
    --shell /sbin/nologin --comment "TVMaster" tvmaster
fi

# Hardware encoders need access to the GPU's device nodes.
for g in video render; do
  if getent group "$g" >/dev/null 2>&1; then usermod -aG "$g" tvmaster 2>/dev/null || true; fi
done

# An access token, made once. The service refuses to start without one so the
# server is never open to the whole network by accident.
mkdir -p /etc/tvmaster
if [ ! -s /etc/tvmaster/token ]; then
  ( umask 077; /usr/bin/tvmaster token > /etc/tvmaster/token )
fi
chown root:tvmaster /etc/tvmaster/token
chmod 640 /etc/tvmaster/token

started=no
if [ -d /run/systemd/system ] && command -v systemctl >/dev/null 2>&1; then
  systemctl daemon-reload || true
  systemctl enable tvmaster.service >/dev/null 2>&1 || true
  # restart (not start) so an upgrade runs the new binary
  if systemctl restart tvmaster.service; then started=yes; fi
fi

echo
echo "TVMaster is installed."
[ "$started" = yes ] && echo "  It is running; it starts at boot and restarts if it stops."
echo "  Open        http://$(hostname 2>/dev/null || echo localhost):8787/admin/"
echo "  Access token  sudo cat /etc/tvmaster/token"
echo "  Settings      /etc/tvmaster/tvmaster.env"
echo "  Hardware      tvmaster doctor"
echo
