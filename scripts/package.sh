#!/usr/bin/env bash
# Builds a release archive of the TVMaster daemon.
#   scripts/package.sh [target-triple] [out-dir]
# Writes tvmaster-<version>-<target>.tar.gz (.zip for Windows targets) and a
# .sha256 next to it. Without a target it builds for this machine.
set -euo pipefail

cd "$(dirname "$0")/.."
TARGET="${1:-$(rustc -vV | sed -n 's/^host: //p')}"
OUT="${2:-dist}"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
NAME="tvmaster-${VERSION}-${TARGET}"

cargo build --release --locked -p tvmaster-cli --target "$TARGET"

case "$TARGET" in
  *windows*) EXE="tvmaster.exe" ;;
  *)         EXE="tvmaster" ;;
esac

STAGE="$(mktemp -d)/${NAME}"
mkdir -p "$STAGE" "$OUT"
cp "target/${TARGET}/release/${EXE}" "$STAGE/"
cp README.md "$STAGE/"
cp scripts/install.sh scripts/install.ps1 "$STAGE/"
cp -R docs "$STAGE/docs"

ABS_OUT="$(cd "$OUT" && pwd)"
case "$TARGET" in
  *windows*)
    ARCHIVE="${NAME}.zip"
    (cd "$(dirname "$STAGE")" && 7z a -tzip -bso0 "${ABS_OUT}/${ARCHIVE}" "${NAME}" 2>/dev/null \
      || zip -qr "${ABS_OUT}/${ARCHIVE}" "${NAME}")
    ;;
  *)
    ARCHIVE="${NAME}.tar.gz"
    tar -C "$(dirname "$STAGE")" -czf "${ABS_OUT}/${ARCHIVE}" "${NAME}"
    ;;
esac

(cd "$ABS_OUT" && { shasum -a 256 "$ARCHIVE" 2>/dev/null || sha256sum "$ARCHIVE"; } > "${ARCHIVE}.sha256")
echo "Wrote ${ABS_OUT}/${ARCHIVE}"
