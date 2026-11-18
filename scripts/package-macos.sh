#!/bin/bash
# Builds the macOS release: one universal binary (Apple Silicon + Intel) in
# ROrganizer.app, signed, inside a .dmg. Run by the release workflow; also
# runs on a Mac by hand.
#
#   MACOS_SIGN_IDENTITY  SHA-1 of the signing certificate (required)
#   MACOS_KEYCHAIN       keychain holding it (optional)
#   OUT                  output directory (default target/macos)
#
# Prints the path of the .dmg on its last line.
set -euo pipefail
cd "$(dirname "$0")/.."

: "${MACOS_SIGN_IDENTITY:?SHA-1 of the signing certificate}"
OUT=${OUT:-target/macos}
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
BUNDLE_ID=fr.loulouw-labs.rorganizer
# Oldest macOS the binary is linked for. Apple Silicon starts at 11 anyway.
MIN_MACOS=11.0

export MACOSX_DEPLOYMENT_TARGET=$MIN_MACOS
for target in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --release --locked --target "$target"
done

rm -rf "$OUT"
APP="$OUT/ROrganizer.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create -output "$APP/Contents/MacOS/rorganizer" \
    target/aarch64-apple-darwin/release/rorganizer \
    target/x86_64-apple-darwin/release/rorganizer

# The source icon is 256 px: the set stops at 128@2x, which covers the
# Finder and the Dock.
ICONSET="$OUT/ROrganizer.iconset"
mkdir -p "$ICONSET"
for size in 16 32 128 256; do
    sips -z "$size" "$size" resources/icons/icon.png --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
done
for size in 16 32 128; do
    double=$((size * 2))
    sips -z "$double" "$double" resources/icons/icon.png --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/ROrganizer.icns"
rm -rf "$ICONSET"

# No LSUIElement: the app starts with a Dock icon and drops it itself when
# hidden in the menu bar (platform/macos/main_window.rs).
cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
    <key>CFBundleName</key><string>ROrganizer</string>
    <key>CFBundleDisplayName</key><string>ROrganizer</string>
    <key>CFBundleExecutable</key><string>rorganizer</string>
    <key>CFBundleIconFile</key><string>ROrganizer</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>LSMinimumSystemVersion</key><string>$MIN_MACOS</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
EOF

# macOS ties the Accessibility grant to the signing certificate: every
# release must be signed with the same one (AGENTS.md).
sign() {
    codesign --force --timestamp=none ${MACOS_KEYCHAIN:+--keychain "$MACOS_KEYCHAIN"} \
        --sign "$MACOS_SIGN_IDENTITY" "$1"
}
sign "$APP"
codesign --verify --strict --verbose=2 "$APP"

# Drag-and-drop layout: the app next to a link to /Applications.
STAGE="$OUT/dmg"
mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
DMG="$OUT/ROrganizer-$VERSION-universal-macos.dmg"
hdiutil create -quiet -volname ROrganizer -srcfolder "$STAGE" -fs HFS+ -format UDZO -ov "$DMG"
rm -rf "$STAGE"
sign "$DMG"

echo "$DMG"
