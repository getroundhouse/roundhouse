#!/usr/bin/env bash
# Packages this checkout as Roundhouse.app with Velopack's `vpk`: the three binaries, built in release and stamped as
# `tools/install.sh` stamps them, in one bundle with Velopack's updater. It makes the portable zip people download,
# and the update package and feed that `vpk upload` publishes. Unsigned unless it's given an identity. It's macOS
# only, like the app, and needs the .NET SDK 8 or newer, which runs `vpk`.
#
#   tools/package/package.sh                  Package this commit, unsigned
#   tools/package/package.sh --allow-dirty    The same, with uncommitted changes in the build
#   tools/package/package.sh --out DIR        Write to DIR rather than target/package
#   tools/package/package.sh --sign IDENTITY [--keychain PATH] [--notary-profile NAME]
#       Sign with a Developer ID, named as `codesign -s` takes it, and given a notarytool profile, notarize and staple
#
# Writes `releases/`, which holds the portable zip, the full package and the feed, and `Roundhouse.app`, unzipped from
# the portable zip, to check or run. A package from an earlier version left in `releases/` gets a delta to this one.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
BINARIES=(roundhouse-desktop roundhoused roundhouse)
# Velopack's name for the app, which names its packages and its cache directory. The bundle identifier is in
# Info.plist.
PACK_ID=Roundhouse
# The app's icon, a drawing for each size. Until the repo has one, the bundle gets vpk's default.
ICONSET="$ROOT/apps/desktop/AppIcon.iconset"

die() { echo "error: $1" >&2; exit 1; }

# A Unix time in UTC, in a `date` format. macOS's own `date`, since GNU's, which some put first on PATH, reads `-r` as a
# file.
utc() { /bin/date -u -r "$1" "+$2"; }

# Velopack orders releases by version, and the app orders hubs by `ROUNDHOUSE_BUILD_NUMBER`, so both come from one
# time, in UTC: year.monthday.time, such as 2026.1108.153045. Semver forbids leading zeros, so January 2nd is 102 and
# 00:00:09 is 9, which still sort in time order as numbers.
version_at() {
  printf '%d.%d.%d\n' "$(utc "$1" %Y)" "$((10#$(utc "$1" %m%d)))" "$((10#$(utc "$1" %H%M%S)))"
}

# Runs .NET from the directory that holds dotnet-tools.json, which is how it finds the vpk that file pins.
dotnet_here() {
  (cd "$HERE" && DOTNET_CLI_TELEMETRY_OPTOUT=1 DOTNET_NOLOGO=1 dotnet "$@")
}

# Sets BUILD and NUMBER the way `tools/install.sh` does, so a packaged hub and an installed one can tell which is
# older, and VERSION from the same time.
stamp() {
  local allow_dirty=$1
  BUILD="$(git -C "$ROOT" rev-parse --short=12 HEAD)"
  NUMBER="$(git -C "$ROOT" log -1 --format=%ct)"
  if [ -n "$(git -C "$ROOT" status --porcelain)" ]; then
    [ "$allow_dirty" = yes ] || die "this checkout has uncommitted changes. Commit them, or use --allow-dirty"
    NUMBER="$(date +%s)"
    BUILD="$BUILD-dirty-$(utc "$NUMBER" %Y%m%d%H%M%S)"
  fi
  VERSION="$(version_at "$NUMBER")"
}

# Puts the binaries in `bins/`, and the bundle's Info.plist and icon beside it, in a directory of their own.
stage() {
  local staging=$1 bin icon=DefaultApp.icns
  (cd "$ROOT" && ROUNDHOUSE_BUILD="$BUILD" ROUNDHOUSE_BUILD_NUMBER="$NUMBER" \
    cargo build --release --locked -p desktop -p daemon -p cli --bins)
  mkdir "$staging/bins"
  for bin in "${BINARIES[@]}"; do
    cp "${CARGO_TARGET_DIR:-$ROOT/target}/release/$bin" "$staging/bins/$bin"
  done
  if [ -d "$ICONSET" ]; then
    icon=AppIcon.icns
    iconutil -c icns "$ICONSET" -o "$staging/$icon"
  fi
  cp "$HERE/Info.plist" "$staging/Info.plist"
  plutil -replace CFBundleShortVersionString -string "$VERSION" "$staging/Info.plist"
  plutil -replace CFBundleVersion -string "$VERSION" "$staging/Info.plist"
  # vpk puts the icon in Resources under its own file name, and puts in its default when it isn't given one.
  plutil -replace CFBundleIconFile -string "$icon" "$staging/Info.plist"
}

package() {
  local allow_dirty=$1 out=$2 sign=$3 keychain=$4 notary=$5 icon=() signing=()
  command -v dotnet > /dev/null || die "vpk needs the .NET SDK 8 or newer, such as from \`brew install dotnet\`"
  dotnet_here tool restore > /dev/null
  stamp "$allow_dirty"
  # Not local, because the trap outlives this function if anything fails.
  staging="$(mktemp -d)"
  trap 'rm -rf "$staging"' EXIT
  stage "$staging"
  [ ! -f "$staging/AppIcon.icns" ] || icon=(--icon "$staging/AppIcon.icns")
  if [ -n "$sign" ]; then
    signing+=(--signAppIdentity "$sign" --signEntitlements "$HERE/Roundhouse.entitlements")
    [ -z "$keychain" ] || signing+=(--keychain "$keychain")
    [ -z "$notary" ] || signing+=(--notaryProfile "$notary")
  fi
  mkdir -p "$out/releases"
  # The portable zip and no installer. vpk's installer puts the app in /Applications as root, so every update would
  # then ask for an admin password.
  dotnet_here vpk --skip-updates pack --packId "$PACK_ID" --packVersion "$VERSION" --packTitle Roundhouse \
    --packDir "$staging/bins" --mainExe roundhouse-desktop --plist "$staging/Info.plist" --noInst \
    --outputDir "$out/releases" ${icon[@]+"${icon[@]}"} ${signing[@]+"${signing[@]}"}
  rm -rf "$out/Roundhouse.app"
  ditto -x -k "$out/releases/$PACK_ID-osx-Portable.zip" "$out"
  echo "Packaged $VERSION, build $BUILD:"
  echo "  $out/Roundhouse.app"
  echo "  $out/releases/$PACK_ID-osx-Portable.zip"
}

[ "$(uname)" = Darwin ] || die "packaging is macOS only"
allow_dirty=no out="${CARGO_TARGET_DIR:-$ROOT/target}/package" sign="" keychain="" notary=""
while [ $# -gt 0 ]; do
  case "$1" in
    --allow-dirty) allow_dirty=yes ;;
    --out) out="${2:?--out needs a directory}"; shift ;;
    --sign) sign="${2:?--sign needs an identity}"; shift ;;
    --keychain) keychain="${2:?--keychain needs a path}"; shift ;;
    --notary-profile) notary="${2:?--notary-profile needs a name}"; shift ;;
    *) die "unknown option $1. See the top of this script" ;;
  esac
  shift
done
[ -n "$sign" ] || [ -z "$keychain$notary" ] || die "--keychain and --notary-profile are for signing, so they need --sign"
mkdir -p "$out"
package "$allow_dirty" "$(cd "$out" && pwd)" "$sign" "$keychain" "$notary"
