#!/usr/bin/env bash
# Build "MD Star.app" for the Volt reader, with document types so Finder's
# Open With and double-click open files in it.
#
# Usage:
#   ./bundle.sh             build target/bundle/MD Star.app
#   ./bundle.sh --install   also copy it to ~/Applications and register it
#
# The app is signed ad hoc, which runs on this Mac. Distribution needs a
# Developer ID signature and notarization (see script/package_macos.sh).

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
APP_NAME="MD Star"
EXECUTABLE="mdstar-volt"
BUNDLE_ID="dev.mdstar.volt"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$HERE/Cargo.toml" | head -1)"
ICON="$ROOT/crates/mdstar-app/icons/icon.icns"
OUT="$HERE/target/bundle"
APP="$OUT/$APP_NAME.app"
INSTALL=false

for arg in "$@"; do
  case "$arg" in
    --install) INSTALL=true ;;
    -h|--help) sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $arg" >&2; exit 1 ;;
  esac
done

[[ "$(uname -s)" == "Darwin" ]] || { echo "bundle.sh builds a macOS app" >&2; exit 1; }

echo "  → cargo build --release"
(cd "$HERE" && cargo build --release --quiet)

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$HERE/target/release/$EXECUTABLE" "$APP/Contents/MacOS/$EXECUTABLE"
[[ -f "$ICON" ]] && cp "$ICON" "$APP/Contents/Resources/AppIcon.icns"

# Markdown is ours by default; text, data and source files list MD Star
# under Open With without taking them over.
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$APP_NAME</string>
  <key>CFBundleDisplayName</key><string>$APP_NAME</string>
  <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
  <key>CFBundleExecutable</key><string>$EXECUTABLE</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
  <key>NSHumanReadableCopyright</key><string>MD Star contributors. MIT OR Apache-2.0.</string>
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeName</key><string>Markdown Document</string>
      <key>CFBundleTypeRole</key><string>Editor</string>
      <key>CFBundleTypeIconFile</key><string>AppIcon</string>
      <key>LSHandlerRank</key><string>Default</string>
      <key>LSItemContentTypes</key>
      <array>
        <string>net.daringfireball.markdown</string>
        <string>public.markdown</string>
      </array>
    </dict>
    <dict>
      <key>CFBundleTypeName</key><string>Text and Data</string>
      <key>CFBundleTypeRole</key><string>Editor</string>
      <key>LSHandlerRank</key><string>Alternate</string>
      <key>LSItemContentTypes</key>
      <array>
        <string>public.plain-text</string>
        <string>public.json</string>
        <string>public.xml</string>
        <string>public.yaml</string>
        <string>public.comma-separated-values-text</string>
        <string>public.tab-separated-values-text</string>
        <string>public.source-code</string>
        <string>public.log</string>
        <string>public.html</string>
        <string>public.toml</string>
        <string>com.microsoft.ini</string>
      </array>
    </dict>
    <!-- Extensions with no system type (macOS gives them dynamic types),
         claimed by name. A separate entry: with content types present,
         Launch Services would ignore the extension list. -->
    <dict>
      <key>CFBundleTypeName</key><string>Markdown Document</string>
      <key>CFBundleTypeRole</key><string>Editor</string>
      <key>CFBundleTypeIconFile</key><string>AppIcon</string>
      <key>LSHandlerRank</key><string>Default</string>
      <key>CFBundleTypeExtensions</key>
      <array><string>mdown</string><string>mkd</string><string>mkdn</string><string>mdx</string></array>
    </dict>
    <dict>
      <key>CFBundleTypeName</key><string>Configuration File</string>
      <key>CFBundleTypeRole</key><string>Editor</string>
      <key>LSHandlerRank</key><string>Alternate</string>
      <key>CFBundleTypeExtensions</key>
      <array>
        <string>env</string><string>lock</string><string>json5</string><string>jsonc</string>
        <string>conf</string><string>cfg</string><string>properties</string>
      </array>
    </dict>
    <dict>
      <key>CFBundleTypeName</key><string>Folder</string>
      <key>CFBundleTypeRole</key><string>Viewer</string>
      <key>LSHandlerRank</key><string>Alternate</string>
      <key>LSItemContentTypes</key>
      <array><string>public.folder</string></array>
    </dict>
  </array>
  <key>UTImportedTypeDeclarations</key>
  <array>
    <dict>
      <key>UTTypeIdentifier</key><string>net.daringfireball.markdown</string>
      <key>UTTypeDescription</key><string>Markdown Document</string>
      <key>UTTypeConformsTo</key><array><string>public.plain-text</string></array>
      <key>UTTypeTagSpecification</key>
      <dict>
        <key>public.filename-extension</key>
        <array><string>md</string><string>markdown</string><string>mdown</string><string>mkd</string></array>
        <key>public.mime-type</key><string>text/markdown</string>
      </dict>
    </dict>
  </array>
</dict>
</plist>
PLIST

plutil -lint "$APP/Contents/Info.plist" >/dev/null
codesign --force --sign - "$APP" >/dev/null 2>&1
echo "  ✓ $APP"

LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
if $INSTALL; then
  mkdir -p "$HOME/Applications"
  rm -rf "$HOME/Applications/$APP_NAME.app"
  cp -R "$APP" "$HOME/Applications/"
  "$LSREGISTER" -f "$HOME/Applications/$APP_NAME.app"
  echo "  ✓ installed to ~/Applications and registered with Launch Services"
else
  "$LSREGISTER" -f "$APP"
fi
