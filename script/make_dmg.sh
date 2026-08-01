#!/usr/bin/env bash
# Build a styled macOS DMG installer from an existing .app bundle.
#
# Produces a compressed, read-only DMG with a drag-to-Applications layout,
# custom window geometry, icon placement, an optional background image, and an
# optional volume icon. Window styling is best-effort: it needs a scriptable
# Finder, so on headless CI runners the script still emits a valid (unstyled)
# DMG rather than failing the build.
#
# Usage:
#   ./script/make_dmg.sh --app "dist/MD Star.app" [options]
#
# Options:
#   --app <path>         Source .app bundle (required)
#   --output <path>      Output .dmg path (default: dist/<name>-<version>.dmg)
#   --volname <name>     Mounted volume name (default: app bundle name)
#   --background <png>   Background image (default: assets/dmg/background.png if present)
#   --no-style           Skip Finder window styling
#   --help               Show this message
#
# Signing:
#   CODESIGN_IDENTITY="Developer ID Application: Name (TEAMID)"  signs the DMG
#   NOTARY_PROFILE="mdstar"                                      notarizes + staples

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

APP_PATH=""
OUTPUT=""
VOLNAME=""
BACKGROUND=""
STYLE=true

WINDOW_WIDTH=620
WINDOW_HEIGHT=420
ICON_SIZE=112
APP_ICON_X=160
APP_ICON_Y=200
DROP_ICON_X=460
DROP_ICON_Y=200

# Finder scripting blocks on a TCC "Automation" prompt when the calling terminal
# lacks permission, and that prompt never appears in CI or non-interactive
# shells. Cap the wait so styling degrades instead of hanging the build.
STYLE_TIMEOUT_SECONDS=25

# Runs inside a subshell so the shell's "Terminated" job notice stays contained.
run_with_timeout() {
  local seconds="$1"; shift
  (
    set +m
    "$@" >/dev/null 2>&1 &
    local command_pid=$!
    local waited=0
    while kill -0 "$command_pid" 2>/dev/null; do
      if (( waited >= seconds )); then
        kill -KILL "$command_pid" 2>/dev/null
        wait "$command_pid" 2>/dev/null
        exit 124
      fi
      sleep 1
      waited=$((waited + 1))
    done
    wait "$command_pid" 2>/dev/null
    exit $?
  ) 2>/dev/null
}

error()   { printf '\033[31merror\033[0m: %s\n' "$1" >&2; exit 1; }
info()    { printf '\033[34m  →\033[0m %s\n' "$1"; }
ok()      { printf '\033[32m  ✓\033[0m %s\n' "$1"; }
warn()    { printf '\033[33m  !\033[0m %s\n' "$1"; }
section() { printf '\n\033[1m%s\033[0m\n' "$1"; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --app)        APP_PATH="$2"; shift 2 ;;
    --output)     OUTPUT="$2"; shift 2 ;;
    --volname)    VOLNAME="$2"; shift 2 ;;
    --background) BACKGROUND="$2"; shift 2 ;;
    --no-style)   STYLE=false; shift ;;
    --help)       sed -n '/^# Usage/,/^set -e/p' "$0" | grep '^#' | sed 's/^# \{0,1\}//'; exit 0 ;;
    *)            error "Unknown option: $1" ;;
  esac
done

[[ "$(uname -s)" == "Darwin" ]] || error "DMG creation requires macOS"
[[ -n "$APP_PATH" ]] || error "--app is required (e.g. --app \"dist/MD Star.app\")"
[[ -d "$APP_PATH" ]] || error "App bundle not found: $APP_PATH"

APP_NAME="$(basename "$APP_PATH" .app)"
[[ -n "$VOLNAME" ]] || VOLNAME="$APP_NAME"

VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' \
  "$APP_PATH/Contents/Info.plist" 2>/dev/null || echo "0.0.0")"

if [[ -z "$OUTPUT" ]]; then
  OUTPUT="$ROOT_DIR/dist/${APP_NAME// /-}-$VERSION.dmg"
fi
mkdir -p "$(dirname "$OUTPUT")"

if [[ -z "$BACKGROUND" && -f "$ROOT_DIR/assets/dmg/background.png" ]]; then
  BACKGROUND="$ROOT_DIR/assets/dmg/background.png"
fi

section "DMG installer — $APP_NAME $VERSION"

STAGING="$(mktemp -d)"
TEMP_DMG="$(mktemp -u).dmg"
MOUNT_POINT=""

cleanup() {
  if [[ -n "$MOUNT_POINT" && -d "$MOUNT_POINT" ]]; then
    hdiutil detach "$MOUNT_POINT" -force >/dev/null 2>&1 || true
  fi
  rm -rf "$STAGING" "$TEMP_DMG"
}
trap cleanup EXIT

# ── Stage contents ──────────────────────────────────────────────────────────
info "Staging bundle"
cp -R "$APP_PATH" "$STAGING/"
ln -s /Applications "$STAGING/Applications"

if [[ -n "$BACKGROUND" && -f "$BACKGROUND" ]]; then
  mkdir -p "$STAGING/.background"
  cp "$BACKGROUND" "$STAGING/.background/background.png"
  ok "Background image"
