#!/usr/bin/env sh
# MD Star CLI installer
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/danieldear/mdstar/main/install.sh | sh
#   ./install.sh --link-app          # link the CLI out of an installed MD Star.app
#
# Override the source repository with REPO=owner/name when testing a fork.

set -e

REPO="${REPO:-danieldear/mdstar}"
BIN_NAME="md"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
APP_PATH="${APP_PATH:-}"

error() { printf '\033[31merror\033[0m: %s\n' "$1" >&2; exit 1; }
info()  { printf '\033[34minfo\033[0m:  %s\n' "$1"; }
ok()    { printf '\033[32m  ok\033[0m:  %s\n' "$1"; }

print_path_hint() {
  case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
      printf '\n\033[33mAdd %s to your PATH:\033[0m\n' "$INSTALL_DIR"
      printf '  echo '\''export PATH="%s:$PATH"'\'' >> ~/.zshrc\n' "$INSTALL_DIR"
      printf '  source ~/.zshrc\n\n'
      ;;
  esac
}

link_macos_app() {
  [ "$(uname -s)" = "Darwin" ] || error "--link-app is only supported on macOS"

  if [ -z "$APP_PATH" ]; then
    if [ -d "/Applications/MD Star.app" ]; then
      APP_PATH="/Applications/MD Star.app"
    elif [ -d "$HOME/Applications/MD Star.app" ]; then
      APP_PATH="$HOME/Applications/MD Star.app"
    elif [ -d "/Applications/Markwell.app" ]; then
      APP_PATH="/Applications/Markwell.app"
    elif [ -d "$HOME/Applications/Markwell.app" ]; then
      APP_PATH="$HOME/Applications/Markwell.app"
    else
      error "MD Star.app not found. Set APP_PATH=\"/path/to/MD Star.app\""
    fi
  fi

  APP_BIN="$APP_PATH/Contents/MacOS/$BIN_NAME"
  [ -x "$APP_BIN" ] || error "App binary not found or not executable: $APP_BIN"

  mkdir -p "$INSTALL_DIR"
  ln -sf "$APP_BIN" "$INSTALL_DIR/$BIN_NAME"
  ok "Linked $BIN_NAME → $APP_BIN"
  print_path_hint
  info "Run \`md --help\` to get started."
}

case "${1:-}" in
  --link-app)
    link_macos_app
    exit 0
    ;;
  "" ) ;;
  * )
    error "Unknown option: $1"
    ;;
esac

[ -n "$REPO" ] || error "Set REPO=owner/repository before running this installer, or use --link-app on macOS"

# ── Detect platform ──────────────────────────────────────────────────────────
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin)
    case "$ARCH" in
      arm64)  TARGET="aarch64-apple-darwin" ;;
      x86_64) TARGET="x86_64-apple-darwin"  ;;
      *)      error "Unsupported macOS arch: $ARCH" ;;
    esac
    ;;
  Linux)
    case "$ARCH" in
      x86_64)  TARGET="x86_64-unknown-linux-gnu"  ;;
      aarch64) TARGET="aarch64-unknown-linux-gnu" ;;
      *)       error "Unsupported Linux arch: $ARCH" ;;
    esac
    ;;
  *)
    error "Unsupported OS: $OS. Install manually from https://github.com/$REPO/releases"
    ;;
esac

# ── Resolve latest version ────────────────────────────────────────────────────
info "Fetching latest release from GitHub…"
LATEST=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
  | grep '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')

[ -z "$LATEST" ] && error "Could not determine latest version"
info "Latest version: $LATEST"

# ── Download ─────────────────────────────────────────────────────────────────
ARCHIVE="md-${LATEST}-${TARGET}.tar.gz"
URL="https://github.com/$REPO/releases/download/${LATEST}/${ARCHIVE}"

TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

info "Downloading $ARCHIVE…"
curl -fsSL --progress-bar "$URL" -o "$TMPDIR/$ARCHIVE"

# Verify checksum if available
SHA_URL="${URL}.sha256"
if curl -fsSL "$SHA_URL" -o "$TMPDIR/$ARCHIVE.sha256" 2>/dev/null; then
  info "Verifying checksum…"
  EXPECTED=$(awk '{print $1}' "$TMPDIR/$ARCHIVE.sha256")
  ACTUAL=$(shasum -a 256 "$TMPDIR/$ARCHIVE" | awk '{print $1}')
  [ "$EXPECTED" = "$ACTUAL" ] || error "Checksum mismatch"
  ok "Checksum verified"
fi

# ── Install ───────────────────────────────────────────────────────────────────
tar -xzf "$TMPDIR/$ARCHIVE" -C "$TMPDIR"
mkdir -p "$INSTALL_DIR"
install -m 755 "$TMPDIR/$BIN_NAME" "$INSTALL_DIR/$BIN_NAME"

ok "Installed $BIN_NAME $LATEST → $INSTALL_DIR/$BIN_NAME"

# ── PATH hint ─────────────────────────────────────────────────────────────────
print_path_hint

info "Run \`md --help\` to get started."
