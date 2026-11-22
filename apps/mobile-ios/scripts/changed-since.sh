#!/usr/bin/env bash
# Says whether what the iPhone app is built from differs between COMMIT and this checkout's HEAD: the crates it builds
# for a phone, as HEAD's Cargo.lock has them, its Xcode project, and Cargo's settings. Exits 0 if it does and 1 if not,
# so the TestFlight workflow uploads only a build that's new. Needs the history back to COMMIT, and the crates fetched,
# as building does.
#
#   apps/mobile-ios/scripts/changed-since.sh COMMIT
set -euo pipefail

commit="${1:?give the commit to compare with}"
repo="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$repo"

# The workspace's own crates and copies the app builds for a phone, as paths from here. A crate it stopped building
# changed its Cargo.toml or Cargo.lock, which are below. The app's own crate holds its Xcode project.
crates=()
while IFS= read -r dir; do
  crates+=("${dir#"$repo"/}")
done < <(
  cargo tree --locked -p mobile-ios --target aarch64-apple-ios -e normal,build --prefix none -f '{p}' |
    sed -n 's/.*(\(\/[^)]*\)).*/\1/p' | sort -u
)
[ ${#crates[@]} -gt 0 ] || { echo "error: cargo tree listed none of the app's crates" >&2; exit 2; }

if git diff --quiet "$commit" HEAD -- "${crates[@]}" Cargo.lock Cargo.toml rust-toolchain.toml .cargo; then
  echo "Nothing the iPhone app is built from changed since $commit"
  exit 1
fi
echo "What the iPhone app is built from changed since $commit"
