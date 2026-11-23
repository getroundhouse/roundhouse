# The videos agents make are WebM, decoded by libvpx built from its source at a pinned commit

**Status:** Decided.
**First recorded:** 2026-11-21, in #2341.

The app plays one kind of video, the kind agents are told to make: WebM, with VP8 or VP9 and no sound, which is what Playwright records. It decodes it with libvpx, the same way on macOS, iOS, Windows and Linux. The build fetches libvpx's source with git, at the commit `third_party/libvpx` pins, as it fetches Ghostty's, and compiles it with the configs kept there. Any other video, such as an MP4 or a MOV the person has, opens in the system's own player.

On x86, libvpx's fast paths are written for nasm, so building for x86 needs nasm, and fails without it. libvpx's plain C would build without it, but decodes 3 to 5 times slower, and on an older x86 core it couldn't keep up with a 60 fps recording of a Retina screen.

## Chosen over

- **Each platform's own decoders,** AVFoundation on Apple's systems and Media Foundation on Windows. They play H.264 and HEVC in MP4, but not WebM, and Linux has no decoder that every distro installs, so each platform would play different files.
- **AV1, through `rav1d`.** It's in Rust, so the safer parser of a file an agent wrote, and makes the smallest files, but nothing an agent records writes it, so every recording would need converting.
- **H.264 in MP4.** Almost everything records it, but a decoder of it shipped with the app needs its patents licensed, but for Cisco's own build of openh264, which would have to be downloaded at runtime.
- **Animated WebP or APNG,** which GPUI already decodes. It decodes every frame into memory at once, about 3.6 GB for 30 seconds at 1280×800, and can't seek.
- **libvpx's plain C on x86,** which needs no nasm, as above.
- **A copy of libvpx's source in the repo,** 3.7 MB, which builds offline and shows a release's changes as a diff. The build already fetches Ghostty's, and a commit pins each file by its hash.
- **A crate of libvpx from crates.io.** None builds 1.17's decoders from source for every target. `shiguredo_libvpx` downloads prebuilt libraries, with none for Windows on arm64.
- **A repo of our own with the source, as a git dependency,** like `libghostty-vt`. It's a second repo to keep in step with this one.

## What it costs

- **libvpx is C, reading files an agent wrote, which can't be trusted.** It's the decoder Chrome ships and fuzzes, and a new release is taken as `third_party/README.md` says.
- **A video in another format doesn't play in the app,** until an agent converts it, with ffmpeg.
- **nasm is one more thing building for x86 needs,** on a person's machine and in CI's x86 jobs, as `docs/reference/commands.md` says.
- **Building needs git and the network the first time,** and after a clean: 6 MB to download, 33 MB on disk.
- **A new release shows as a commit, not a diff of libvpx's code,** which is read in its own repository.
- **The configs are kept up to date by hand,** with `tools/libvpx-sync.sh`, which writes each target's config with libvpx's own `configure`, and the commit, and keeps libvpx's licenses beside them.
