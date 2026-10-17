#!/usr/bin/env bash
# Paczki binarne aplikacji. Wynik trafia do dist/.
#   tools/package.sh linux     -> .deb + .AppImage            (uruchamiać na Linuksie, najlepiej na starym Ubuntu: glibc)
#   tools/package.sh windows   -> .exe + .zip                 (natywnie w Git Bash na Windows albo cross: TARGET=x86_64-pc-windows-gnu + mingw)
#   tools/package.sh macos     -> .app (zip) + .dmg           (tylko na macOS; architektura = maszyny lub TARGET=…-apple-darwin)
#   tools/package.sh native    -> to, co pasuje do bieżącego systemu
# Zmienne: TARGET (cel rustc), APPIMAGETOOL (ścieżka do appimagetool), SKIP_BUILD=1 (użyj gotowej binarki).
# Aplikacja wkompilowuje zasoby (czcionka, WASM, odtwarzacz), więc binarka jest samodzielna.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$PWD"
NAME=btb-cgeditor
TITLE="BTB CGEditor"
VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -1)"
DIST="$ROOT/dist"
TARGET="${TARGET:-}"
step() { printf '\n\033[1;34m== %s\033[0m\n' "$*"; }

TARGET_DIR="$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"

build() {
  local ext="${1:-}"
  if [ -z "${SKIP_BUILD:-}" ]; then
    step "cargo build --release ${TARGET:+--target $TARGET}"
    cargo build --release --bin "$NAME" ${TARGET:+--target "$TARGET"}
  fi
  BIN="$TARGET_DIR/${TARGET:+$TARGET/}release/$NAME$ext"
  [ -f "$BIN" ] || { echo "Brak binarki: $BIN" >&2; exit 1; }
}

arch_of() { # nazwa architektury z celu rustc lub maszyny
  case "${TARGET:-$(uname -m)}" in
    x86_64*|amd64) echo x86_64 ;;
    aarch64*|arm64) echo aarch64 ;;
    *) echo "${TARGET:-$(uname -m)}" ;;
  esac
}

package_linux() {
  build
  local arch; arch="$(arch_of)"
  local deb_arch; case "$arch" in x86_64) deb_arch=amd64 ;; aarch64) deb_arch=arm64 ;; *) deb_arch="$arch" ;; esac
  mkdir -p "$DIST"
  local work; work="$(mktemp -d)"; trap 'rm -rf "$work"' RETURN

  step ".deb"
  local pkg="$work/deb"
  install -Dm755 "$BIN" "$pkg/usr/bin/$NAME"
  install -Dm644 packaging/$NAME.desktop "$pkg/usr/share/applications/$NAME.desktop"
  install -Dm644 assets/icon.png "$pkg/usr/share/icons/hicolor/512x512/apps/$NAME.png"
  install -d "$pkg/DEBIAN"
  local size; size="$(du -sk "$pkg/usr" | cut -f1)"
  cat > "$pkg/DEBIAN/control" <<CTRL
Package: $NAME
Version: $VERSION
Section: video
Priority: optional
Architecture: $deb_arch
Installed-Size: $size
Depends: libc6, libgl1, libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libx11-6, libxcursor1, libxrandr2, libxi6, libgtk-3-0
Maintainer: BTB <kpediapl@gmail.com>
Description: $TITLE
 Edytor grafiki telewizyjnej (CG): elementy ze sceną, warstwami i animacjami,
 eksport na stronę output.html dla OBS (Browser Source).
CTRL
  dpkg-deb --root-owner-group --build "$pkg" "$DIST/${NAME}_${VERSION}_${deb_arch}.deb" >/dev/null
  echo "$DIST/${NAME}_${VERSION}_${deb_arch}.deb"

  step "AppImage"
  local appdir="$work/$TITLE.AppDir"
  install -Dm755 "$BIN" "$appdir/usr/bin/$NAME"
  install -Dm644 packaging/$NAME.desktop "$appdir/usr/share/applications/$NAME.desktop"
  install -Dm644 packaging/$NAME.desktop "$appdir/$NAME.desktop"
  install -Dm644 assets/icon.png "$appdir/$NAME.png"
  install -Dm644 assets/icon.png "$appdir/usr/share/icons/hicolor/512x512/apps/$NAME.png"
  cat > "$appdir/AppRun" <<'RUN'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/btb-cgeditor" "$@"
