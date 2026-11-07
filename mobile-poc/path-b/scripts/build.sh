#!/usr/bin/env bash
# Builds path B: the Rust static library, then the Swift host that links it.
#   scripts/build.sh sim    [--release]    ad-hoc signed bundle for the simulator
#   scripts/build.sh device [--release]    unsigned bundle for aarch64-apple-ios (see ../README.md)
set -euo pipefail

mode="${1:-sim}"; shift || true
config=Debug; cargo_args=()
if [[ "${1:-}" == "--release" ]]; then config=Release; cargo_args+=(--release); fi
case "$mode" in
  sim)    target=aarch64-apple-ios-sim; sdk=iphonesimulator ;;
  device) target=aarch64-apple-ios;     sdk=iphoneos ;;
  *) echo "usage: $0 sim|device [--release]" >&2; exit 2 ;;
esac

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
../scripts/prepare-patches.sh
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-6}" IPHONEOS_DEPLOYMENT_TARGET=16.0 \
  nice -n 10 cargo build --target "$target" ${cargo_args[@]+"${cargo_args[@]}"}

cd ios
xcodegen generate --quiet
signing=()
[[ "$mode" == device ]] && signing=(CODE_SIGNING_ALLOWED=NO)
nice -n 10 xcodebuild build -quiet \
  -project GpuiKitPocB.xcodeproj -scheme GpuiKitPocB -configuration "$config" \
  -sdk "$sdk" -derivedDataPath "$root/build/DerivedData" -jobs 6 \
  ${signing[@]+"${signing[@]}"}
echo "$root/build/DerivedData/Build/Products/$config-$sdk/GpuiKitPocB.app"
