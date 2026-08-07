#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-run}"
APP_NAME="MDStarNative"
BUNDLE_ID="dev.mdstar.native"
MIN_SYSTEM_VERSION="13.0"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NATIVE_DIR="$ROOT_DIR/macos/MDStarNative"
DIST_DIR="$ROOT_DIR/dist"
APP_BUNDLE="$DIST_DIR/MD Star.app"
APP_CONTENTS="$APP_BUNDLE/Contents"
APP_MACOS="$APP_CONTENTS/MacOS"
APP_FRAMEWORKS="$APP_CONTENTS/Frameworks"
APP_BINARY="$APP_MACOS/$APP_NAME"
RUST_LIBRARY="$ROOT_DIR/target/debug/libmdstar_ffi.dylib"
APP_ICON_SOURCE="$ROOT_DIR/crates/mdstar-app/icons/icon.icns"

pkill -x "$APP_NAME" >/dev/null 2>&1 || true

(
  cd "$ROOT_DIR"
  cargo build -p mdstar-ffi
)
(
  cd "$NATIVE_DIR"
  swift build
  BUILD_BINARY="$(swift build --show-bin-path)/$APP_NAME"
  rm -rf "$APP_BUNDLE"
  mkdir -p "$APP_MACOS" "$APP_FRAMEWORKS" "$APP_CONTENTS/Resources"
  cp "$BUILD_BINARY" "$APP_BINARY"
  cp "$RUST_LIBRARY" "$APP_FRAMEWORKS/"
  [[ -f "$APP_ICON_SOURCE" ]] || { echo "missing app icon: $APP_ICON_SOURCE" >&2; exit 1; }
  cp "$APP_ICON_SOURCE" "$APP_CONTENTS/Resources/AppIcon.icns"
  install_name_tool -id "@rpath/libmdstar_ffi.dylib" "$APP_FRAMEWORKS/libmdstar_ffi.dylib"
  install_name_tool -change "$ROOT_DIR/target/debug/deps/libmdstar_ffi.dylib" "@rpath/libmdstar_ffi.dylib" "$APP_BINARY"
  chmod +x "$APP_BINARY"
)


cat >"$APP_CONTENTS/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleExecutable</key><string>$APP_NAME</string>
  <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
  <key>CFBundleName</key><string>MD Star</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>LSMinimumSystemVersion</key><string>$MIN_SYSTEM_VERSION</string>
  <key>NSPrincipalClass</key><string>NSApplication</string>
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeName</key><string>Markdown Document</string>
      <key>CFBundleTypeRole</key><string>Viewer</string>
      <key>CFBundleTypeIconFile</key><string>AppIcon</string>
      <key>LSHandlerRank</key><string>Owner</string>
      <key>LSItemContentTypes</key>
      <array>
        <string>net.daringfireball.markdown</string>
        <string>public.markdown</string>
      </array>
    </dict>
  </array>
</dict></plist>
PLIST

codesign --force --sign - --timestamp=none "$APP_FRAMEWORKS/libmdstar_ffi.dylib"
codesign --force --sign - --timestamp=none --deep "$APP_BUNDLE"

open_app() { /usr/bin/open -n "$APP_BUNDLE"; }
case "$MODE" in
  run) open_app ;;
  --debug|debug) lldb -- "$APP_BINARY" ;;
  --logs|logs) open_app; /usr/bin/log stream --info --style compact --predicate "process == '$APP_NAME'" ;;
  --telemetry|telemetry) open_app; /usr/bin/log stream --info --style compact --predicate "subsystem == '$BUNDLE_ID'" ;;
  --verify|verify) open_app; sleep 1; pgrep -x "$APP_NAME" >/dev/null ;;
  *) echo "usage: $0 [run|--debug|--logs|--telemetry|--verify]" >&2; exit 2 ;;
esac
