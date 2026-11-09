#!/usr/bin/env bash
# Installs this checkout as the Roundhouse you use every day, outside `target/`, so that rebuilding or archiving
# a worktree can't touch what's running. Each version gets its own directory, and `current` names the one to
# start. A running hub starts its workers from the version it came from, so moving `current` doesn't affect it.
# Run this by hand. It's macOS only, like the desktop app. `tools/build.sh` builds and stamps it, so it needs `jq` too.
#
#   tools/install.sh                 Build this commit in release, and make it current
#   tools/install.sh --allow-dirty   The same, with uncommitted changes in the build
#   tools/install.sh --list          Show the installed versions
#   tools/install.sh --use VERSION   Make an installed version current, such as to roll back
#
# Installs to ROUNDHOUSE_INSTALL_ROOT, or ~/Library/Application Support/roundhouse-builds.
set -euo pipefail

INSTALL_ROOT="${ROUNDHOUSE_INSTALL_ROOT:-$HOME/Library/Application Support/roundhouse-builds}"
# shellcheck source=SCRIPTDIR/build.sh
source "$(dirname "$0")/build.sh"

die() { echo "error: $1" >&2; exit 1; }

list_versions() {
  local current name
  current="$(readlink "$INSTALL_ROOT/current" 2>/dev/null || true)"
  [ -d "$INSTALL_ROOT" ] || { echo "nothing installed in $INSTALL_ROOT"; return; }
  for name in $(ls -1t "$INSTALL_ROOT"); do
    [ -L "$INSTALL_ROOT/$name" ] && continue
    if [ "$name" = "$current" ]; then echo "* $name"; else echo "  $name"; fi
  done
}

# A hub outlives its clients, so it stays on its own version until it's stopped.
report_hub() {
  local version=$1 status running
  status="$("$INSTALL_ROOT/current/roundhouse" hub status 2>&1 || true)"
  running="$(echo "$status" | awk '$1 == "pid" { print $4; exit }')"
  if [ -n "$running" ] && [ "$running" != "$version" ]; then
    echo "The running hub is still $running. The app replaces an older one when it starts with nothing running. To stop it and its agents now:"
    echo "  \"$INSTALL_ROOT/current/roundhouse\" hub stop"
  elif [ -z "$running" ] && ! echo "$status" | grep -q "no hub"; then
    echo "$status"
  fi
}

make_current() {
  local version=$1
  # A link passes the second check through what it names, and `current` pointing at itself breaks the install.
  [ ! -L "$INSTALL_ROOT/$version" ] && [ -x "$INSTALL_ROOT/$version/roundhoused" ] || die "$version isn't installed. See --list"
  ln -sfn "$version" "$INSTALL_ROOT/current"
  echo "$version is current. Start it with:"
  echo "  \"$INSTALL_ROOT/current/roundhouse-desktop\""
  report_hub "$version"
}

install_this_commit() {
  local allow_dirty=$1
  stamp "$allow_dirty" install
  if [ ! -d "$INSTALL_ROOT/$BUILD" ]; then
    mkdir -p "$INSTALL_ROOT"
    # Not local, because the trap outlives this function if anything fails.
    tmp="$(mktemp -d "$INSTALL_ROOT/.installing.XXXXXX")"
    trap 'rm -rf "$tmp"' EXIT
    build_release "$tmp"
    chmod 755 "$tmp"
    mv "$tmp" "$INSTALL_ROOT/$BUILD"
    trap - EXIT
  fi
  make_current "$BUILD"
}

[ "$(uname)" = Darwin ] || die "installing is macOS only for now"
case "${1:-}" in
  "") install_this_commit no ;;
  --allow-dirty) install_this_commit yes ;;
  --list) list_versions ;;
  --use) make_current "${2:?--use needs a version. See --list}" ;;
  *) die "unknown option $1. See the top of this script" ;;
esac