fi

# A volume icon makes the mounted disk match the app in Finder.
VOLUME_ICON="$APP_PATH/Contents/Resources/AppIcon.icns"
if [[ -f "$VOLUME_ICON" ]]; then
  cp "$VOLUME_ICON" "$STAGING/.VolumeIcon.icns"
fi

# ── Create a writable image to lay out ──────────────────────────────────────
info "Creating writable image"
hdiutil create \
  -srcfolder "$STAGING" \
  -volname "$VOLNAME" \
  -fs HFS+ \
  -format UDRW \
  -ov \
  "$TEMP_DMG" >/dev/null
ok "Writable image"

info "Mounting"
MOUNT_OUTPUT="$(hdiutil attach "$TEMP_DMG" -readwrite -noverify -noautoopen)"
MOUNT_POINT="$(echo "$MOUNT_OUTPUT" | grep -o '/Volumes/.*$' | tail -1)"
[[ -n "$MOUNT_POINT" && -d "$MOUNT_POINT" ]] || error "Failed to mount working image"
ok "Mounted at $MOUNT_POINT"

# ── Style the Finder window (best effort) ───────────────────────────────────
if $STYLE; then
  info "Applying Finder layout"
  BACKGROUND_CLAUSE=""
  if [[ -f "$MOUNT_POINT/.background/background.png" ]]; then
    BACKGROUND_CLAUSE='set background picture of viewOptions to file ".background:background.png"'
  fi

  SCRIPT_FILE="$STAGING/../layout.applescript"
  cat >"$SCRIPT_FILE" <<APPLESCRIPT
tell application "Finder"
  tell disk "$VOLNAME"
    open
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    set the bounds of container window to {200, 140, $((200 + WINDOW_WIDTH)), $((140 + WINDOW_HEIGHT))}
    set viewOptions to the icon view options of container window
    set arrangement of viewOptions to not arranged
    set icon size of viewOptions to $ICON_SIZE
    $BACKGROUND_CLAUSE
    set position of item "$APP_NAME.app" of container window to {$APP_ICON_X, $APP_ICON_Y}
    set position of item "Applications" of container window to {$DROP_ICON_X, $DROP_ICON_Y}
    close
    open
    update without registering applications
    delay 1
    close
  end tell
end tell
APPLESCRIPT

  set +e
  run_with_timeout "$STYLE_TIMEOUT_SECONDS" osascript "$SCRIPT_FILE"
  STYLE_STATUS=$?
  set -e

  case $STYLE_STATUS in
    0)   ok "Window layout, icon size, and positions" ;;
    124) warn "Finder did not respond within ${STYLE_TIMEOUT_SECONDS}s — shipping unstyled layout."
         warn "Grant your terminal Automation access to Finder in System Settings >"
         warn "Privacy & Security > Automation, then re-run to get the styled window." ;;
    *)   warn "Finder styling failed (exit $STYLE_STATUS) — shipping unstyled layout" ;;
  esac
fi

# Mark the volume as having a custom icon.
if [[ -f "$MOUNT_POINT/.VolumeIcon.icns" ]]; then
  if command -v SetFile >/dev/null 2>&1; then
    SetFile -a C "$MOUNT_POINT" 2>/dev/null && ok "Volume icon" || warn "Could not set volume icon attribute"
  else
    warn "SetFile unavailable — volume icon not applied"
  fi
fi

chmod -Rf go-w "$MOUNT_POINT" 2>/dev/null || true
sync

info "Unmounting"
hdiutil detach "$MOUNT_POINT" >/dev/null
MOUNT_POINT=""

# ── Compress to the final read-only image ───────────────────────────────────
info "Compressing"
rm -f "$OUTPUT"
hdiutil convert "$TEMP_DMG" -format UDZO -imagekey zlib-level=9 -o "$OUTPUT" >/dev/null
ok "Compressed image"

# ── Sign and notarize ───────────────────────────────────────────────────────
if [[ -n "${CODESIGN_IDENTITY:-}" ]]; then
  codesign --force --sign "$CODESIGN_IDENTITY" "$OUTPUT"
  ok "Signed DMG"

  if [[ -n "${NOTARY_PROFILE:-}" ]]; then
    section "Notarizing"
    info "Submitting to Apple (this can take a few minutes)…"
    xcrun notarytool submit "$OUTPUT" --keychain-profile "$NOTARY_PROFILE" --wait
    xcrun stapler staple "$OUTPUT"
    ok "Notarized and stapled"
  else
    warn "NOTARY_PROFILE not set — signed but not notarized"
  fi
else
  warn "CODESIGN_IDENTITY not set — unsigned DMG."
  warn "Recipients must right-click the app and choose Open on first launch."
fi

# ── Verify ──────────────────────────────────────────────────────────────────
hdiutil verify "$OUTPUT" >/dev/null 2>&1 && ok "Image verified" || warn "Image verification reported problems"
shasum -a 256 "$OUTPUT" > "$OUTPUT.sha256"

SIZE="$(du -h "$OUTPUT" | cut -f1 | tr -d ' ')"
section "Done"
ok "Installer: $OUTPUT ($SIZE)"
ok "Checksum:  $OUTPUT.sha256"
