# Commands

Every command for building, testing, running and packaging Roundhouse from a checkout. `AGENTS.md` has the few every session uses, and `docs/standards/merging.md` says which checks a pull request needs.

Building needs Zig 0.17 on the PATH, which builds Ghostty's terminal core: `brew install zig` on a Mac. Building for x86, on an Intel or AMD machine or with `cargo xwin` below, needs nasm on the PATH too, which assembles libvpx's fast paths for the `video` crate, and the build fails without it: `brew install nasm` on a Mac, `apt install nasm` on Ubuntu, and on Windows the `win64` zip from nasm's releases at nasm.us, as `.github/actions/nasm` installs it.

```sh
cargo test                        # The backend, with a fake agent. Needs no GPUI, so it runs on Linux too
cargo test --workspace            # Also the transcript and the desktop app's UI tests
cargo run -p desktop --bin roundhouse-desktop -- --note "What to try"   # The app. Its sidebar and title name this checkout and say the note
cargo run -p desktop --example look -- <workspace> diff --note "What it shows"   # The app, opened on a view, for screenshots
cargo test -p desktop --test ui gallery   # Every screen, from made-up data, against its image in docs/spec/screens, and every size of both styles of the app icon. No hub, no window
ROUNDHOUSE_UPDATE_SCREENS=1 cargo test -p desktop --test ui gallery   # Accepts how they look now. A confirmed screen needs the owner
cargo test -p desktop --test ui gallery::clips:: -- --ignored   # Records every clip, a short video of the app from made-up data, a frame at a time, into .context/clips/, as the gallery's clips/mod.rs says. Add the end of a clip's test's name to record only it. On a Mac, and never in CI
cargo test -p mobile-ios --test ui gallery   # Every screen of the iPhone app, and the handheld's components, against its image in docs/spec/screens/phone. Accepted the same way
cargo run -p guide -- OUT         # Builds docs/guide as the site's /docs/ pages in OUT, which mustn't exist, and refuses a link or a picture that isn't there. Run it after changing a page, as tools/guide/README.md says, since CI does
cargo run -p design               # Writes design/Index.bl, every canvas of the design with its status, from the badge at each one's top. Run it after adding a canvas or changing a badge, and after a merge, since a test refuses an index that's behind
cargo run -p spec-page            # The spec as a page in .context/spec/, each area's screens beside its rules, and which are confirmed, and the design system's page, its foundations to look one up and then its components. Open the paths it prints
tools/install.sh                  # Installs this commit as the app to use every day, outside target/. Run it by hand
git rev-list --reverse main | sed -n 50p   # The commit a published version was built from, here 0.1.50's, which is the 50th on main
tools/package/package.sh          # Packages this commit as Roundhouse.app, unsigned, in target/package. Needs the .NET SDK
tools/package/package-windows.sh --prebuilt DIR   # Packages Windows binaries, built on Windows by tools/build.sh, as the installer and its updates, unsigned. On a Mac or Linux, with the .NET SDK. `--arch arm64` for ARM64 ones
tools/package/package-linux.sh --prebuilt DIR     # Packages Linux binaries, built in Ubuntu 22.04 by tools/build.sh, as the AppImage and its updates. On Linux, with the .NET SDK and squashfs-tools. `--arch arm64` for ARM64 ones
python3 -m unittest discover -s tools/package   # The scripts that publish releases, against a directory for the bucket, and the one that makes the Windows icon
cargo run -p iphone -- install    # Builds the iPhone app, signs it for development, and installs and opens it on a phone. `-- upload` uploads it to TestFlight and waits for Apple to process it, as `apps/mobile-ios/README.md` says
cargo run -p kit-sync             # Brings third_party/gpui-kit/vendor and its patches in third_party/gpui-kit/patches to the release it pins. `-- patch <name>` records a change, and `-- drop <name>` takes one out, as `third_party/README.md` says
docker run --rm -v "$PWD":/src -w /src rust:1.100.0 tools/libvpx-sync.sh   # Makes third_party/libvpx again for the libvpx release it pins: the commit the build fetches, each target's config and the licenses, as third_party/README.md says
tools/smoke/real-claude.sh        # The backend against the real CLI. About $0.42, mostly a plan and one reply on Sonnet, so run it by hand
tools/smoke/capture_claude_fixture.py   # After a CLI upgrade: re-capture its output, then run `cargo test -p agents`
cargo run -p agents --example conformance   # The conformance scenarios, through the adapter, on the CLI the hub would find, against a scripted model, on a Mac. `--paid` adds the real one, under a cent a scenario on Haiku, so run it by hand. `--save` keeps the fixtures, as docs/reference/conformance.md says
cargo test -p dictation-eval      # Scores which words each field that can be dictated into hints, for the made-up cases in tools/dictation-eval/cases/, against tools/dictation-eval/baseline.json
ROUNDHOUSE_UPDATE_EVAL=1 cargo test -p dictation-eval   # Accepts the scores now as the baseline, which a change to which words are hinted does, so its diff shows the cases that gained and lost
cargo run -p speech-eval --release -- run --out DIR   # Hears 300 Tech-Sentences recordings, 100 of LibriSpeech's and 96 sound-alikes with dictation's model on a Mac, with no hints, the terms said, and those among others, into DIR. About 10 minutes on an M4 Max, and it goes on where it stopped. `--clips N` hears N of each set, `--all` every clip, about an hour and a half, `--boost` tries another boost, `--model` takes the model's files from a folder. It downloads the datasets, and the model unless given it, into ~/Library/Caches/roundhouse-speech-eval
cargo run -p speech-eval --release -- compare DIR   # Sets the run in DIR against tools/speech-eval/baseline.json, in Markdown, for the PR of a change to dictation. `accept DIR` makes it the baseline
cargo run -p speech-eval --release -- hear FILE...   # Hears WAV or FLAC files through dictation with its model on a Mac, and prints each one's live and final text. `--engine-only` hears each with the engine alone, live and then whole, rather than cut at its pauses, `--hint` adds a hint word and `--language en` chooses the language
cargo run -p speech-eval --release -- kept hear RECORDINGS --out DIR   # Hears dictations someone chose to keep, in the `recordings` folder of the app's data directory, with dictation's model on a Mac, each with the hint words, boost and language it was heard with. `--boost` tries another boost, `--no-hints` none, and `--todays-words HUB_DB` the words today's picker gives each, from what its own were picked from and the chat in a copy of the hub's database, `hub.db` in its data directory. It goes on where it stopped. DIR is outside the repository, since it has what they said
cargo run -p speech-eval --release -- kept compare DIR [OTHER]   # Lists the kept recordings that the run in DIR wrote differently from the one in OTHER, or from what the app wrote. Nothing it lists goes in a PR but its totals
cargo run -p speech-eval --release -- kept sent RECORDINGS   # Sets what was sent against the kept recordings it came from, and lists those whose text was changed or left out, with how. It needs no model, and nothing it lists goes in a PR but its totals
cargo run -p phonon --release --example say -- DIR "SENTENCE"   # Says it through the speakers with Phonon, from its files in DIR, as `phonon::MODEL` lists them, downloading the 65 MB first if they aren't there, and how fast it was. CI never runs it
cargo run -p speaking --release --example read -- DIR "TEXT" [--stop-after SECONDS]   # Reads it out a phrase at a time through the speakers, and prints how far it had been heard as each phrase played, and where a stop cut it. CI never runs it
cargo run -p conversation --release --example voice -- DIR "TEXT"   # Fetches Phonon's files into DIR if they aren't there, as conversation mode does the first time, then reads the text out through the join client-core reads with, and prints each update. CI never runs it
cargo xwin clippy --workspace --locked --target x86_64-pc-windows-msvc -- -D warnings   # Windows, from a Mac: lints the product's code as built for it, and runs nothing. Needs cargo-xwin, LLVM and the rustup target
docker run --rm -v "$PWD":/src:ro -e CARGO_TARGET_DIR=/tmp/t -w /src rust:1.100.0 sh -c 'v=0.17.1; z=zig-$(uname -m)-linux-$v; curl -sSfL https://ziglang.org/download/$v/$z.tar.xz | tar -xJ -C /opt && export PATH=/opt/$z:$PATH; apt-get update -qq && apt-get install -y -qq lld nasm; git config --global user.name t; git config --global user.email t@example.com; cargo test --locked'   # Linux, with the Zig that builds Ghostty's terminal core, the lld that links on ARM64, and the nasm x86 builds need
```

## A checkout's own data

Roundhouse can be developed from inside Roundhouse, so nothing run from a checkout can reach the installed app's hub. `cargo run` and `cargo test` use this checkout's own hub, in `.context/dev`, through `.cargo/config.toml`. A debug binary run directly uses `roundhouse-dev`, and only a release build uses the real data, which is what `tools/install.sh` installs. Set `ROUNDHOUSE_HOME` and `ROUNDHOUSE_WORKSPACES` to choose another.

This checkout's tools write to five folders of `.context/`: `dev`, for its own hub's data and worktrees, `gallery`, for the screens and icons as the gallery last drew them, `clips`, for the clips as they were last recorded, `spec`, for the spec's page, and `site`, for the site as `tools/guide/README.md` builds it. Leave them to those tools: put nothing of your own in them, and don't move or delete what's in them.
