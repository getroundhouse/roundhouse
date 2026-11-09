#!/usr/bin/env bash
# Materializes patched sources under patches/build/ from upstream plus the diffs in patches/.
# Only the diffs are kept in the repository, so the surface we maintain stays visible.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/patches/build"
mkdir -p "$out"

# Path A: gpui-base 0.9.0 from crates.io, plus one method.
base="$out/gpui-base-0.9.0"
if [[ ! -f "$base/.patched" ]]; then
  rm -rf "$base"
  tarball="$out/gpui-base-0.9.0.crate"
  curl -fsSL https://static.crates.io/crates/gpui-base/gpui-base-0.9.0.crate -o "$tarball"
  echo "cff6377e30f0c857681662e7edc7286bf30f5606033390401b0706d7bd930833  $tarball" | shasum -a 256 -c - >/dev/null
  tar -xzf "$tarball" -C "$out"
  (cd "$base" && patch -p0 --no-backup-if-mismatch < "$root/patches/gpui-base-0.9.0-text-length.patch")
  touch "$base/.patched"
fi

# Path A: gpui-pre-ios 0.3.11 from crates.io, asking its display link for 120 Hz.
ios="$out/gpui-pre-ios-0.3.11"
if [[ ! -f "$ios/.patched" ]]; then
  rm -rf "$ios"
  tarball="$out/gpui-pre-ios-0.3.11.crate"
  curl -fsSL https://static.crates.io/crates/gpui-pre-ios/gpui-pre-ios-0.3.11.crate -o "$tarball"
  echo "326382c3b3f004617ac0f61338594c00f053b08b7875f3486ea0f221e9b20a75  $tarball" | shasum -a 256 -c - >/dev/null
  tar -xzf "$tarball" -C "$out"
  (cd "$ios" && patch -p0 --no-backup-if-mismatch < "$root/patches/gpui-pre-ios-0.3.11-frame-rate.patch")
  touch "$ios/.patched"
fi

# Path B: Longbridge's fork at a pinned commit, bumped to GPUI 0.3.11.
fork="$out/gpui-mobile"
rev=7548ebc
if [[ ! -f "$fork/.patched" ]]; then
  rm -rf "$fork"
  clone="$(mktemp -d)"
  git clone --quiet --filter=blob:none --no-checkout https://github.com/longbridge/gpui-mobile "$clone"
  mkdir -p "$fork"
  git -C "$clone" archive "$rev" -- Cargo.toml LICENSE-AGPL LICENSE-APACHE LICENSE-GPL README.md src vendor tests | tar -x -C "$fork"
  rm -rf "$clone"
  (cd "$fork" && patch -p1 --no-backup-if-mismatch < "$root/patches/gpui-mobile-0.3.11.patch")
  touch "$fork/.patched"
fi
