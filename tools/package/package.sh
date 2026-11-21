#!/usr/bin/env bash
# Packages this checkout as Roundhouse.app with Velopack's `vpk`: the three binaries, which `tools/build.sh` builds and
# stamps with the version, and the library beside them that pauses what's playing while dictating, in one bundle with
# Velopack's updater. It makes the portable zip people download, the update package and feed the release workflow
# publishes, and the hub's archive, for a Mac with no app on it, from the binaries in the bundle. Unsigned unless it's
# given an identity. It's macOS on Apple silicon only, like the app, and needs `jq` and the .NET SDK 8, 9 or 10, which
# runs `vpk`.
#
#   tools/package/package.sh                  Package this commit, unsigned
#   tools/package/package.sh --allow-dirty    The same, with uncommitted changes in the build
#   tools/package/package.sh --out DIR        Write to DIR rather than target/package
#   tools/package/package.sh --prebuilt DIR   Package the binaries in DIR, which `tools/build.sh --package` built
#                                             from this commit
#   tools/package/package.sh --print-version  Print the version this commit packages as, and stop
#   tools/package/package.sh --sign IDENTITY [--keychain PATH] [--notary-profile NAME]
#       Sign with a Developer ID, named as `codesign -s` takes it, or `-` for ad hoc, and given a notarytool profile,
#       notarize and staple, then check Gatekeeper takes it
#
# Writes `releases/`, which holds the portable zip, the full package, the feed and the hub's archive,
# `Roundhouse-VERSION-macos-arm64-hub.tar.gz`, and `Roundhouse.app`, unzipped from the portable zip, to check or run.
# The default directory's `releases/` starts empty. Another directory's keeps what's there, and a package from an
# earlier version gets a delta to this one.
#
# An app looks for updates in the channel it was packaged for, `macos-arm64`, named for its processor as the others'
# are. One packaged before that was `osx`, Velopack's default, and `publish.py` keeps that feed leading to this one.
#
# The version is `0.1.<commits>`, as `docs/decisions/publish-every-merge.md` says, and it's the build the app and its
# hub say they are. A package with uncommitted changes gets its commit's version, which is never published, since the
# release workflow packages a clean checkout, and its build says when it was built after the version.
#
# The app updates from the published feed. To try updating from another, such as a local server with a key made for
# the test, package with ROUNDHOUSE_UPDATE_URL and ROUNDHOUSE_UPDATE_KEY set, which the build reads, and publish to it
# with `publish.py`.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
# BINARIES, ROOT, version, stamp, target_dir and build_release.
# shellcheck source=SCRIPTDIR/../build.sh
source "$HERE/../build.sh"
# PACK_ID, die, dotnet_tools, dotnet_here, refuse_vpk_variables, check_newer, hub_archive_name and hub_archive. The
# bundle identifier is in Info.plist.
# shellcheck source=SCRIPTDIR/vpk.sh
source "$HERE/vpk.sh"
# The app's icon, a drawing for each size. `docs/spec/components.md` says which style it is.
ICONSET="$ROOT/apps/desktop/icons/working.iconset"
# The channel, which names the feed and what people download, as `publish.py` knows them.
CHANNEL=macos-arm64

# Puts the binaries and NOW_PLAYING in `bins/`, and the bundle's Info.plist and icon beside it, in a directory of
# their own. They come from `prebuilt` if it's set, and `check` makes sure the binaries are this build.
stage() {
  local staging=$1 prebuilt=$2 bin
  if [ -z "$prebuilt" ]; then
    build_release "$staging/bins"
  else
    mkdir "$staging/bins"
    for bin in "${BINARIES[@]}" "$NOW_PLAYING"; do
      cp "$prebuilt/$bin" "$staging/bins/$bin"
    done
  fi
  iconutil -c icns "$ICONSET" -o "$staging/AppIcon.icns"
  cp "$HERE/Info.plist" "$staging/Info.plist"
  plutil -replace CFBundleShortVersionString -string "$VERSION" "$staging/Info.plist"
  plutil -replace CFBundleVersion -string "$VERSION" "$staging/Info.plist"
  # vpk copies the icon into Resources under the name it's given.
  plutil -replace CFBundleIconFile -string AppIcon.icns "$staging/Info.plist"
}

