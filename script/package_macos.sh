#!/usr/bin/env bash
# Production packaging for the native macOS app (MD Star).
#
# Produces a release-mode, optionally universal `MD Star.app` containing:
#   - the native SwiftUI reader (CFBundleExecutable),
#   - the `md` CLI binary, so `install.sh --link-app` works from one download,
#   - the Rust FFI dylib,
#   - document-type associations (icon only when --icon is given),
# and optionally a signed, notarized DMG.
#
# Usage:
#   ./script/package_macos.sh [options]
#
# Options:
#   --arch <universal|arm64|x86_64>  Target architecture (default: universal)
#   --icon <path.icns>               Embed an app icon (default: none)
#   --dmg                            Also build a DMG for distribution
#   --no-cli                         Skip embedding the `md` CLI binary
#   --help                           Show this message
#
# Signing (optional). Unsigned builds run on the machine that made them, but a
# copy that is downloaded or transferred is quarantined and refuses to launch.
# macOS 15 removed the Control-click > Open bypass, so recipients must approve
# the app in System Settings > Privacy & Security, or clear the quarantine flag
# with `xattr -dr com.apple.quarantine "/Applications/MD Star.app"`. Signing and
# notarizing removes the prompt entirely:
#   CODESIGN_IDENTITY="Developer ID Application: Name (TEAMID)"
#   NOTARY_PROFILE="mdstar"     # notarytool keychain profile; enables notarization
#
# Example:
#   CODESIGN_IDENTITY="Developer ID Application: Acme (AB12CD34EF)" \
#   NOTARY_PROFILE=mdstar ./script/package_macos.sh --dmg

set -euo pipefail

APP_NAME="MD Star"
EXECUTABLE_NAME="MDStarNative"
BUNDLE_ID="dev.mdstar.native"
MIN_SYSTEM_VERSION="13.0"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NATIVE_DIR="$ROOT_DIR/macos/MDStarNative"
DIST_DIR="$ROOT_DIR/dist"
APP_BUNDLE="$DIST_DIR/$APP_NAME.app"
CONTENTS="$APP_BUNDLE/Contents"
# The app has no artwork of its own, so the bundle keeps the system's generic
# icon — matching how it looks when run from Xcode. The Tauri app's icon is
# deliberately not inherited; that is a different product's identity. Pass
# --icon <path.icns> once MD Star has its own.
ICON_SOURCE=""

ARCH_MODE="universal"
BUILD_DMG=false
EMBED_CLI=true
INSTALL_APP=false

error()   { printf '\033[31merror\033[0m: %s\n' "$1" >&2; exit 1; }
info()    { printf '\033[34m  →\033[0m %s\n' "$1"; }
ok()      { printf '\033[32m  ✓\033[0m %s\n' "$1"; }
warn()    { printf '\033[33m  !\033[0m %s\n' "$1"; }
section() { printf '\n\033[1m%s\033[0m\n' "$1"; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --arch)    ARCH_MODE="$2"; shift 2 ;;
    --dmg)     BUILD_DMG=true; shift ;;
    --no-cli)  EMBED_CLI=false; shift ;;
    --install) INSTALL_APP=true; shift ;;
    --icon)    ICON_SOURCE="$2"; shift 2 ;;
    --help)    sed -n '/^# Usage/,/^set -e/p' "$0" | grep '^#' | sed 's/^# \{0,1\}//'; exit 0 ;;
    *)         error "Unknown option: $1" ;;
  esac
done

[[ "$(uname -s)" == "Darwin" ]] || error "macOS packaging must run on macOS"

VERSION="$(awk -F'"' '/^version/ {print $2; exit}' "$ROOT_DIR/Cargo.toml")"
[[ -n "$VERSION" ]] || error "Could not read version from Cargo.toml"

case "$ARCH_MODE" in
  universal) RUST_TARGETS=(aarch64-apple-darwin x86_64-apple-darwin); SWIFT_ARCHS=(arm64 x86_64) ;;
  arm64)     RUST_TARGETS=(aarch64-apple-darwin);                     SWIFT_ARCHS=(arm64) ;;
  x86_64)    RUST_TARGETS=(x86_64-apple-darwin);                      SWIFT_ARCHS=(x86_64) ;;
  *)         error "Unknown --arch: $ARCH_MODE (expected universal, arm64, or x86_64)" ;;
esac

section "MD Star $VERSION — $ARCH_MODE"

# ── Rust: FFI dylib + md CLI ────────────────────────────────────────────────
section "Building Rust components"
for target in "${RUST_TARGETS[@]}"; do
  if ! rustup target list --installed | grep -qx "$target"; then
    info "Installing Rust target $target"
    rustup target add "$target"
  fi
  info "cargo build --release --target $target"
  (cd "$ROOT_DIR" && cargo build --release -p mdstar-ffi --target "$target")
  if $EMBED_CLI; then
    (cd "$ROOT_DIR" && cargo build --release -p mdstar-app --bin md --target "$target")
  fi
done

STAGING="$(mktemp -d)"
trap 'rm -rf "$STAGING"' EXIT

