#!/bin/sh
set -eu

if [ "$#" -ne 3 ]; then
  echo "usage: $0 VERSION UNIVERSAL_BINARY OUTPUT_DIRECTORY" >&2
  exit 2
fi

: "${SKRIBI_SIGN_IDENTITY:?set SKRIBI_SIGN_IDENTITY to a Developer ID Application identity}"
: "${SKRIBI_NOTARY_PROFILE:?set SKRIBI_NOTARY_PROFILE to an existing notarytool keychain profile}"

VERSION=$1
BINARY=$2
OUTPUT=$3
APP="$OUTPUT/Skribi.app"
CONTENTS="$APP/Contents"
DMG="$OUTPUT/Skribi-$VERSION-macos-universal.dmg"
NOTARY_ZIP="$OUTPUT/Skribi-notary.zip"

[ -f "$BINARY" ] || { echo "binary not found: $BINARY" >&2; exit 1; }
rm -rf "$APP" "$DMG" "$NOTARY_ZIP"
mkdir -p "$CONTENTS/MacOS" "$CONTENTS/Resources"
cp "$BINARY" "$CONTENTS/MacOS/Skribi"
chmod 755 "$CONTENTS/MacOS/Skribi"
cp assets/Skribi.icns "$CONTENTS/Resources/Skribi.icns"
mkdir -p "$CONTENTS/Resources/Licenses"
cp THIRD_PARTY_NOTICES.md "$CONTENTS/Resources/"
cp assets/fonts/OFL-*.txt "$CONTENTS/Resources/Licenses/"
cat > "$CONTENTS/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleDisplayName</key><string>Skribi</string>
  <key>CFBundleExecutable</key><string>Skribi</string>
  <key>CFBundleIdentifier</key><string>com.paretocybernetics.skribi</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>Skribi</string>
  <key>CFBundleIconFile</key><string>Skribi</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST

codesign --force --options runtime --timestamp --sign "$SKRIBI_SIGN_IDENTITY" "$APP"
codesign --verify --deep --strict --verbose=2 "$APP"
ditto -c -k --keepParent "$APP" "$NOTARY_ZIP"
xcrun notarytool submit "$NOTARY_ZIP" --keychain-profile "$SKRIBI_NOTARY_PROFILE" --wait
xcrun stapler staple "$APP"
xcrun stapler validate "$APP"
hdiutil create -volname Skribi -srcfolder "$APP" -ov -format UDZO "$DMG"
codesign --force --timestamp --sign "$SKRIBI_SIGN_IDENTITY" "$DMG"
xcrun notarytool submit "$DMG" --keychain-profile "$SKRIBI_NOTARY_PROFILE" --wait
xcrun stapler staple "$DMG"
xcrun stapler validate "$DMG"
rm -f "$NOTARY_ZIP"
printf '%s\n' "$DMG"
