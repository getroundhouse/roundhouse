#!/usr/bin/env bash
# Builds the three binaries in release, stamped with which build they are. `tools/install.sh` and
# `tools/package/package.sh` source this, so they build and stamp the same way, and the release workflow runs it, on a
# Mac and on Windows. Needs `jq`, which macOS has.
#
#   tools/build.sh DIR                  Build this commit, put the binaries in DIR, and print the build
#   tools/build.sh --package DIR        The same, stamped as a package, with the version people download
#   tools/build.sh --allow-dirty ...    Either, with uncommitted changes in the build
#
# `docs/architecture.md` says what the stamp is for. ROUNDHOUSE_BUILD is a package's version, or an install's commit,
# and ROUNDHOUSE_BUILD_NUMBER when the commit was made, so a hub can tell an older build. With uncommitted changes, the
# build also says when it was built, and the number is that time, since the same commit can be built with different
# changes.

BINARIES=(roundhouse-desktop roundhoused roundhouse)
# What a program's file ends in where this runs, which is Git's bash on Windows.
case "$(uname -s)" in
  MINGW* | MSYS* | CYGWIN*) EXE=.exe ;;
  *) EXE="" ;;
esac
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# The version, `0.1.<commits>`, as `docs/architecture.md` says. It refuses a shallow clone, which would count only the
# commits it fetched.
version() {
  [ "$(git -C "$ROOT" rev-parse --is-shallow-repository)" = false ] || {
    echo "error: this is a shallow clone, so its commits can't be counted for the version. Fetch the whole history" >&2
    return 1
  }
  echo "0.1.$(git -C "$ROOT" rev-list --count HEAD)"
}

# Sets BUILD and NUMBER, once, so everything built from this checkout says the same. KIND is `install`, whose build is
# the commit, or `package`, whose build is the version.
stamp() {
  local allow_dirty=$1 kind=$2 now
  case "$kind" in
    install) BUILD="$(git -C "$ROOT" rev-parse --short=12 HEAD)" ;;
    package) BUILD="$(version)" || return 1 ;;
    *) echo "error: a build is an install or a package, not $kind" >&2; return 1 ;;
  esac
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
    cp "$target/release/$bin$EXE" "$dest/$bin$EXE"
  done
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  set -euo pipefail
  allow_dirty=no kind=install
  while [[ "${1:-}" == --* ]]; do
    case "$1" in
      --allow-dirty) allow_dirty=yes ;;
      --package) kind=package ;;
      *) echo "error: unknown option $1. See the top of this script" >&2; exit 1 ;;
    esac
    shift
  done
  [ $# -eq 1 ] || {
    echo "error: tools/build.sh [--allow-dirty] [--package] DIR. See the top of this script" >&2
    exit 1
  }
  stamp "$allow_dirty" "$kind"
  # Cargo writes to stderr, so the build is all that's printed.
  build_release "$1"
  echo "$BUILD"
fi