# Checks the bundle is what this script meant to make, since vpk changes the binaries when it signs them.
check() {
  local app=$1 signed=$2 notarized=$3 bin key value file expected
  for key in CFBundleShortVersionString CFBundleVersion; do
    value="$(plutil -extract "$key" raw "$app/Contents/Info.plist")"
    [ "$value" = "$VERSION" ] || die "the bundle's $key is $value, not $VERSION"
  done
  for bin in "${BINARIES[@]}" "$NOW_PLAYING"; do
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
  expected="$(plutil -convert json -o - "$HERE/Roundhouse.entitlements")"
  # Every executable, Velopack's updater too, has the hardened runtime and only the exceptions Roundhouse.entitlements
  # gives.
  for file in "${BINARIES[@]/#/$app/Contents/MacOS/}" "$app/Contents/MacOS/UpdateMac"; do
    # Read whole before grep, which stops reading at the first match, so pipefail would count codesign's SIGPIPE.
    value="$(codesign -d --verbose=1 "$file" 2>&1)"
    grep -Eq '^CodeDirectory .*flags=0x[0-9a-f]+\([^)]*runtime' <<< "$value" \
      || die "$(basename "$file") isn't signed with the hardened runtime"
    value="$(codesign -d --entitlements - --xml "$file" 2> /dev/null | plutil -convert json -o - - 2> /dev/null || true)"
    [ "$value" = "$expected" ] || die "$(basename "$file") is signed with entitlements other than Roundhouse.entitlements"
  done
  # vpk's deep signature signs it with the rest, which gives it the hardened runtime too. A library only the linker
  # signed has none, and notarization would refuse it.
  value="$(codesign -d --verbose=1 "$app/Contents/MacOS/$NOW_PLAYING" 2>&1)"
  grep -Eq '^CodeDirectory .*flags=0x[0-9a-f]+\([^)]*runtime' <<< "$value" \
    || die "$NOW_PLAYING isn't signed with the rest of the bundle"
  [ "$notarized" = yes ] || return 0
  # Gatekeeper's verdict, as a download gets it. Its `origin=` line names whose certificate it is, so it's left out.
  value="$(spctl --assess --type execute -vv "$app" 2>&1)" \
    || die "Gatekeeper refuses the app: $(grep -v '^origin=' <<< "$value")"
  grep -qx 'source=Notarized Developer ID' <<< "$value" \
    || die "Gatekeeper doesn't take the app as notarized: $(grep -v '^origin=' <<< "$value")"
  xcrun stapler validate "$app" > /dev/null || die "the app has no notarization ticket stapled to it"
}

# Checks the hub's archive holds the bundle's own `roundhoused` and `roundhouse`, and that each, out of the bundle, is
# still signed and, once the app is notarized, still notarized, since notarizing the app notarizes each binary in it.
check_hub() {
  local archive=$1 app=$2 signed=$3 notarized=$4 unpacked=$staging/hub bin
  mkdir "$unpacked"
  tar -xzf "$archive" -C "$unpacked"
  for bin in roundhoused roundhouse; do
    cmp -s "$unpacked/$bin" "$app/Contents/MacOS/$bin" || die "$bin in $(basename "$archive") isn't the bundle's"
    [ "$signed" = yes ] || continue
    codesign --verify --strict "$unpacked/$bin" || die "$bin out of the bundle isn't signed"
    [ "$notarized" = yes ] || continue
    codesign --verify --strict -R=notarized --check-notarization "$unpacked/$bin" \
      || die "$bin out of the bundle isn't notarized"
  done
}

