#!/usr/bin/env bash
# Builds, signs, installs and launches a path on a connected iPhone.
#   TEAM_ID=<team> DEVICE=<id> [BUNDLE_ID_SUFFIX=<suffix>] scripts/device.sh <a|b> [--release]
#
# Lets Xcode manage signing, which can register the device and the bundle identifier and create a
# development profile in the Apple Developer account for TEAM_ID. Run it only when that is intended.
set -euo pipefail

path="${1:?usage: TEAM_ID=<team> DEVICE=<id> scripts/device.sh <a|b> [--release]}"; shift
: "${TEAM_ID:?set TEAM_ID to the Apple Developer team that should sign the app}"
config=Debug; cargo_args=()
if [[ "${1:-}" == "--release" ]]; then config=Release; cargo_args+=(--release); fi

root="$(cd "$(dirname "$0")/.." && pwd)"
"$root/scripts/prepare-patches.sh"
suffix="${BUNDLE_ID_SUFFIX:-}"

case "$path" in
  a) dir="$root/path-a"; scheme=GpuiKitPoc; bundle="dev.gpuikit.iospoc$suffix"; app=gpui_kit_ios_poc.app; min=15.0 ;;
  b) dir="$root/path-b"; scheme=GpuiKitPocB; bundle="dev.gpuikit.iospocb$suffix"; app=GpuiKitPocB.app; min=16.0 ;;
  *) echo "unknown path: $path" >&2; exit 2 ;;
esac

(cd "$dir" && CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-6}" IPHONEOS_DEPLOYMENT_TARGET="$min" \
  nice -n 10 cargo build --target aarch64-apple-ios ${cargo_args[@]+"${cargo_args[@]}"})

(cd "$dir/ios" && POC_BUNDLE_ID="$bundle" xcodegen generate --quiet)
nice -n 10 xcodebuild build -quiet -allowProvisioningUpdates \
  -project "$dir/ios/$scheme.xcodeproj" -scheme "$scheme" -configuration "$config" \
  -sdk iphoneos -derivedDataPath "$dir/build/DerivedData" -jobs 6 \
  CODE_SIGN_STYLE=Automatic CODE_SIGN_IDENTITY="Apple Development" DEVELOPMENT_TEAM="$TEAM_ID" \
  PRODUCT_BUNDLE_IDENTIFIER="$bundle" BUNDLE_ID="$bundle"

built="$dir/build/DerivedData/Build/Products/$config-iphoneos/$app"
# devicectl also lists booted simulators as connected, so the device is never guessed.
device="${DEVICE:?set DEVICE to the identifier shown by: xcrun devicectl list devices}"
xcrun devicectl device install app --device "$device" "$built"
xcrun devicectl device process launch --device "$device" "$bundle"
