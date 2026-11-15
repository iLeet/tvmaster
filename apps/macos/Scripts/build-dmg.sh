#!/bin/zsh
# Builds a drag-to-Applications disk image from build/TVMaster.app.
#   Scripts/build-dmg.sh                       uses the app as built (run build-app.sh first)
#   NOTARIZE=1 NOTARY_PROFILE=tvmaster Scripts/build-dmg.sh
#       submits the image to Apple and staples the ticket. Needs a Developer ID
#       signature (SIGN_IDENTITY for build-app.sh) and a stored notarytool profile:
#       xcrun notarytool store-credentials tvmaster --apple-id you@example.com --team-id TEAMID
set -euo pipefail

cd "${0:A:h}/.."
APP="build/TVMaster.app"
[[ -d "$APP" ]] || { echo "Build the app first: Scripts/build-app.sh" >&2; exit 1; }
VERSION="$(cat VERSION)"
VOL="TVMaster"
DMG="build/TVMaster-${VERSION}.dmg"

STAGE="$(mktemp -d)/${VOL}"
mkdir -p "$STAGE"
ditto "$APP" "$STAGE/TVMaster.app"
ln -s /Applications "$STAGE/Applications"
cat > "$STAGE/Read me first.txt" <<TXT
TVMaster ${VERSION}

Drag TVMaster to Applications, then open it. It starts its own TV station
server and needs FFmpeg to stream: if you do not have it, the app tells you
and offers to install it (brew install ffmpeg).

If macOS says the app "cannot be opened because the developer cannot be
verified", this build is not signed with an Apple Developer ID: right-click
TVMaster, choose Open, then Open again. You only do this once.
TXT

rm -f "$DMG"
hdiutil create -volname "$VOL" -srcfolder "$STAGE" -ov -format UDZO -fs HFS+ "$DMG" >/dev/null
rm -rf "${STAGE:h}"

# Sign the image too when we have a real identity.
IDENTITY="${SIGN_IDENTITY:--}"
if [[ "$IDENTITY" != "-" ]]; then
  KC=(); [[ -n "${KEYCHAIN:-}" ]] && KC=(--keychain "$KEYCHAIN")
  codesign --force --sign "$IDENTITY" "${KC[@]}" --timestamp "$DMG"
fi

if [[ "${NOTARIZE:-0}" == "1" ]]; then
  [[ "$IDENTITY" != "-" ]] || { echo "Notarization needs SIGN_IDENTITY (a Developer ID)" >&2; exit 1; }
  xcrun notarytool submit "$DMG" --keychain-profile "${NOTARY_PROFILE:?set NOTARY_PROFILE}" --wait
  xcrun stapler staple "$DMG"
fi

hdiutil verify "$DMG" >/dev/null && echo "Built $DMG ($(du -h "$DMG" | cut -f1))"
