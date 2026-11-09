#!/usr/bin/env bash
# Builds the iPhone app, signs it for development, and installs and opens it on a phone connected to this Mac.
#   TEAM_ID=<team> BUNDLE_ID=<id> DEVICE=<udid> apps/mobile-ios/scripts/build.sh [--debug] [--no-install]
#
# TEAM_ID is the Apple Developer team that signs it, BUNDLE_ID the app's identifier in that team, and DEVICE the
# phone's identifier, as `xcrun devicectl list devices` shows it. None of them are kept in the repo.
#
# Xcode manages the signing, so it can register the phone and the identifier, and make a development profile, in
# that team's Apple Developer account. Run it only when that's meant.
set -euo pipefail

: "${TEAM_ID:?set TEAM_ID to the Apple Developer team that signs the app}"
: "${BUNDLE_ID:?set BUNDLE_ID to the app identifier to sign it as, in that team}"
# devicectl lists booted simulators as connected too, so the phone is never guessed.
device="${DEVICE:?set DEVICE to the phone identifier shown by: xcrun devicectl list devices}"

# Release unless asked, because a debug build's first layout overflows the main thread's stack on the phone.
config=Release; profile=release; cargo_args=(--release); install=1
for arg in "$@"; do
  case "$arg" in
    --debug) config=Debug; profile=debug; cargo_args=() ;;
    --no-install) install=0 ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done

app_dir="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$app_dir/../.." && pwd)"
target_dir="$(cargo metadata --format-version 1 --no-deps --manifest-path "$repo/Cargo.toml" | jq -r .target_directory)"

# The oldest iOS the app supports, which ios/project.yml sets for Xcode too.
IPHONEOS_DEPLOYMENT_TARGET=17.0 cargo build --manifest-path "$repo/Cargo.toml" --locked \
  --target aarch64-apple-ios -p mobile-ios --bin roundhouse-ios ${cargo_args[@]+"${cargo_args[@]}"}

built="$app_dir/build/DerivedData/Build/Products/$config-iphoneos/roundhouse-ios.app"
# The last build phase copies the Rust binary over the stub on every build, but Xcode signs again only when it
# thinks the bundle changed, so a rebuild would leave the new binary unsigned. With no bundle it signs again.
rm -rf "$built"

(cd "$app_dir/ios" && BUNDLE_ID="$BUNDLE_ID" xcodegen generate --quiet)
# Each build's number has to be higher than the last one uploaded. A commit count isn't, from a branch that has
# more commits than main, so it's the time, in UTC, as year-month-day.hour-minute.
# Each part is read as a number, so the time loses its leading zero.
stamp="$(date -u +%Y%m%d.%H%M)"
build_number="${stamp%%.*}.$((10#${stamp#*.}))"
version="$(cargo metadata --format-version 1 --no-deps --manifest-path "$repo/Cargo.toml" \
  | jq -r '.packages[] | select(.name == "mobile-ios") | .version')"
# Building for the phone itself, rather than any iOS device, is what lets Xcode add it to the profile.
xcodebuild build -quiet -allowProvisioningUpdates -allowProvisioningDeviceRegistration \
  -destination "platform=iOS,id=$device" \
  -project "$app_dir/ios/Roundhouse.xcodeproj" -scheme Roundhouse -configuration "$config" \
  -derivedDataPath "$app_dir/build/DerivedData" \
  CODE_SIGN_STYLE=Automatic CODE_SIGN_IDENTITY="Apple Development" DEVELOPMENT_TEAM="$TEAM_ID" \
  BUNDLE_ID="$BUNDLE_ID" RUST_BINARY="$target_dir/aarch64-apple-ios/$profile/roundhouse-ios" \
  MARKETING_VERSION="$version" CURRENT_PROJECT_VERSION="$build_number"

echo "Built $built"
if [[ "$install" == 1 ]]; then
  xcrun devicectl device install app --device "$device" "$built"
  xcrun devicectl device process launch --terminate-existing --device "$device" "$BUNDLE_ID"
fi
