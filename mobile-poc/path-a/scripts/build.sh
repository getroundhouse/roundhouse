#!/usr/bin/env bash
# Builds path A and packages it as a .app.
#   scripts/build.sh sim    [--release]    ad-hoc signed bundle for the simulator
#   scripts/build.sh device [--release]    unsigned bundle for aarch64-apple-ios (see README.md)
# Polite by default: other work on this Mac is timing-sensitive.
set -euo pipefail

mode="${1:-sim}"; shift || true
profile=debug
cargo_args=()
if [[ "${1:-}" == "--release" ]]; then profile=release; cargo_args+=(--release); fi

case "$mode" in
  sim)    target=aarch64-apple-ios-sim ;;
  device) target=aarch64-apple-ios ;;
  *) echo "usage: $0 sim|device [--release]" >&2; exit 2 ;;
esac

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
../scripts/prepare-patches.sh
bundle_id="${BUNDLE_ID:-dev.gpuikit.iospoc}"
app="$root/build/$mode-$profile/GpuiKitPoc.app"

CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-6}" IPHONEOS_DEPLOYMENT_TARGET=15.0 \
  nice -n 10 cargo build --target "$target" ${cargo_args[@]+"${cargo_args[@]}"}

rm -rf "$app"
mkdir -p "$app"
cp "target/$target/$profile/gpui_kit_ios_poc" "$app/gpui_kit_ios_poc"
sed "s/\$(BUNDLE_ID)/$bundle_id/" app/Info.plist > "$app/Info.plist"

if [[ "$mode" == sim ]]; then
  codesign --force --sign - "$app"
else
  echo "Device bundle is unsigned. Signing needs a team and profile; see README.md."
fi
echo "$app"
