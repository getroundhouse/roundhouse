#!/usr/bin/env bash
# Builds the iPhone app and signs it, then either installs and opens it on a phone connected to this Mac, or uploads it
# to TestFlight.
#   TEAM_ID=<team> BUNDLE_ID=<id> DEVICE=<udid> apps/mobile-ios/scripts/build.sh [--debug] [--no-install]
#   TEAM_ID=<team> BUNDLE_ID=<id> apps/mobile-ios/scripts/build.sh --testflight [--binary PATH]
#   apps/mobile-ios/scripts/build.sh --compile PATH
#
# TEAM_ID is the Apple Developer team that signs it, BUNDLE_ID the app's identifier in that team, and DEVICE the
# phone's identifier, as `xcrun devicectl list devices` shows it. None of them are kept in the repo.
#
# Xcode manages the signing, so it can register the phone and the identifier, and make a development profile, in
# that team's Apple Developer account. Run it only when that's meant.
#
# --testflight archives it for any iPhone and uploads it to App Store Connect, for the team's own testers only, to the
# app whose record there has BUNDLE_ID. Xcode signs it for the App Store on the way, with a distribution certificate
# Apple keeps, so it reaches the team's account: through Xcode's own sign-in, or, where there's none, as in CI, through
# an App Store Connect key with the Admin role, as ASC_KEY_PATH, the .p8 file, ASC_KEY_ID and ASC_ISSUER_ID.
#
# --compile only builds the Rust binary an upload packages, and copies it to PATH, and --binary packages one built that
# way rather than building it, so the keys needn't be where the dependencies compile.
set -euo pipefail

# Release unless asked, because a debug build's first layout overflows the main thread's stack on the phone.
config=Release; profile=release; cargo_args=(--release); install=1; mode=phone; binary=""; compiled=""
while [ $# -gt 0 ]; do
  case "$1" in
    --debug) config=Debug; profile=debug; cargo_args=() ;;
    --no-install) install=0 ;;
    --testflight) mode=testflight ;;
    --binary) binary="${2:?--binary needs the path to a roundhouse-ios}"; shift ;;
    --compile) mode=compile; compiled="${2:?--compile needs the path to put roundhouse-ios at}"; shift ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
  shift
done
if [[ "$mode" != phone && "$config" != Release ]]; then
  echo "an upload is a release build, so not with --debug" >&2; exit 2
fi
if [[ -n "$binary" && "$mode" != testflight ]]; then
  echo "--binary is for --testflight" >&2; exit 2
fi
if [[ "$mode" != compile ]]; then
  : "${TEAM_ID:?set TEAM_ID to the Apple Developer team that signs the app}"
  : "${BUNDLE_ID:?set BUNDLE_ID to the app identifier to sign it as, in that team}"
fi
# devicectl lists booted simulators as connected too, so the phone is never guessed.
if [[ "$mode" == phone ]]; then
  device="${DEVICE:?set DEVICE to the phone identifier shown by: xcrun devicectl list devices}"
fi
# Either all of the key or none of it, so a missing one isn't taken for Xcode's sign-in.
key_args=()
if [[ -n "${ASC_KEY_PATH:-}${ASC_KEY_ID:-}${ASC_ISSUER_ID:-}" ]]; then
  : "${ASC_KEY_PATH:?set ASC_KEY_PATH to the .p8 file of an App Store Connect key, with ASC_KEY_ID and ASC_ISSUER_ID}"
  : "${ASC_KEY_ID:?set ASC_KEY_ID to the ID of the App Store Connect key}"
  : "${ASC_ISSUER_ID:?set ASC_ISSUER_ID to the issuer ID of the App Store Connect key}"
  key_args=(-authenticationKeyPath "$ASC_KEY_PATH" -authenticationKeyID "$ASC_KEY_ID"
    -authenticationKeyIssuerID "$ASC_ISSUER_ID")
fi

app_dir="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$app_dir/../.." && pwd)"
# version, which is the desktop's for the same commit. It needs the whole history, which only an upload insists on,
# since a shallow checkout can still build for a phone of its own.
# shellcheck source=SCRIPTDIR/../../../tools/build.sh
source "$repo/tools/build.sh"
if [[ "$mode" == testflight ]]; then
  version="$(version)"
else
  version="$(version 2>/dev/null || echo 0.1.0)"
fi

