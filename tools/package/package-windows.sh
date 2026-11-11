#!/usr/bin/env bash
# Packages Windows binaries as Roundhouse's installer and its update package, with Velopack's `vpk`. It runs on a Mac or
# on Linux, since vpk packages for Windows from either, and it takes the three binaries already built, which
# `tools/build.sh --package` makes on Windows and stamps with the version. Unsigned. Needs `jq`, `unzip`, Python 3 and
# the .NET SDK 8, 9 or 10, which runs `vpk`.
#
#   tools/package/package-windows.sh --prebuilt DIR              Package the binaries in DIR, built from this commit
#   tools/package/package-windows.sh --prebuilt DIR --arch arm64 The same, for binaries built for ARM64, not x64
#   tools/package/package-windows.sh --prebuilt DIR --out DIR    Write to DIR rather than target/package-windows-ARCH
#   tools/package/package-windows.sh --print-version             Print the version this commit packages as, and stop
#
# Writes `releases/`: the installer people download, a portable zip, the full package and the feed. The default
# directory's `releases/` starts empty. Another directory's keeps what's there, and a package from an earlier version
# gets a delta to this one.
#
# The version is `0.1.<commits>`, as for the Mac, and each platform publishes to a feed of its own. An app looks for
# updates in the channel it was packaged for, which is `win` for x64 and `win-arm64` for ARM64, so each gets its own
# processor's. The installer puts the app in `%LOCALAPPDATA%\Roundhouse`, for one user, without asking for an
# administrator.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
# BINARIES, ROOT, version, stamp and target_dir.
# shellcheck source=SCRIPTDIR/../build.sh
source "$HERE/../build.sh"
# PACK_ID, die, dotnet_tools, dotnet_here, refuse_vpk_variables and check_newer.
# shellcheck source=SCRIPTDIR/vpk.sh
source "$HERE/vpk.sh"
# The app's icon, a drawing for each size. `docs/spec/look.md` says which style it is.
ICONSET="$ROOT/apps/desktop/icons/working.iconset"

# Which processor a Windows program is built for, as this script names them, from the program's header.
built_for() {
  python3 - "$1" << 'PYTHON'
import struct, sys

with open(sys.argv[1], "rb") as program:
    start = program.read(4096)
header = struct.unpack_from("<I", start, 0x3C)[0] if start[:2] == b"MZ" else 0
machine = struct.unpack_from("<H", start, header + 4)[0] if start[header:header + 4] == b"PE\0\0" else 0
print({0x8664: "x64", 0xAA64: "arm64"}.get(machine, "something else"))
PYTHON
}

# Puts the binaries in `bins/`, and the icon beside it, in a directory of their own, once each binary is this build,
# for this processor. The binaries can't be run here to ask them, but each holds its build.
stage() {
  local staging=$1 prebuilt=$2 bin for
  mkdir "$staging/bins"
  for bin in "${BINARIES[@]}"; do
    [ -f "$prebuilt/$bin.exe" ] || die "$prebuilt has no $bin.exe"
    grep -qaF "$BUILD" "$prebuilt/$bin.exe" || die "$bin.exe in $prebuilt isn't build $BUILD"
    for="$(built_for "$prebuilt/$bin.exe")"
    [ "$for" = "$ARCH" ] || die "$bin.exe in $prebuilt is built for $for, not $ARCH"
    cp "$prebuilt/$bin.exe" "$staging/bins/$bin.exe"
  done
  python3 "$HERE/ico.py" "$ICONSET" "$staging/AppIcon.ico"
}

# Checks vpk made what the release workflow publishes, under the names it publishes them by.
check() {
  local releases=$1 bin file full held
  for file in "$PACK_ID-$CHANNEL-Setup.exe" "$PACK_ID-$CHANNEL-Portable.zip" "$FEED"; do
    [ -f "$releases/$file" ] || die "vpk didn't make $file. It made: $(cd "$releases" && echo *)"
  done
  full="$(jq -r --arg version "$VERSION" \
    '[.Assets[] | select(.Version == $version and .Type == "Full") | .FileName] | first // empty' "$releases/$FEED")"
  [ -n "$full" ] && [ -f "$releases/$full" ] || die "$FEED doesn't list a full package for $VERSION that's there"
  # Read whole before grep, which stops reading at the first match, so pipefail would count unzip's SIGPIPE.
  held="$(unzip -Z1 "$releases/$full")"
  for bin in "${BINARIES[@]}"; do
    grep -q "/$bin\.exe\$" <<< "$held" || die "$full doesn't hold $bin.exe"
  done
}

package() {
  local out=$1 prebuilt=$2
  refuse_vpk_variables
  dotnet_tools
  VERSION="$(version)"
  stamp no package
  mkdir -p "$out/releases"
  check_newer "$out/releases" "$FEED"
  # Not local, because the trap outlives this function if anything fails.
  staging="$(mktemp -d)"
  trap 'rm -rf "$staging"' EXIT
  stage "$staging" "$prebuilt"
  # `[win]` has vpk package for Windows wherever it runs.
  dotnet_here vpk '[win]' --skip-updates pack --packId "$PACK_ID" --packVersion "$VERSION" --packTitle Roundhouse \
    --runtime "win-$ARCH" --channel "$CHANNEL" --packDir "$staging/bins" --mainExe roundhouse-desktop.exe \
    --icon "$staging/AppIcon.ico" --outputDir "$out/releases"
  check "$out/releases"
  echo "Packaged $VERSION for Windows on $ARCH, build $BUILD:"
  echo "  $out/releases/$PACK_ID-$CHANNEL-Setup.exe"
}

out="" prebuilt="" ARCH=x64
while [ $# -gt 0 ]; do
  case "$1" in
    --arch) ARCH="${2:?--arch needs x64 or arm64}"; shift ;;
    --out) out="${2:?--out needs a directory}"; shift ;;
    --prebuilt) prebuilt="$(cd "${2:?--prebuilt needs a directory}" && pwd)"; shift ;;
    --print-version) version; exit ;;
    *) die "unknown option $1. See the top of this script" ;;
  esac
  shift
done
# The channel names the feed and what people download, as `publish.py` knows them.
case "$ARCH" in
  x64) CHANNEL=win ;;
  arm64) CHANNEL=win-arm64 ;;
  *) die "--arch is x64 or arm64, not $ARCH" ;;
esac
FEED="releases.$CHANNEL.json"
[ -n "$prebuilt" ] || die "--prebuilt DIR is needed, with the binaries \`tools/build.sh --package DIR\` built on Windows"
if [ -z "$out" ]; then
  out="$(target_dir)/package-windows-$ARCH"
  rm -rf "$out/releases"
fi
mkdir -p "$out"
package "$(cd "$out" && pwd)" "$prebuilt"