package() {
  local allow_dirty=$1 out=$2 prebuilt=$3 sign=$4 keychain=$5 notary=$6 signing=() signed notarized hub
  # Such as signing with vpk's default entitlements, which are .NET's exceptions to the hardened runtime.
  refuse_vpk_variables
  dotnet_tools
  VERSION="$(version)"
  stamp "$allow_dirty" package
  mkdir -p "$out/releases"
  check_newer "$out/releases" "releases.$CHANNEL.json"
  # Not local, because the trap outlives this function if anything fails.
  staging="$(mktemp -d)"
  trap 'rm -rf "$staging"' EXIT
  stage "$staging" "$prebuilt"
  if [ -n "$sign" ]; then
    signing+=(--signAppIdentity "$sign")
    [ -z "$keychain" ] || signing+=(--keychain "$keychain")
    [ -z "$notary" ] || signing+=(--notaryProfile "$notary")
  fi
  # The portable zip and no installer. vpk's installer puts the app in /Applications as root, so every update would
  # then ask for an admin password. The entitlements go in even unsigned, so nothing else can choose them.
  dotnet_here vpk --skip-updates pack --packId "$PACK_ID" --packVersion "$VERSION" --packTitle Roundhouse \
    --runtime osx-arm64 --channel "$CHANNEL" --packDir "$staging/bins" --mainExe roundhouse-desktop \
    --plist "$staging/Info.plist" --icon "$staging/AppIcon.icns" --signEntitlements "$HERE/Roundhouse.entitlements" \
    --noInst --outputDir "$out/releases" ${signing[@]+"${signing[@]}"}
  rm -rf "$out/Roundhouse.app"
  ditto -x -k "$out/releases/$PACK_ID-$CHANNEL-Portable.zip" "$out"
  signed="$([ -n "$sign" ] && echo yes || echo no)" notarized="$([ -n "$notary" ] && echo yes || echo no)"
  check "$out/Roundhouse.app" "$signed" "$notarized"
  hub="$out/releases/$(hub_archive_name "$CHANNEL")"
  hub_archive "$out/Roundhouse.app/Contents/MacOS" "$hub"
  check_hub "$hub" "$out/Roundhouse.app" "$signed" "$notarized"
  echo "Packaged $VERSION, build $BUILD:"
  echo "  $out/Roundhouse.app"
  echo "  $out/releases/$PACK_ID-$CHANNEL-Portable.zip"
  echo "  $hub"
  echo "It's a release build, so opening it uses your own hub and data. To try it on throwaway ones, run it directly,"
  echo "since \`open\` doesn't pass the environment on, then stop the hub it started:"
  echo "  export ROUNDHOUSE_HOME=\"\$(mktemp -d)\" ROUNDHOUSE_WORKSPACES=\"\$(mktemp -d)\""
  echo "  \"$out/Roundhouse.app/Contents/MacOS/roundhouse-desktop\""
  echo "  \"$out/Roundhouse.app/Contents/MacOS/roundhouse\" hub stop"
}

[ "$(uname)" = Darwin ] || die "packaging is macOS only"
allow_dirty=no out="" prebuilt="" sign="" keychain="" notary=""
while [ $# -gt 0 ]; do
  case "$1" in
    --allow-dirty) allow_dirty=yes ;;
    --out) out="${2:?--out needs a directory}"; shift ;;
    --prebuilt) prebuilt="$(cd "${2:?--prebuilt needs a directory}" && pwd)"; shift ;;
    --print-version) version; exit ;;
    --sign) sign="${2:?--sign needs an identity}"; shift ;;
    --keychain) keychain="${2:?--keychain needs a path}"; shift ;;
    --notary-profile) notary="${2:?--notary-profile needs a name}"; shift ;;
    *) die "unknown option $1. See the top of this script" ;;
  esac
  shift
done
[ -n "$sign" ] || [ -z "$keychain$notary" ] || die "--keychain and --notary-profile are for signing, so they need --sign"
# A dirty build's stamp is when it was built, which this run couldn't know.
[ -z "$prebuilt" ] || [ "$allow_dirty" = no ] || die "--prebuilt is for a clean commit's binaries, not --allow-dirty"
if [ -z "$out" ]; then
  out="$(target_dir)/package"
  rm -rf "$out/releases"
fi
mkdir -p "$out"
package "$allow_dirty" "$(cd "$out" && pwd)" "$prebuilt" "$sign" "$keychain" "$notary"
