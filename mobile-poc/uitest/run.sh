#!/usr/bin/env bash
# Runs the XCUITest driver against an already installed app on an already booted simulator.
#   uitest/run.sh <simulator id> <bundle id> <output dir> [test name]
# Does not open Simulator.app: xcodebuild test uses the booted, headless device.
set -euo pipefail
dev="$1"; bundle="$2"; out="$3"; only="${4:-}"
cd "$(dirname "$0")"
xcodegen generate --quiet
args=()
[[ -n "$only" ]] && args+=(-only-testing:"DriveTests/DriveTests/$only")
TEST_RUNNER_POC_BUNDLE_ID="$bundle" TEST_RUNNER_POC_OUT="$out" \
  nice -n 10 xcodebuild test -quiet \
    -project PocDriver.xcodeproj -scheme PocDriver \
    -destination "platform=iOS Simulator,id=$dev" \
    -derivedDataPath build/DerivedData \
    -jobs 6 \
    ${args[@]+"${args[@]}"}