RUN
  chmod +x "$appdir/AppRun"
  local tool="${APPIMAGETOOL:-}"
  if [ -z "$tool" ]; then
    tool="$ROOT/dist/.cache/appimagetool-$arch.AppImage"
    if [ ! -x "$tool" ]; then
      mkdir -p "$(dirname "$tool")"
      curl -fL -o "$tool" "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$arch.AppImage"
      chmod +x "$tool"
    fi
  fi
  # APPIMAGE_EXTRACT_AND_RUN: działa bez FUSE (CI, kontenery)
  ARCH="$arch" APPIMAGE_EXTRACT_AND_RUN=1 "$tool" "$appdir" "$DIST/${TITLE// /_}-$VERSION-$arch.AppImage" >/dev/null
  echo "$DIST/${TITLE// /_}-$VERSION-$arch.AppImage"
}

package_windows() {
  build .exe
  local arch; arch="$(arch_of)"
  mkdir -p "$DIST"
  step "Windows .exe + .zip"
  cp "$BIN" "$DIST/$NAME-$VERSION-windows-$arch.exe"
  local work; work="$(mktemp -d)"; trap 'rm -rf "$work"' RETURN
  mkdir "$work/$NAME-$VERSION"
  cp "$BIN" "$work/$NAME-$VERSION/$NAME.exe"
  if command -v zip >/dev/null; then
    (cd "$work" && zip -qr "$DIST/$NAME-$VERSION-windows-$arch.zip" "$NAME-$VERSION")
  else
    (cd "$work" && powershell -NoProfile -Command "Compress-Archive -Path '$NAME-$VERSION' -DestinationPath '$DIST/$NAME-$VERSION-windows-$arch.zip' -Force")
  fi
  ls -l "$DIST"/$NAME-$VERSION-windows-$arch.*
}

package_macos() {
  [ "$(uname -s)" = Darwin ] || { echo "Paczka macOS wymaga macOS (iconutil/hdiutil)." >&2; exit 1; }
  build
  local arch; arch="$(arch_of)"
  mkdir -p "$DIST"
  step "macOS .app + .dmg"
  local work; work="$(mktemp -d)"; trap 'rm -rf "$work"' RETURN
  local app="$work/$TITLE.app"
  mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
  cp "$BIN" "$app/Contents/MacOS/$NAME"
  # ikona .icns z assets/icon.png
  local iset="$work/icon.iconset"; mkdir "$iset"
  for s in 16 32 64 128 256 512; do
    sips -z $s $s assets/icon.png --out "$iset/icon_${s}x${s}.png" >/dev/null
    sips -z $((s*2)) $((s*2)) assets/icon.png --out "$iset/icon_${s}x${s}@2x.png" >/dev/null
  done
  iconutil -c icns "$iset" -o "$app/Contents/Resources/$NAME.icns"
  cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>$TITLE</string>
  <key>CFBundleDisplayName</key><string>$TITLE</string>
  <key>CFBundleIdentifier</key><string>pl.btb.cgeditor</string>
  <key>CFBundleExecutable</key><string>$NAME</string>
  <key>CFBundleIconFile</key><string>$NAME</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
  # podpis ad-hoc (bez konta deweloperskiego); użytkownik musi raz zatwierdzić uruchomienie. Dla notaryzacji: CODESIGN_ID="Developer ID Application: …"
  codesign --force --deep -s "${CODESIGN_ID:--}" "$app"
  local base="$NAME-$VERSION-macos-$arch"
  ditto -c -k --keepParent "$app" "$DIST/$base.app.zip"
  local stage="$work/dmg"; mkdir "$stage"; cp -R "$app" "$stage/"; ln -s /Applications "$stage/Applications"
  hdiutil create -quiet -volname "$TITLE" -srcfolder "$stage" -ov -format UDZO "$DIST/$base.dmg"
  ls -l "$DIST/$base".*
}

case "${1:-native}" in
  linux) package_linux ;;
  windows) package_windows ;;
  macos) package_macos ;;
  native)
    case "$(uname -s)" in
      Linux) package_linux ;;
      Darwin) package_macos ;;
      MINGW*|MSYS*|CYGWIN*) package_windows ;;
      *) echo "Nieznany system: $(uname -s)" >&2; exit 1 ;;
    esac ;;
  *) echo "Użycie: $0 {linux|windows|macos|native}" >&2; exit 2 ;;
esac
