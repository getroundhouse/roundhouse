#!/usr/bin/env bash
# Packages Linux binaries as Roundhouse's AppImage and its update package, with Velopack's `vpk`. It runs on Linux, and
# takes the three binaries already built, which `tools/build.sh --package` makes in an Ubuntu 22.04 container and
# stamps with the version. Needs `jq`, `unzip`, `readelf`, and the .NET SDK 8, 9 or 10, which runs `vpk`, and
# `mksquashfs` and `unsquashfs`, which make and read the AppImage, from Ubuntu's `squashfs-tools`.
#
#   tools/package/package-linux.sh --prebuilt DIR              Package the binaries in DIR, built from this commit
#   tools/package/package-linux.sh --prebuilt DIR --arch arm64 The same, for binaries built for ARM64, not amd64
#   tools/package/package-linux.sh --prebuilt DIR --out DIR    Write to DIR rather than target/package-linux-ARCH
#   tools/package/package-linux.sh --print-version             Print the version this commit packages as, and stop
#
# Writes `releases/`: the AppImage people download, the full package and the feed. The default directory's
# `releases/` starts empty. Another directory's keeps what's there, and a package from an earlier version gets a delta
# to this one.
#
# The version is `0.1.<commits>`, as for the Mac, and each platform publishes to a feed of its own. An app looks for
# updates in the channel it was packaged for, which is `linux-amd64` for amd64 and `linux-arm64` for ARM64, so each gets
# its own processor's. The binaries may need no glibc newer than Ubuntu 22.04's, so the app runs there and on every
# distro since.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
# BINARIES, ROOT, version, stamp and target_dir.
# shellcheck source=SCRIPTDIR/../build.sh
source "$HERE/../build.sh"
# PACK_ID, die, dotnet_tools, dotnet_here, refuse_vpk_variables, check_newer and full_package.
# shellcheck source=SCRIPTDIR/vpk.sh
source "$HERE/vpk.sh"
# The app's icon, at 512 pixels. `docs/spec/look.md` says which style it is.
ICON="$ROOT/apps/desktop/icons/working.iconset/icon_256x256@2x.png"
# Ubuntu 22.04's glibc, the newest any binary may need.
GLIBC=2.35

# Which processor a Linux program is built for, as this script names them, from the program's header.
built_for() {
  case "$(readelf -h "$1" | sed -n 's/^ *Machine: *//p')" in
    "Advanced Micro Devices X86-64") echo amd64 ;;
    AArch64) echo arm64 ;;
    *) echo "something else" ;;
  esac
}

# The newest glibc a program needs, from the versions of it the program names.
glibc_needed() {
  readelf -V --wide "$1" | grep -o 'GLIBC_[0-9][0-9.]*' | sed 's/^GLIBC_//' | sort -uV | tail -n 1
}

# Puts the binaries in `bins/` in a directory of their own, once each binary is this build, for this processor, and
# needs no newer glibc than GLIBC. The binaries can't be run here to ask them, but each holds its build.
stage() {
  local staging=$1 prebuilt=$2 bin for needs
  mkdir "$staging/bins"
  for bin in "${BINARIES[@]}"; do
    [ -f "$prebuilt/$bin" ] || die "$prebuilt has no $bin"
    grep -qaF "$BUILD" "$prebuilt/$bin" || die "$bin in $prebuilt isn't build $BUILD"
    for="$(built_for "$prebuilt/$bin")"
    [ "$for" = "$ARCH" ] || die "$bin in $prebuilt is built for $for, not $ARCH"
    needs="$(glibc_needed "$prebuilt/$bin")"
    [ "$(printf '%s\n' "$needs" "$GLIBC" | sort -V | tail -n 1)" = "$GLIBC" ] \
      || die "$bin in $prebuilt needs glibc $needs, and may need no newer than $GLIBC. Build it on Ubuntu 22.04"
    cp "$prebuilt/$bin" "$staging/bins/$bin"
    # Whatever the copy it came from lost, such as an artifact's download.
    chmod 755 "$staging/bins/$bin"
  done
}

