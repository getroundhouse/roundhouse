#!/usr/bin/env bash
# Packages this checkout as Roundhouse.app with Velopack's `vpk`: the three binaries, which `tools/build.sh` builds and
# stamps with the version, in one bundle with Velopack's updater. It makes the portable zip people download, and the
# update package and feed the release workflow publishes. Unsigned unless it's given an identity. It's macOS on Apple
# silicon only, like the app, and needs `jq` and the .NET SDK 8, 9 or 10, which runs `vpk`.
#
#   tools/package/package.sh                  Package this commit, unsigned
#   tools/package/package.sh --allow-dirty    The same, with uncommitted changes in the build
#   tools/package/package.sh --out DIR        Write to DIR rather than target/package
#   tools/package/package.sh --sign IDENTITY [--keychain PATH] [--notary-profile NAME]
#       Sign with a Developer ID, named as `codesign -s` takes it, or `-` for ad hoc, and given a notarytool profile,
#       notarize and staple
#
# Writes `releases/`, which holds the portable zip, the full package and the feed, and `Roundhouse.app`, unzipped from
# the portable zip, to check or run. The default directory's `releases/` starts empty. Another directory's keeps what's
# there, and a package from an earlier version gets a delta to this one.
#
# The version is `0.1.<commits>`, as `docs/architecture.md` says, and it's the build the app and its hub say they are. A
# package with uncommitted changes gets its commit's version, which is never published, since the release workflow
# packages a clean checkout, and its build says when it was built after the version.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
# BINARIES, ROOT, version, stamp, target_dir and build_release.
# shellcheck source=SCRIPTDIR/../build.sh
source "$HERE/../build.sh"
# Velopack's name for the app, which names its packages and its cache directory. The bundle identifier is in
# Info.plist.
PACK_ID=Roundhouse
# The app's icon, a drawing for each size. `docs/spec/look.md` says which style it is.
ICONSET="$ROOT/apps/desktop/icons/working.iconset"

die() { echo "error: $1" >&2; exit 1; }

# Runs .NET from the directory that holds dotnet-tools.json, which is how it finds the vpk that file pins.
dotnet_here() {
  (cd "$HERE" && DOTNET_CLI_TELEMETRY_OPTOUT=1 DOTNET_NOLOGO=1 dotnet "$@")
}

# Stops before the build if `releases/` has this version or a newer one, since vpk refuses a version that isn't newer
# than every one there, but only after the build. This is the check for packaging into an --out that holds earlier
# releases.
check_newer() {
  local releases=$1 newer
  [ -f "$releases/releases.osx.json" ] || return 0
  newer="$(jq -r --arg version "$VERSION" '
    def parts: split("-")[0] | split(".") | map(tonumber);
    [.Assets[].Version | select(parts >= ($version | parts))] | first // empty' "$releases/releases.osx.json")"
  [ -z "$newer" ] || die "$releases already has version $newer, and vpk only adds a newer one than every one there.
Package into another --out"
}

# Puts the binaries in `bins/`, and the bundle's Info.plist and icon beside it, in a directory of their own.
stage() {
  local staging=$1
  build_release "$staging/bins"
  iconutil -c icns "$ICONSET" -o "$staging/AppIcon.icns"
  cp "$HERE/Info.plist" "$staging/Info.plist"
  plutil -replace CFBundleShortVersionString -string "$VERSION" "$staging/Info.plist"
  plutil -replace CFBundleVersion -string "$VERSION" "$staging/Info.plist"
  # vpk copies the icon into Resources under the name it's given.
  plutil -replace CFBundleIconFile -string AppIcon.icns "$staging/Info.plist"
}

