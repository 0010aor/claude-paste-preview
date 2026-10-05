#!/bin/sh
# Runs install.sh against a throwaway HOME with a stub `claude` to check what it installs and removes.
# shellcheck disable=SC2016 # check() evals its condition later, so the single quotes are deliberate.
set -eu

repo=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
failures=0

check() {
  if eval "$2"; then printf 'ok   %s\n' "$1"; else printf 'FAIL %s\n' "$1"; failures=$((failures + 1)); fi
}

make_stubs() {
  stubs="$work/stubs"
  mkdir -p "$stubs"
  cat >"$stubs/claude" <<'STUB'
#!/bin/sh
echo "$*" >>"$CLAUDE_CALLS"
STUB
  cat >"$work/helper" <<'STUB'
#!/bin/sh
[ "$1" = "--version" ] && echo "claude-paste-helper 0.0.0-test"
STUB
  chmod +x "$stubs/claude" "$work/helper"
}

run_install() {
  search_path="$1"
  shift
  HOME="$work/home" PATH="$search_path:/usr/bin:/bin" CLAUDE_CALLS="$work/calls" \
    sh "$repo/install.sh" --helper "$work/helper" --marketplace "$repo" "$@" >"$work/out" 2>&1
}

make_stubs
mkdir -p "$work/home"
bin="$work/home/.local/bin"

run_install "$stubs:$bin"
check "installs the helper" '[ -x "$bin/claude-paste-helper" ]'
if [ "$(uname -s)" != Linux ]; then
  check "provides no xclip outside Linux" '[ ! -e "$bin/xclip" ]'
elif PATH=/usr/bin:/bin command -v xclip >/dev/null 2>&1 || PATH=/usr/bin:/bin command -v wl-paste >/dev/null 2>&1; then
  check "leaves an existing xclip alone" '[ ! -e "$bin/xclip" ]'
else
  check "links xclip to the helper when none exists" '[ "$(readlink "$bin/xclip")" = "$bin/claude-paste-helper" ]'
fi
check "installs the plugin from the marketplace" 'grep -q "plugin install paste-preview@paste-preview" "$work/calls"'

run_install "$stubs:$bin" --uninstall
check "uninstall removes the helper" '[ ! -e "$bin/claude-paste-helper" ]'
check "uninstall removes its own xclip link" '[ ! -e "$bin/xclip" ]'
check "uninstall removes the plugin" 'grep -q "plugin uninstall paste-preview@paste-preview" "$work/calls"'

mkdir -p "$bin" && printf '#!/bin/sh\n' >"$bin/xclip" && chmod +x "$bin/xclip"
run_install "$stubs:$bin"
run_install "$stubs:$bin" --uninstall
check "never touches a real xclip" '[ -f "$bin/xclip" ] && [ ! -L "$bin/xclip" ]'

if HOME="$work/home" PATH="/usr/bin:/bin" sh "$repo/install.sh" >"$work/out" 2>&1; then
  check "refuses to run without Claude Code" 'false'
else
  check "refuses to run without Claude Code" 'grep -q "not on PATH" "$work/out"'
fi

[ "$failures" -eq 0 ] || { printf '%s check(s) failed\n' "$failures"; exit 1; }
