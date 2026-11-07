#!/usr/bin/env bash
# Installs this checkout as the Trestle you use every day, outside `target/`, so that rebuilding or archiving
# a worktree can't touch what's running. Each version gets its own directory, and `current` names the one to
# start. A running hub starts its workers from the version it came from, so moving `current` doesn't affect it.
# Run this by hand. It's macOS only, like the desktop app.
#
#   tools/install.sh                 Build this commit in release, and make it current
#   tools/install.sh --allow-dirty   The same, with uncommitted changes in the build
#   tools/install.sh --list          Show the installed versions
#   tools/install.sh --use VERSION   Make an installed version current, such as to roll back
#
# Installs to TRESTLE_INSTALL_ROOT, or ~/Library/Application Support/trestle-builds.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
INSTALL_ROOT="${TRESTLE_INSTALL_ROOT:-$HOME/Library/Application Support/trestle-builds}"
BINARIES=(trestle-desktop trestled trestle)

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
  status="$("$INSTALL_ROOT/current/trestle" hub status 2>&1 || true)"
  running="$(echo "$status" | awk '$1 == "pid" { print $4; exit }')"
  if [ -n "$running" ] && [ "$running" != "$version" ]; then
    echo "The running hub is still $running. Stopping it stops its agents, and a new one starts when the app needs it:"
    echo "  \"$INSTALL_ROOT/current/trestle\" hub stop"
  elif [ -z "$running" ] && ! echo "$status" | grep -q "no hub"; then
    echo "$status"
  fi
}

make_current() {
  local version=$1
  # A link passes the second check through what it names, and `current` pointing at itself breaks the install.
  [ ! -L "$INSTALL_ROOT/$version" ] && [ -x "$INSTALL_ROOT/$version/trestled" ] || die "$version isn't installed. See --list"
  ln -sfn "$version" "$INSTALL_ROOT/current"
  echo "$version is current. Start it with:"
  echo "  \"$INSTALL_ROOT/current/trestle-desktop\""
  report_hub "$version"
}

install_this_commit() {
  local allow_dirty=$1 version bin
  cd "$ROOT"
  version="$(git rev-parse --short=12 HEAD)"
  if [ -n "$(git status --porcelain)" ]; then
    [ "$allow_dirty" = yes ] || die "this checkout has uncommitted changes. Commit them, or use --allow-dirty"
    # Timestamped, because the same commit can be installed with different changes.
    version="$version-dirty-$(date +%Y%m%d%H%M%S)"
  fi
  if [ ! -d "$INSTALL_ROOT/$version" ]; then
    TRESTLE_BUILD="$version" cargo build --release --locked -p desktop -p daemon -p cli --bins
    mkdir -p "$INSTALL_ROOT"
    # Not local, because the trap outlives this function if anything fails.
    tmp="$(mktemp -d "$INSTALL_ROOT/.installing.XXXXXX")"
    trap 'rm -rf "$tmp"' EXIT
    for bin in "${BINARIES[@]}"; do
      cp "${CARGO_TARGET_DIR:-$ROOT/target}/release/$bin" "$tmp/$bin"
    done
    chmod 755 "$tmp"
    mv "$tmp" "$INSTALL_ROOT/$version"
    trap - EXIT
  fi
  make_current "$version"
}

[ "$(uname)" = Darwin ] || die "installing is macOS only for now"
case "${1:-}" in
  "") install_this_commit no ;;
  --allow-dirty) install_this_commit yes ;;
  --list) list_versions ;;
  --use) make_current "${2:?--use needs a version. See --list}" ;;
  *) die "unknown option $1. See the top of this script" ;;
esac