if [[ -z "$binary" ]]; then
  # The oldest iOS the app supports, which ios/project.yml sets for Xcode too.
  IPHONEOS_DEPLOYMENT_TARGET=17.0 cargo build --manifest-path "$repo/Cargo.toml" --locked \
    --target aarch64-apple-ios -p mobile-ios --bin roundhouse-ios ${cargo_args[@]+"${cargo_args[@]}"}
  target_dir="$(cargo metadata --format-version 1 --no-deps --manifest-path "$repo/Cargo.toml" |
    jq -r .target_directory)"
  binary="$target_dir/aarch64-apple-ios/$profile/roundhouse-ios"
fi
if [[ "$mode" == compile ]]; then
  cp "$binary" "$compiled"
  exit
fi

(cd "$app_dir/ios" && BUNDLE_ID="$BUNDLE_ID" xcodegen generate --quiet)
# Each build's number has to be higher than the last one uploaded. A commit count isn't, from a branch that has
# more commits than main, so it's the time, in UTC, as year-month-day.hour-minute.
# Each part is read as a number, so the time loses its leading zero.
stamp="$(date -u +%Y%m%d.%H%M)"
build_number="${stamp%%.*}.$((10#${stamp#*.}))"
build_settings=(BUNDLE_ID="$BUNDLE_ID" RUST_BINARY="$binary" MARKETING_VERSION="$version"
  CURRENT_PROJECT_VERSION="$build_number")

if [[ "$mode" == testflight ]]; then
  work="$app_dir/build/testflight"
  rm -rf "$work"
  mkdir -p "$work"
  # Unsigned, so archiving needs no certificate. Exporting signs it for the App Store, with a profile and a certificate
  # Xcode makes in the team's account as it needs them.
  xcodebuild archive -quiet -destination "generic/platform=iOS" \
    -project "$app_dir/ios/Roundhouse.xcodeproj" -scheme Roundhouse -configuration Release \
    -derivedDataPath "$app_dir/build/DerivedData" -archivePath "$work/Roundhouse.xcarchive" \
    CODE_SIGNING_ALLOWED=NO DEVELOPMENT_TEAM="$TEAM_ID" "${build_settings[@]}"
  # Internal testing only, since anything further goes through Apple's review. Without symbols, since the archive's
  # are the stub's, not the Rust binary's. The build number is the one above, not one Xcode picks.
  cat >"$work/ExportOptions.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>method</key><string>app-store-connect</string>
	<key>destination</key><string>upload</string>
	<key>signingStyle</key><string>automatic</string>
	<key>teamID</key><string>$TEAM_ID</string>
	<key>testFlightInternalTestingOnly</key><true/>
	<key>uploadSymbols</key><false/>
	<key>manageAppVersionAndBuildNumber</key><false/>
</dict>
</plist>
EOF
  xcodebuild -exportArchive -allowProvisioningUpdates ${key_args[@]+"${key_args[@]}"} \
    -archivePath "$work/Roundhouse.xcarchive" -exportOptionsPlist "$work/ExportOptions.plist" \
    -exportPath "$work/export"
  echo "Uploaded $version ($build_number) to App Store Connect, which processes it before TestFlight offers it"
  exit
fi

built="$app_dir/build/DerivedData/Build/Products/$config-iphoneos/roundhouse-ios.app"
# The last build phase copies the Rust binary over the stub on every build, but Xcode signs again only when it
# thinks the bundle changed, so a rebuild would leave the new binary unsigned. With no bundle it signs again.
rm -rf "$built"

# Building for the phone itself, rather than any iOS device, is what lets Xcode add it to the profile.
xcodebuild build -quiet -allowProvisioningUpdates -allowProvisioningDeviceRegistration \
  -destination "platform=iOS,id=$device" \
  -project "$app_dir/ios/Roundhouse.xcodeproj" -scheme Roundhouse -configuration "$config" \
  -derivedDataPath "$app_dir/build/DerivedData" \
  CODE_SIGN_IDENTITY="Apple Development" DEVELOPMENT_TEAM="$TEAM_ID" "${build_settings[@]}"

echo "Built $built"
if [[ "$install" == 1 ]]; then
  xcrun devicectl device install app --device "$device" "$built"
  xcrun devicectl device process launch --terminate-existing --device "$device" "$BUNDLE_ID"
fi
