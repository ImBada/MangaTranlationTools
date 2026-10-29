#!/usr/bin/env bash
# Builds a release binary and wraps it in MangaTranslationTools.app (ad-hoc signed).
# With --dist it also writes the dmg and zip the Electron build ships (mac target: dmg, zip).
# Usage: scripts/bundle-macos.sh [--dist] [output-dir]
set -euo pipefail

DIST=0
if [ "${1:-}" = "--dist" ]; then
  DIST=1
  shift
fi
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
ICONS="$ROOT_DIR/crates/mtt-app/assets/icons"
OUT_DIR="${1:-$ROOT_DIR/target/bundle}"
APP="$OUT_DIR/MangaTranslationTools.app"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT_DIR/Cargo.toml" | head -1)"

# No default features: the packaged app leaves out the UI automation (`MTT_AUTOMATION`).
cargo build --release -p mtt-app --no-default-features --manifest-path "$ROOT_DIR/Cargo.toml"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$ROOT_DIR/target/release/manga-translation-tools" "$APP/Contents/MacOS/MangaTranslationTools"
if [ -f "$ICONS/icon.icns" ]; then
  cp "$ICONS/icon.icns" "$APP/Contents/Resources/AppIcon.icns"
fi

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>MangaTranslationTools</string>
  <key>CFBundleDisplayName</key><string>MangaTranslationTools</string>
  <key>CFBundleIdentifier</key><string>com.mangatranslationtools.native</string>
  <key>CFBundleExecutable</key><string>MangaTranslationTools</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.graphics-design</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || echo "codesign skipped"
echo "$APP"

if [ "$DIST" = "1" ]; then
  ARCH="$(uname -m)"
  DMG="$OUT_DIR/MangaTranslationTools-${VERSION}-${ARCH}.dmg"
  ZIP="$OUT_DIR/MangaTranslationTools-${VERSION}-${ARCH}-mac.zip"
  STAGE="$(mktemp -d)"
  cp -R "$APP" "$STAGE/"
  ln -s /Applications "$STAGE/Applications"
  rm -f "$DMG" "$ZIP"
  hdiutil create -volname "MangaTranslationTools ${VERSION}" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
  rm -rf "$STAGE"
  ditto -c -k --keepParent "$APP" "$ZIP"
  echo "$DMG"
  echo "$ZIP"
fi