# Where an AppImage's files start: after its runtime, a program whose section headers are the last of it.
files_start() {
  readelf -h "$1" | awk -F: '
    /Start of section headers/ { start = $2 + 0 }
    /Size of section headers/ { size = $2 + 0 }
    /Number of section headers/ { count = $2 + 0 }
    END { print start + size * count }'
}

# Checks vpk made what the release workflow publishes, under the names it publishes them by. The full package holds
# the AppImage, which holds the binaries.
check() {
  local releases=$1 appimage=$PACK_ID-$CHANNEL.AppImage bin file full held
  for file in "$appimage" "$FEED"; do
    [ -f "$releases/$file" ] || die "vpk didn't make $file. It made: $(cd "$releases" && echo *)"
  done
  full="$(full_package "$releases" "$FEED")"
  # Read whole before grep, which stops reading at the first match, so pipefail would count unzip's SIGPIPE.
  held="$(unzip -Z1 "$releases/$full")"
  grep -qx "lib/app/$PACK_ID.AppImage" <<< "$held" || die "$full doesn't hold the AppImage"
  held="$(unsquashfs -o "$(files_start "$releases/$appimage")" -ll "$releases/$appimage")"
  for bin in "${BINARIES[@]}" UpdateNix; do
    grep -qE "^-rwxr-xr-x .* squashfs-root/usr/bin/$bin\$" <<< "$held" || die "$appimage doesn't hold $bin, executable"
  done
}

package() {
  local out=$1 prebuilt=$2
  refuse_vpk_variables
  dotnet_tools
  command -v mksquashfs > /dev/null \
    || die "vpk makes the AppImage with mksquashfs, such as from \`apt install squashfs-tools\`"
  VERSION="$(version)"
  stamp no package
  mkdir -p "$out/releases"
  check_newer "$out/releases" "$FEED"
  # Not local, because the trap outlives this function if anything fails.
  staging="$(mktemp -d)"
  trap 'rm -rf "$staging"' EXIT
  stage "$staging" "$prebuilt"
  dotnet_here vpk '[linux]' --skip-updates pack --packId "$PACK_ID" --packVersion "$VERSION" --packTitle Roundhouse \
    --runtime "$RUNTIME" --channel "$CHANNEL" --packDir "$staging/bins" --mainExe roundhouse-desktop \
    --icon "$ICON" --categories Development --outputDir "$out/releases"
  check "$out/releases"
  echo "Packaged $VERSION for Linux on $ARCH, build $BUILD:"
  echo "  $out/releases/$PACK_ID-$CHANNEL.AppImage"
}

out="" prebuilt="" ARCH=amd64
while [ $# -gt 0 ]; do
  case "$1" in
    --arch) ARCH="${2:?--arch needs amd64 or arm64}"; shift ;;
    --out) out="${2:?--out needs a directory}"; shift ;;
    --prebuilt) prebuilt="$(cd "${2:?--prebuilt needs a directory}" && pwd)"; shift ;;
    --print-version) version; exit ;;
    *) die "unknown option $1. See the top of this script" ;;
  esac
  shift
done
# The channel names the feed and what people download, as `publish.py` knows them. Each names its processor, since vpk
# leaves the channel out of the AppImage's name for `linux`, its own. RUNTIME is the processor as vpk names it.
case "$ARCH" in
  amd64) CHANNEL=linux-amd64 RUNTIME=linux-x64 ;;
  arm64) CHANNEL=linux-arm64 RUNTIME=linux-arm64 ;;
  *) die "--arch is amd64 or arm64, not $ARCH" ;;
esac
FEED="releases.$CHANNEL.json"
[ -n "$prebuilt" ] || die "--prebuilt DIR is needed, with the binaries \`tools/build.sh --package DIR\` built on Linux"
if [ -z "$out" ]; then
  out="$(target_dir)/package-linux-$ARCH"
  rm -rf "$out/releases"
fi
mkdir -p "$out"
package "$(cd "$out" && pwd)" "$prebuilt"
