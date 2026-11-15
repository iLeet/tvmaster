#!/bin/zsh
# Builds "TVMaster.app" from the Swift package.
#   Scripts/build-app.sh                 release build for this Mac
#   UNIVERSAL=1 Scripts/build-app.sh     arm64 + x86_64
#   CONFIG=debug Scripts/build-app.sh
# If ../../target/release/tvmaster exists (cargo build --release -p tvmaster-cli) it is bundled,
# so the app can run its own server.
set -euo pipefail

cd "${0:A:h}/.."
CONFIG="${CONFIG:-release}"
APP_NAME="TVMaster"
BUNDLE_ID="app.tvmaster.mac"
VERSION="$(cat VERSION)"
OUT="build/${APP_NAME}.app"

ARCHS=()
[[ "${UNIVERSAL:-0}" == "1" ]] && ARCHS=(--arch arm64 --arch x86_64)

swift build -c "$CONFIG" "${ARCHS[@]}"
BIN="$(swift build -c "$CONFIG" "${ARCHS[@]}" --show-bin-path)/TVMaster"

rm -rf "$OUT"
mkdir -p "$OUT/Contents/MacOS" "$OUT/Contents/Resources"
cp "$BIN" "$OUT/Contents/MacOS/TVMaster"

# Bundled server (optional)
SERVER="../../target/release/tvmaster"
if [[ -x "$SERVER" ]]; then
  cp "$SERVER" "$OUT/Contents/Resources/tvmaster"
  echo "Bundled the server from $SERVER"
else
  echo "Note: no server binary at $SERVER; the app will look for an installed one."
fi

# App icon: build an .icns from Resources/AppIcon.png
ICON_KEYS=""
if [[ -f Resources/AppIcon.png ]]; then
  ICONSET="$(mktemp -d)/AppIcon.iconset"
  mkdir -p "$ICONSET"
  for s in 16 32 128 256 512; do
    sips -z $s $s Resources/AppIcon.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
    sips -z $((s*2)) $((s*2)) Resources/AppIcon.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
  done
  iconutil -c icns "$ICONSET" -o "$OUT/Contents/Resources/AppIcon.icns"
  ICON_KEYS='<key>CFBundleIconFile</key><string>AppIcon</string>'
fi

cat > "$OUT/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleExecutable</key><string>TVMaster</string>
  <key>CFBundleIdentifier</key><string>${BUNDLE_ID}</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>${APP_NAME}</string>
  <key>CFBundleDisplayName</key><string>${APP_NAME}</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  ${ICON_KEYS}
  <key>LSMinimumSystemVersion</key><string>15.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.entertainment</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSLocalNetworkUsageDescription</key><string>TVMaster connects to your TV station server on your network.</string>
  <key>NSAppTransportSecurity</key>
  <dict><key>NSAllowsLocalNetworking</key><true/><key>NSAllowsArbitraryLoads</key><true/></dict>
</dict>
</plist>
PLIST

# Signing. The default is an ad-hoc signature, enough to launch on this Mac.
#   SIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)"   a real, notarizable signature
#   SIGN_IDENTITY="TVMaster Self-Signed" (see Scripts/make-selfsigned-cert.sh)   stable identity, your own Macs only
#   KEYCHAIN=/path/to.keychain-db   look for the identity there instead of your login keychain
IDENTITY="${SIGN_IDENTITY:--}"
KC=()
[[ -n "${KEYCHAIN:-}" ]] && KC=(--keychain "$KEYCHAIN")
SIGN=(codesign --force --sign "$IDENTITY" "${KC[@]}")
# Hardened runtime and a secure timestamp are what notarization requires; they
# need a real identity (an ad-hoc signature cannot be timestamped).
[[ "$IDENTITY" != "-" ]] && SIGN+=(--options runtime --timestamp)
# Sign inside-out: the bundled daemon first, then the app that contains it.
[[ -f "$OUT/Contents/Resources/tvmaster" ]] && "${SIGN[@]}" "$OUT/Contents/Resources/tvmaster"
"${SIGN[@]}" "$OUT"
codesign --verify --strict "$OUT" && echo "Signed with: ${IDENTITY/#-/ad hoc}"
echo "Built $OUT"