lipo_or_copy() {
  local relative="$1" output="$2" inputs=()
  for target in "${RUST_TARGETS[@]}"; do
    inputs+=("$ROOT_DIR/target/$target/release/$relative")
  done
  if [[ ${#inputs[@]} -gt 1 ]]; then
    lipo -create -output "$output" "${inputs[@]}"
  else
    cp "${inputs[0]}" "$output"
  fi
}

lipo_or_copy "libmdstar_ffi.dylib" "$STAGING/libmdstar_ffi.dylib"
ok "Rust FFI dylib"
if $EMBED_CLI; then
  lipo_or_copy "md" "$STAGING/md"
  ok "md CLI binary"
fi

# ── Swift: native app ───────────────────────────────────────────────────────
section "Building native app"
SWIFT_ARCH_FLAGS=()
for arch in "${SWIFT_ARCHS[@]}"; do SWIFT_ARCH_FLAGS+=(--arch "$arch"); done

# Package.swift searches target/ffi before target/debug, so stage the universal
# dylib there — otherwise a multi-arch build links the arm64-only debug copy.
LINK_DIR="$ROOT_DIR/target/ffi"
mkdir -p "$LINK_DIR"
cp "$STAGING/libmdstar_ffi.dylib" "$LINK_DIR/"

(cd "$NATIVE_DIR" && swift build -c release "${SWIFT_ARCH_FLAGS[@]}")
BUILT_APP_BINARY="$(cd "$NATIVE_DIR" && swift build -c release "${SWIFT_ARCH_FLAGS[@]}" --show-bin-path)/$EXECUTABLE_NAME"
[[ -f "$BUILT_APP_BINARY" ]] || error "Native binary not found: $BUILT_APP_BINARY"
ok "Native app binary"

# ── Assemble the bundle ─────────────────────────────────────────────────────
section "Assembling $APP_NAME.app"
rm -rf "$APP_BUNDLE"
mkdir -p "$CONTENTS/MacOS" "$CONTENTS/Frameworks" "$CONTENTS/Resources"

cp "$BUILT_APP_BINARY" "$CONTENTS/MacOS/$EXECUTABLE_NAME"
cp "$STAGING/libmdstar_ffi.dylib" "$CONTENTS/Frameworks/"
if $EMBED_CLI; then cp "$STAGING/md" "$CONTENTS/MacOS/md"; fi

if [[ -n "$ICON_SOURCE" && -f "$ICON_SOURCE" ]]; then
  cp "$ICON_SOURCE" "$CONTENTS/Resources/AppIcon.icns"
  ok "App icon: $ICON_SOURCE"
elif [[ -n "$ICON_SOURCE" ]]; then
  error "Icon not found: $ICON_SOURCE"
fi

install_name_tool -id "@rpath/libmdstar_ffi.dylib" "$CONTENTS/Frameworks/libmdstar_ffi.dylib"
for target in "${RUST_TARGETS[@]}"; do
  install_name_tool -change "$ROOT_DIR/target/$target/release/deps/libmdstar_ffi.dylib" \
    "@rpath/libmdstar_ffi.dylib" "$CONTENTS/MacOS/$EXECUTABLE_NAME" 2>/dev/null || true
done
chmod +x "$CONTENTS/MacOS/$EXECUTABLE_NAME"
if $EMBED_CLI; then chmod +x "$CONTENTS/MacOS/md"; fi

# Referencing an icon that was never embedded leaves Finder showing a broken
# entry rather than the generic app icon.
if [[ -f "$CONTENTS/Resources/AppIcon.icns" ]]; then
  ICON_PLIST_ENTRY='  <key>CFBundleIconFile</key><string>AppIcon</string>'
else
  ICON_PLIST_ENTRY=''
fi

cat >"$CONTENTS/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>$EXECUTABLE_NAME</string>
  <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
  <key>CFBundleName</key><string>$APP_NAME</string>
  <key>CFBundleDisplayName</key><string>$APP_NAME</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
$ICON_PLIST_ENTRY
  <key>LSMinimumSystemVersion</key><string>$MIN_SYSTEM_VERSION</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
  <key>NSHumanReadableCopyright</key><string>Copyright © 2026 MD Star Contributors. MIT OR Apache-2.0.</string>
  <key>NSPrincipalClass</key><string>NSApplication</string>
  <key>NSSupportsAutomaticTermination</key><true/>
  <key>NSSupportsSuddenTermination</key><true/>
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeName</key><string>Markdown Document</string>
      <key>CFBundleTypeRole</key><string>Viewer</string>
      <key>LSHandlerRank</key><string>Owner</string>
      <key>LSItemContentTypes</key>
      <array>
        <string>net.daringfireball.markdown</string>
        <string>public.markdown</string>
      </array>
    </dict>
    <dict>
      <key>CFBundleTypeName</key><string>Plain Text Document</string>
      <key>CFBundleTypeRole</key><string>Viewer</string>
      <key>LSHandlerRank</key><string>Alternate</string>
      <key>LSItemContentTypes</key>
      <array>
        <string>public.plain-text</string>
        <string>public.text</string>
      </array>
    </dict>
  </array>
  <key>UTImportedTypeDeclarations</key>
  <array>
    <dict>
      <key>UTTypeIdentifier</key><string>net.daringfireball.markdown</string>
      <key>UTTypeDescription</key><string>Markdown Document</string>
      <key>UTTypeConformsTo</key>
      <array><string>public.plain-text</string></array>
      <key>UTTypeTagSpecification</key>
      <dict>
        <key>public.filename-extension</key>
        <array>
          <string>md</string><string>markdown</string><string>mdown</string><string>mkd</string>
        </array>
        <key>public.mime-type</key><string>text/markdown</string>
      </dict>
    </dict>
  </array>
</dict>
</plist>
PLIST
ok "Info.plist (document types, version $VERSION)"

# ── Sign ────────────────────────────────────────────────────────────────────
section "Signing"
SIGN_IDENTITY="${CODESIGN_IDENTITY:-}"
if [[ -n "$SIGN_IDENTITY" ]]; then
  info "Developer ID: $SIGN_IDENTITY"
  codesign --force --timestamp --options runtime --sign "$SIGN_IDENTITY" \
    "$CONTENTS/Frameworks/libmdstar_ffi.dylib"
  if $EMBED_CLI; then
    codesign --force --timestamp --options runtime --sign "$SIGN_IDENTITY" "$CONTENTS/MacOS/md"
  fi
  codesign --force --timestamp --options runtime --sign "$SIGN_IDENTITY" "$APP_BUNDLE"
  codesign --verify --deep --strict --verbose=2 "$APP_BUNDLE"
  ok "Signed with Developer ID (Gatekeeper-ready)"
else
  codesign --force --sign - --timestamp=none "$CONTENTS/Frameworks/libmdstar_ffi.dylib"
  if $EMBED_CLI; then
    codesign --force --sign - --timestamp=none "$CONTENTS/MacOS/md"
  fi
  codesign --force --sign - --timestamp=none "$APP_BUNDLE"
  warn "Ad-hoc signed (no CODESIGN_IDENTITY set)."
  warn "This build runs here, but a copy that is downloaded or transferred is"
  warn "quarantined and will not launch. macOS 15 removed the Control-click"
  warn "bypass; recipients must approve it in System Settings > Privacy &"
  warn "Security, or run:"
  warn "  xattr -dr com.apple.quarantine \"/Applications/MD Star.app\""
fi

# ── DMG ─────────────────────────────────────────────────────────────────────
# make_dmg.sh owns layout, compression, signing and notarization so the
# installer can also be rebuilt on its own from an existing bundle.
if $BUILD_DMG; then
  if [[ ! -f "$ROOT_DIR/assets/dmg/background.png" ]]; then
    "$ROOT_DIR/script/make_dmg_background.sh" || warn "Could not generate DMG background"
  fi
  CODESIGN_IDENTITY="$SIGN_IDENTITY" NOTARY_PROFILE="${NOTARY_PROFILE:-}" \
    "$ROOT_DIR/script/make_dmg.sh" \
      --app "$APP_BUNDLE" \
      --volname "$APP_NAME" \
      --output "$DIST_DIR/MD-Star-$VERSION-$ARCH_MODE.dmg"
fi

# ── Install ─────────────────────────────────────────────────────────────────
# The Tauri app shipped under the same display name ("MD Star.app"). Leaving
# both installed makes LaunchServices resolve the name to whichever it indexed
# first, so installing replaces the previous bundle and re-registers the new one.
if $INSTALL_APP; then
  section "Installing to /Applications"
  INSTALLED="/Applications/$APP_NAME.app"
  if [[ -d "$INSTALLED" ]]; then
    PREVIOUS_ID="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$INSTALLED/Contents/Info.plist" 2>/dev/null || echo unknown)"
    info "Replacing existing $INSTALLED ($PREVIOUS_ID)"
    rm -rf "$INSTALLED"
  fi
  cp -R "$APP_BUNDLE" "$INSTALLED"
  ok "Installed $INSTALLED"

  # Finder and the Dock cache a bundle's icon by path, so replacing an app in
  # place keeps showing the previous artwork until the timestamp changes.
  touch "$INSTALLED"

  LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"
  if [[ -x "$LSREGISTER" ]]; then
    "$LSREGISTER" -f "$INSTALLED" >/dev/null 2>&1 || true
    ok "Re-registered with LaunchServices"
  fi
  info "Dock still showing the old icon? Run: killall Dock"
  info "Verify with: /usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' \\"
  info "  \"$INSTALLED/Contents/Info.plist\"   # expect $BUNDLE_ID"
fi

section "Done"
ok "App bundle: $APP_BUNDLE"
if $EMBED_CLI; then
  ok "Embedded CLI: $CONTENTS/MacOS/md (link with ./install.sh --link-app)"
fi
if $BUILD_DMG; then
  ok "DMG ready for distribution"
fi

# A trailing `false && ...` guard would otherwise become the script's exit code.
exit 0
