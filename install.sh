#!/bin/sh
# Installs the paste-preview Claude Code plugin and its helper on Linux and macOS. --uninstall removes both.
set -eu

REPO="${PASTE_PREVIEW_REPO:-0010aor/claude-paste-preview}"
BIN_DIR="${PASTE_PREVIEW_BIN_DIR:-$HOME/.local/bin}"
HELPER="$BIN_DIR/claude-paste-helper"
PLUGIN="paste-preview@paste-preview"

helper_source=""
marketplace="$REPO"
action="install"

say() { printf '%s\n' "$*"; }
fail() { printf 'install.sh: %s\n' "$*" >&2; exit 1; }

while [ $# -gt 0 ]; do
  case "$1" in
    --uninstall) action="uninstall" ;;
    --helper) [ $# -ge 2 ] || fail "--helper needs a path"; helper_source="$2"; shift ;;
    --marketplace) [ $# -ge 2 ] || fail "--marketplace needs a repo or folder"; marketplace="$2"; shift ;;
    *) fail "unknown option $1" ;;
  esac
  shift
done

command -v claude >/dev/null 2>&1 || fail "Claude Code (claude) is not on PATH"

release_asset() {
  case "$(uname -s)-$(uname -m)" in
    Linux-x86_64 | Linux-amd64) echo "claude-paste-helper-linux-x86_64" ;;
    Linux-aarch64 | Linux-arm64) echo "claude-paste-helper-linux-aarch64" ;;
    Darwin-arm64) echo "claude-paste-helper-macos-arm64" ;;
    Darwin-x86_64) echo "claude-paste-helper-macos-x86_64" ;;
    *) fail "no prebuilt helper for $(uname -s) $(uname -m)" ;;
  esac
}

download() {
  if command -v curl >/dev/null 2>&1; then curl -fsSL "$1" -o "$2"
  elif command -v wget >/dev/null 2>&1; then wget -q "$1" -O "$2"
  else fail "need curl or wget to download the helper"
  fi
}

verify_checksum() {
  file="$1" expected="$2"
  if command -v sha256sum >/dev/null 2>&1; then actual=$(sha256sum "$file" | cut -d' ' -f1)
  elif command -v shasum >/dev/null 2>&1; then actual=$(shasum -a 256 "$file" | cut -d' ' -f1)
  else say "Warning: no sha256sum or shasum; skipping the checksum check."; return
  fi
  [ "$actual" = "$expected" ] || fail "checksum mismatch for the downloaded helper"
}

install_helper() {
  mkdir -p "$BIN_DIR"
  if [ -n "$helper_source" ]; then
    cp "$helper_source" "$HELPER.tmp"
  else
    asset=$(release_asset)
    url="https://github.com/$REPO/releases/latest/download/$asset"
    download "$url" "$HELPER.tmp"
    download "$url.sha256" "$HELPER.sha256"
    verify_checksum "$HELPER.tmp" "$(cut -d' ' -f1 "$HELPER.sha256")"
    rm -f "$HELPER.sha256"
  fi
  chmod +x "$HELPER.tmp"
  mv "$HELPER.tmp" "$HELPER"
  say "Installed $("$HELPER" --version) to $HELPER"
}

provide_xclip_if_missing() {
  if command -v xclip >/dev/null 2>&1 || command -v wl-paste >/dev/null 2>&1; then
    say "Found xclip or wl-paste already; Ctrl+V keeps using it."
    return
  fi
  ln -sf "$HELPER" "$BIN_DIR/xclip"
  say "No xclip or wl-paste found: $BIN_DIR/xclip now points at the helper, so Ctrl+V pastes images."
}

install_plugin() {
  claude plugin marketplace add "$marketplace" >/dev/null
  claude plugin install "$PLUGIN" >/dev/null
  say "Installed the $PLUGIN Claude Code plugin."
}

uninstall() {
  claude plugin uninstall "$PLUGIN" >/dev/null 2>&1 || true
  claude plugin marketplace remove paste-preview >/dev/null 2>&1 || true
  if [ "$(readlink "$BIN_DIR/xclip" 2>/dev/null)" = "$HELPER" ]; then
    rm -f "$BIN_DIR/xclip"
  fi
  rm -f "$HELPER"
  say "Removed the plugin and the helper."
}

if [ "$action" = "uninstall" ]; then
  uninstall
  exit 0
fi

case "$(uname -s)" in
  Linux)
    install_helper
    provide_xclip_if_missing
    ;;
  Darwin) install_helper ;;
  *) fail "on Windows, run install.ps1 in PowerShell instead" ;;
esac
install_plugin
say "Done. Restart Claude Code and paste an image."
