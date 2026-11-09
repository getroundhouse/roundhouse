#!/usr/bin/env bash
# Builds the three binaries in release, stamped with which build they are. `tools/install.sh` and
# `tools/package/package.sh` source this, so they build and stamp the same way. Needs `jq`, which macOS has.
#
#   tools/build.sh DIR                  Build this commit, put the binaries in DIR, and print the build
#   tools/build.sh --allow-dirty DIR    The same, with uncommitted changes in the build
#
# `docs/architecture.md` says what the stamp is for. ROUNDHOUSE_BUILD is the commit and ROUNDHOUSE_BUILD_NUMBER when it
# was made, so a hub can tell an older build. With uncommitted changes, both say when it was built, since the same
# commit can be built with different changes.

BINARIES=(roundhouse-desktop roundhoused roundhouse)
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Sets BUILD and NUMBER, once, so everything built from this checkout says the same.
stamp() {
  local allow_dirty=$1 now
  BUILD="$(git -C "$ROOT" rev-parse --short=12 HEAD)"
  NUMBER="$(git -C "$ROOT" log -1 --format=%ct)"
  if [ -n "$(git -C "$ROOT" status --porcelain)" ]; then
    [ "$allow_dirty" = yes ] || {
      echo "error: this checkout has uncommitted changes. Commit them, or use --allow-dirty" >&2
      return 1
    }
    # One `date`, so the number and the suffix are the same second, in UTC.
    now="$(date -u '+%s %Y%m%d%H%M%S')"
    NUMBER="${now% *}"
    BUILD="$BUILD-dirty-${now#* }"
  fi
}

# Where cargo puts what it builds, which `build.target-dir` or CARGO_TARGET_DIR can move.
target_dir() {
  (cd "$ROOT" && cargo metadata --no-deps --format-version 1 --locked) | jq -r .target_directory
}

# Builds the binaries in release with the stamp `stamp` set, and copies them into DEST.
build_release() {
  local dest=$1 target bin
  : "${BUILD:?stamp comes first}" "${NUMBER:?stamp comes first}"
  (cd "$ROOT" && ROUNDHOUSE_BUILD="$BUILD" ROUNDHOUSE_BUILD_NUMBER="$NUMBER" \
    cargo build --release --locked -p desktop -p daemon -p cli --bins)
  target="$(target_dir)"
  mkdir -p "$dest"
  for bin in "${BINARIES[@]}"; do
    cp "$target/release/$bin" "$dest/$bin"
  done
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  set -euo pipefail
  allow_dirty=no
  if [ "${1:-}" = --allow-dirty ]; then
    allow_dirty=yes
    shift
  fi
  [ $# -eq 1 ] || { echo "error: tools/build.sh [--allow-dirty] DIR. See the top of this script" >&2; exit 1; }
  stamp "$allow_dirty"
  # Cargo writes to stderr, so the build is all that's printed.
  build_release "$1"
  echo "$BUILD"
fi
