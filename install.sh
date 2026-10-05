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

if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
  bold=$(printf '\033[1m') dim=$(printf '\033[2m') green=$(printf '\033[32m') red=$(printf '\033[31m') reset=$(printf '\033[0m')
else
  bold="" dim="" green="" red="" reset=""
fi
case "${LC_ALL:-${LC_CTYPE:-${LANG:-}}}" in
  *UTF-8* | *utf8* | *UTF8* | *utf-8*) mark_ok="✓" mark_info="•" mark_fail="✗" ;;
  *) mark_ok="ok" mark_info="--" mark_fail="!!" ;;
esac

title() { printf '\n%s%s%s\n\n' "$bold" "$1" "$reset"; }
step() { printf '  %s%s%s %s%s\n' "$1" "$2" "$reset" "$3" "${4:+ $dim$4$reset}"; }
done_step() { step "$green" "$mark_ok" "$1" "${2:-}"; }
note_step() { step "$dim" "$mark_info" "$1" "${2:-}"; }
finish() { printf '\n%s\n\n' "$1"; }
fail() { printf '\n  %s%s %s%s\n\n' "$red" "$mark_fail" "$1" "$reset" >&2; exit 1; }
tilde() { case "$1" in "$HOME"/*) printf '~%s' "${1#"$HOME"}" ;; *) printf '%s' "$1" ;; esac; }

while [ $# -gt 0 ]; do
  case "$1" in
    --uninstall) action="uninstall" ;;
    --helper) [ $# -ge 2 ] || fail "--helper needs a path"; helper_source="$2"; shift ;;
    --marketplace) [ $# -ge 2 ] || fail "--marketplace needs a repo or folder"; marketplace="$2"; shift ;;
    *) fail "unknown option $1" ;;
  esac
  shift
done

command -v claude >/dev/null 2>&1 || fail "Claude Code (claude) is not on PATH. Install it first: https://claude.com/claude-code"

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
  if command -v curl >/dev/null 2>&1; then curl -fsSL "$1" -o "$2" || fail "could not download $1"
  elif command -v wget >/dev/null 2>&1; then wget -q "$1" -O "$2" || fail "could not download $1"
  else fail "need curl or wget to download the helper"
  fi
}

verify_checksum() {
  file="$1" expected="$2"
  if command -v sha256sum >/dev/null 2>&1; then actual=$(sha256sum "$file" | cut -d' ' -f1)
  elif command -v shasum >/dev/null 2>&1; then actual=$(shasum -a 256 "$file" | cut -d' ' -f1)
  else return 1
  fi
  [ "$actual" = "$expected" ] || fail "the downloaded helper failed its checksum; nothing was installed"
}

install_helper() {
  mkdir -p "$BIN_DIR"
  detail="from your local build"
  if [ -n "$helper_source" ]; then
    cp "$helper_source" "$HELPER.tmp"
  else
    url="https://github.com/$REPO/releases/latest/download/$(release_asset)"
    download "$url" "$HELPER.tmp"
    download "$url.sha256" "$HELPER.sha256"
    if verify_checksum "$HELPER.tmp" "$(cut -d' ' -f1 "$HELPER.sha256")"; then detail="checksum verified"
    else detail="no sha256 tool found, checksum not checked"
    fi
    rm -f "$HELPER.sha256"
  fi
  chmod +x "$HELPER.tmp"
  mv "$HELPER.tmp" "$HELPER"
  done_step "Installed $("$HELPER" --version)" "($detail, $(tilde "$HELPER"))"
}

provide_xclip_if_missing() {
  if command -v xclip >/dev/null 2>&1 || command -v wl-paste >/dev/null 2>&1; then
    note_step "Ctrl+V keeps using your existing xclip or wl-paste"
    return
  fi
  ln -sf "$HELPER" "$BIN_DIR/xclip"
  done_step "Enabled Ctrl+V image paste" "($(tilde "$BIN_DIR/xclip") points at the helper)"
}

install_plugin() {
  claude plugin marketplace add "$marketplace" >/dev/null 2>&1 || true
  claude plugin install "$PLUGIN" >/dev/null || fail "Claude Code could not install the plugin"
  done_step "Installed the paste-preview plugin in Claude Code"
}

uninstall() {
  title "Removing paste-preview"
  claude plugin uninstall "$PLUGIN" >/dev/null 2>&1 || true
  claude plugin marketplace remove paste-preview >/dev/null 2>&1 || true
  done_step "Removed the plugin from Claude Code"
  if [ "$(readlink "$BIN_DIR/xclip" 2>/dev/null)" = "$HELPER" ]; then
    rm -f "$BIN_DIR/xclip"
    done_step "Removed the xclip link" "($(tilde "$BIN_DIR/xclip"))"
  fi
  rm -f "$HELPER"
  done_step "Removed the helper" "($(tilde "$HELPER"))"
  finish "paste-preview is uninstalled."
}

if [ "$action" = "uninstall" ]; then
  uninstall
  exit 0
fi

title "Installing paste-preview for Claude Code"
case "$(uname -s)" in
  Linux)
    install_helper
    provide_xclip_if_missing
    ;;
  Darwin) install_helper ;;
  *) fail "on Windows, run install.ps1 in PowerShell instead" ;;
esac
install_plugin
finish "${bold}Done.${reset} Restart Claude Code, then paste an image with Ctrl+V."
