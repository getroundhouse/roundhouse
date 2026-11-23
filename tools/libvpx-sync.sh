#!/usr/bin/env bash
# Makes third_party/libvpx for the libvpx release pinned below: the commit the `video` crate's build script fetches
# and compiles its VP8 and VP9 decoders from, a folder of config headers for each target the app is built for, which
# libvpx's own configure writes, as Chromium's copy of libvpx is made, and libvpx's licenses. So building the app runs
# no configure, make or perl. `third_party/README.md` says when to run it.
#
# Every target's config is written on one Linux machine, with `--enable-external-build`, which skips configure's tests
# of the compiler, so no target needs its own toolchain. It needs bash, git, make and perl, which Rust's image has, so
# from a Mac it runs in that, as docs/reference/commands.md says. Configure takes x86_64 Linux for its x32 ABI there,
# since it tests nothing, but that reaches only its makefiles' flags, which aren't kept.
set -euo pipefail
cd "$(dirname "$0")/.."

# renovate: datasource=github-tags depName=webmproject/libvpx
LIBVPX_VERSION=v1.17.0
# The commit LIBVPX_VERSION tags. A tag can be moved, so the copy is made only from the commit it was checked to be.
LIBVPX_COMMIT=6df3ec34557879fff673706f4a1d9fbd0f3a6f0e

# The decoders alone, without what a player doesn't use: libvpx's tools, its tests and its post-processing, and VP9's
# profiles of more than 8 bits, which nothing records for a screen. `--size-limit` makes the decoder refuse a frame
# bigger than 16384 pixels either way, as Chromium's does, before it allocates for one.
OPTIONS=(
  --enable-external-build
  --disable-vp8-encoder --disable-vp9-encoder
  --disable-examples --disable-tools --disable-docs --disable-unit-tests
  --disable-install-docs --disable-install-bins
  --disable-libyuv --disable-webm-io
  --disable-postproc --disable-vp9-postproc --disable-temporal-denoising
  --disable-vp9-highbitdepth
  --enable-pic
  --size-limit=16384x16384
)

# Each folder of `config/`, and the target configure writes it for. The Mac's serves iOS too, whose arm64 has the same
# extensions. x86's name yasm only because configure's external build otherwise tests nasm without running it, and
# fails, as Chromium's script found: the build script assembles with nasm. Windows on arm64 leaves out SVE, which
# MSVC's compiler doesn't have.
TARGETS=(
  "arm64-darwin --target=arm64-darwin23-gcc"
  "arm64-linux --target=arm64-linux-gcc"
  "x64-linux --target=x86_64-linux-gcc --as=yasm"
  "x64-windows --target=x86_64-win64-vs17 --as=yasm"
  "arm64-windows --target=arm64-win64-vs17 --disable-sve --disable-sve2"
)

out="$PWD/third_party/libvpx"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

git -c init.defaultBranch=main init -q "$work/libvpx"
git -C "$work/libvpx" fetch -q --depth 1 https://github.com/webmproject/libvpx.git "refs/tags/$LIBVPX_VERSION:refs/tags/$LIBVPX_VERSION"
found="$(git -C "$work/libvpx" rev-parse "$LIBVPX_VERSION^{commit}")"
if [ "$found" != "$LIBVPX_COMMIT" ]; then
  echo "error: $LIBVPX_VERSION is $found, not $LIBVPX_COMMIT. Check the release, then set LIBVPX_COMMIT." >&2
  exit 1
fi
git -C "$work/libvpx" -c advice.detachedHead=false checkout -q "$LIBVPX_VERSION"
source="$work/libvpx"

rm -rf "$out"
mkdir -p "$out/config"
for target in "${TARGETS[@]}"; do
  read -r name arguments <<<"$target"
  build="$work/build/$name"
  mkdir -p "$build"
  # shellcheck disable=SC2086 # Each target's arguments are words.
  (cd "$build" && "$source/configure" $arguments "${OPTIONS[@]}" >configure.log 2>&1) || {
    cat "$build/configure.log" >&2
    exit 1
  }
  make -s -C "$build" libvpx_srcs.txt target=libs >/dev/null
  make -s -C "$build" vpx_config.asm vpx_version.h vp8_rtcd.h vp9_rtcd.h vpx_dsp_rtcd.h vpx_scale_rtcd.h >/dev/null
  mkdir -p "$out/config/$name"
  cp "$build"/{vpx_config.h,vpx_config.c,vpx_config.asm,vp8_rtcd.h,vp9_rtcd.h,vpx_dsp_rtcd.h,vpx_scale_rtcd.h} \
    "$out/config/$name/"
  # What's compiled, which the build script reads: the C and assembly libvpx's own build would, but for the config's
  # own file, which is in the config's folder.
  grep -E '\.(c|asm)$' "$build/libvpx_srcs.txt" | grep -vx 'vpx_config.c' | LC_ALL=C sort >"$out/config/$name/sources.txt"
done
# The same in every build, since it says only the release.
cp "$work/build/arm64-linux/vpx_version.h" "$out/config/"

echo "$LIBVPX_COMMIT" >"$out/COMMIT"

# libvpx's license, its grant of patent rights, and who wrote it, which the license asks to keep with the code, and the
# license of x264's x86inc, which libvpx's x86 assembly includes. They ship with the app, and the policy test checks
# them, so they're kept here, though the code is fetched.
for file in LICENSE PATENTS AUTHORS; do cp "$source/$file" "$out/$file"; done
cp "$source/third_party/x86inc/LICENSE" "$out/LICENSE-X86INC"

# Each file's SHA-256, which the policy test checks the folder against, so a change to it that the script didn't make
# can't merge.
(cd "$out" && find . -type f | sed 's#^\./##' | LC_ALL=C sort | xargs sha256sum >"$work/SHA256SUMS")
mv "$work/SHA256SUMS" "$out/SHA256SUMS"

echo "third_party/libvpx is libvpx $LIBVPX_VERSION, at $LIBVPX_COMMIT, with ${#TARGETS[@]} targets' configs."