# Checks the bundle is what this script meant to make, since vpk changes the binaries when it signs them.
check() {
  local app=$1 signed=$2 bin key value file
  for key in CFBundleShortVersionString CFBundleVersion; do
    value="$(plutil -extract "$key" raw "$app/Contents/Info.plist")"
    [ "$value" = "$VERSION" ] || die "the bundle's $key is $value, not $VERSION"
  done
  for bin in "${BINARIES[@]}"; do
    value="$(lipo -archs "$app/Contents/MacOS/$bin")"
    [ "$value" = arm64 ] || die "$bin is built for $value, not arm64"
  done
  # A package's build is its version, so this is what the app and its hub will say they are. The app has no
  # --version, since it opens a window, but it shows its build, so the build is in the binary.
  for bin in roundhoused roundhouse; do
    value="$("$app/Contents/MacOS/$bin" --version)"
    [ "$value" = "$bin $BUILD" ] || die "$bin says it's \"$value\", not $BUILD"
  done
  grep -qaF "$BUILD" "$app/Contents/MacOS/roundhouse-desktop" || die "roundhouse-desktop doesn't hold its build, $BUILD"
  [ "$signed" = yes ] || return 0
  codesign --verify --deep --strict "$app"
  # Every executable, Velopack's updater too, has the hardened runtime and none of its exceptions.
  for file in "${BINARIES[@]/#/$app/Contents/MacOS/}" "$app/Contents/MacOS/UpdateMac"; do
    # Read whole before grep, which stops reading at the first match, so pipefail would count codesign's SIGPIPE.
    value="$(codesign -d --verbose=1 "$file" 2>&1)"
    grep -Eq '^CodeDirectory .*flags=0x[0-9a-f]+\([^)]*runtime' <<< "$value" \
      || die "$(basename "$file") isn't signed with the hardened runtime"
    value="$(codesign -d --entitlements - --xml "$file" 2> /dev/null | plutil -convert json -o - - 2> /dev/null || true)"
    [ "$value" = "{}" ] || die "$(basename "$file") is signed with entitlements other than Roundhouse.entitlements"
  done
}

package() {
  local allow_dirty=$1 out=$2 sign=$3 keychain=$4 notary=$5 vpk_vars signing=()
  command -v dotnet > /dev/null || die "vpk needs the .NET SDK 8, 9 or 10, such as from \`brew install dotnet\`"
  # vpk also reads each option from a VPK_ variable, which would override this script, such as signing with vpk's
  # default entitlements, which are .NET's exceptions to the hardened runtime.
  vpk_vars="$(compgen -e | grep '^VPK_' || true)"
  [ -z "$vpk_vars" ] || die "unset $(echo "$vpk_vars" | tr '\n' ' ')first, since vpk would take them over this script"
  dotnet_here tool restore > /dev/null
  VERSION="$(version)"
  stamp "$allow_dirty" package
  mkdir -p "$out/releases"
  check_newer "$out/releases"
  # Not local, because the trap outlives this function if anything fails.
  staging="$(mktemp -d)"
  trap 'rm -rf "$staging"' EXIT
  stage "$staging"
  if [ -n "$sign" ]; then
    signing+=(--signAppIdentity "$sign")
    [ -z "$keychain" ] || signing+=(--keychain "$keychain")
    [ -z "$notary" ] || signing+=(--notaryProfile "$notary")
  fi
  # The portable zip and no installer. vpk's installer puts the app in /Applications as root, so every update would
  # then ask for an admin password. The entitlements go in even unsigned, so nothing else can choose them.
  dotnet_here vpk --skip-updates pack --packId "$PACK_ID" --packVersion "$VERSION" --packTitle Roundhouse \
    --runtime osx-arm64 --packDir "$staging/bins" --mainExe roundhouse-desktop --plist "$staging/Info.plist" \
    --icon "$staging/AppIcon.icns" --signEntitlements "$HERE/Roundhouse.entitlements" --noInst \
    --outputDir "$out/releases" ${signing[@]+"${signing[@]}"}
  rm -rf "$out/Roundhouse.app"
  ditto -x -k "$out/releases/$PACK_ID-osx-Portable.zip" "$out"
  check "$out/Roundhouse.app" "$([ -n "$sign" ] && echo yes || echo no)"
  echo "Packaged $VERSION, build $BUILD:"
  echo "  $out/Roundhouse.app"
  echo "  $out/releases/$PACK_ID-osx-Portable.zip"
  echo "It's a release build, so opening it uses your own hub and data. To try it on throwaway ones, run it directly,"
  echo "since \`open\` doesn't pass the environment on, then stop the hub it started:"
  echo "  export ROUNDHOUSE_HOME=\"\$(mktemp -d)\" ROUNDHOUSE_WORKSPACES=\"\$(mktemp -d)\""
  echo "  \"$out/Roundhouse.app/Contents/MacOS/roundhouse-desktop\""
  echo "  \"$out/Roundhouse.app/Contents/MacOS/roundhouse\" hub stop"
}

[ "$(uname)" = Darwin ] || die "packaging is macOS only"
allow_dirty=no out="" sign="" keychain="" notary=""
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
if [ -z "$out" ]; then
  out="$(target_dir)/package"
  rm -rf "$out/releases"
fi
mkdir -p "$out"
package "$allow_dirty" "$(cd "$out" && pwd)" "$sign" "$keychain" "$notary"
