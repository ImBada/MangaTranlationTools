#!/usr/bin/env bash
# Linux release (Electron build: AppImage, deb). Always writes a tar.gz with an AppDir
# layout; builds the AppImage when `appimagetool` is on PATH and the deb when `dpkg-deb` is.
# Usage: scripts/package-linux.sh [output-dir]
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
ICONS="$ROOT_DIR/crates/mtt-app/assets/icons"
OUT_DIR="${1:-$ROOT_DIR/target/bundle}"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT_DIR/Cargo.toml" | head -1)"
ARCH="$(uname -m)"
NAME="MangaTranslationTools"

cargo build --release -p mtt-app --manifest-path "$ROOT_DIR/Cargo.toml"

APPDIR="$OUT_DIR/$NAME.AppDir"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications" "$APPDIR/usr/share/icons/hicolor/512x512/apps"
cp "$ROOT_DIR/target/release/manga-translation-tools" "$APPDIR/usr/bin/manga-translation-tools"
if [ -f "$ICONS/icon.png" ]; then
  cp "$ICONS/icon.png" "$APPDIR/usr/share/icons/hicolor/512x512/apps/manga-translation-tools.png"
  cp "$ICONS/icon.png" "$APPDIR/manga-translation-tools.png"
fi
cat > "$APPDIR/usr/share/applications/manga-translation-tools.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=$NAME
Exec=manga-translation-tools
Icon=manga-translation-tools
Categories=Graphics;
DESKTOP
cp "$APPDIR/usr/share/applications/manga-translation-tools.desktop" "$APPDIR/"
cat > "$APPDIR/AppRun" <<'APPRUN'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/manga-translation-tools" "$@"
APPRUN
chmod +x "$APPDIR/AppRun"

tar -C "$OUT_DIR" -czf "$OUT_DIR/$NAME-$VERSION-linux-$ARCH.tar.gz" "$NAME.AppDir"
echo "$OUT_DIR/$NAME-$VERSION-linux-$ARCH.tar.gz"

if command -v appimagetool >/dev/null 2>&1; then
  ARCH="$ARCH" appimagetool "$APPDIR" "$OUT_DIR/$NAME-$VERSION-$ARCH.AppImage"
  echo "$OUT_DIR/$NAME-$VERSION-$ARCH.AppImage"
fi

if command -v dpkg-deb >/dev/null 2>&1; then
  DEB_ARCH="$(dpkg --print-architecture 2>/dev/null || echo amd64)"
  DEBROOT="$(mktemp -d)"
  mkdir -p "$DEBROOT/DEBIAN" "$DEBROOT/usr"
  cp -R "$APPDIR/usr/." "$DEBROOT/usr/"
  cat > "$DEBROOT/DEBIAN/control" <<CONTROL
Package: manga-translation-tools
Version: $VERSION
Architecture: $DEB_ARCH
Maintainer: MangaTranslationTools
Section: graphics
Priority: optional
Description: Manga translation editor (native)
CONTROL
  dpkg-deb --build "$DEBROOT" "$OUT_DIR/manga-translation-tools_${VERSION}_${DEB_ARCH}.deb" >/dev/null
  rm -rf "$DEBROOT"
  echo "$OUT_DIR/manga-translation-tools_${VERSION}_${DEB_ARCH}.deb"
fi
