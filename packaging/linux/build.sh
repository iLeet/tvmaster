#!/usr/bin/env bash
# Builds the .deb, .rpm and Arch package for one architecture.
#   packaging/linux/build.sh <path-to-tvmaster-binary> <amd64|arm64> [out-dir]
# Needs Docker (it runs nfpm from its official image) and is run from the repo root.
set -euo pipefail
BIN="$1"; ARCH="${2:-amd64}"; OUT="${3:-dist}"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
mkdir -p "$OUT"
WORK="$(mktemp -d)"
cp "$BIN" "$WORK/tvmaster"
cp -R packaging docs README.md "$WORK/"
for fmt in deb rpm archlinux; do
  mkdir -p "$WORK/out-$fmt"
  docker run --rm --user "$(id -u):$(id -g)" -e VERSION="$VERSION" -e ARCH="$ARCH" -v "$WORK":/pkg -w /pkg \
    goreleaser/nfpm:latest package --packager "$fmt" -f packaging/linux/nfpm.yaml --target "/pkg/out-$fmt"
done
cp "$WORK"/out-*/* "$OUT"/
rm -rf "$WORK"
ls -la "$OUT"
