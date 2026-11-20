#!/usr/bin/env bash
# Install the Samsung/Tizen command-line tools needed to build, sign and install the app.
# Safe to run again: it skips what is already installed. Everything goes in $TIZEN_HOME
# (default ~/tizen-studio) and nothing else on the machine is touched (no shell profile edits).
set -euo pipefail

TIZEN_HOME="${TIZEN_HOME:-$HOME/tizen-studio}"
VERSION="${TIZEN_STUDIO_VERSION:-6.1}"
BASE="https://download.tizen.org/sdk/Installer/tizen-studio_${VERSION}"
PACKAGES=(TV-SAMSUNG-Public-WebAppDevelopment Certificate-Manager cert-add-on)

case "$(uname -s)" in
  Darwin) FILE="web-cli_Tizen_Studio_${VERSION}_macos-64.bin" ;;
  Linux)  FILE="web-cli_Tizen_Studio_${VERSION}_ubuntu-64.bin" ;;
  *) echo "Unsupported OS: $(uname -s) (Tizen Studio CLI exists for macOS, Linux and Windows; use the Windows installer by hand)"; exit 1 ;;
esac

if [ ! -x "$TIZEN_HOME/package-manager/package-manager-cli.bin" ]; then
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' EXIT
  echo "Downloading $FILE (about 270 MB) ..."
  curl -fL --progress-bar -o "$work/$FILE" "$BASE/$FILE"
  chmod +x "$work/$FILE"
  echo "Installing Tizen Studio CLI to $TIZEN_HOME ..."
  # Run from the home folder, not from the download folder.
  (cd "$HOME" && "$work/$FILE" --accept-license "$TIZEN_HOME")
else
  echo "Tizen Studio CLI already installed in $TIZEN_HOME"
fi

echo "Installing TV extension and certificate tools ..."
(cd "$HOME" && "$TIZEN_HOME/package-manager/package-manager-cli.bin" install --accept-license "${PACKAGES[@]}")

cat <<EOF

Tools are installed. One manual step remains, once per year (see README.md, "Certificate"):
create a Samsung certificate profile for your TV in Certificate Manager:

  open "$TIZEN_HOME/tools/certificate-manager/certificate-manager.app"

Close Certificate Manager afterwards: while it is open it holds the TV's only debug connection.
EOF
