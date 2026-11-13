# Roundhouse

An open source app for running many coding agents in parallel, each in its own workspace. Roundhouse is a working name.

## Docs

Start with `README.md`. `docs/documentation.md` lists every doc, and says what goes in a doc and what goes on GitHub, so read it before writing either. Read "State ownership" in `docs/architecture.md` before deciding where a change goes, `docs/rust.md` before writing Rust, `docs/gpui.md` before writing UI code, `docs/performance.md` before running a timing test, `docs/reviews.md` before reviewing a change or having one reviewed, and `docs/pull-requests.md` before opening a PR or writing its commit.

## Building and testing

Building needs Zig 0.17 on the PATH, which builds Ghostty's terminal core: `brew install zig` on a Mac.

```sh
cargo test                        # The backend, with a fake agent. Needs no GPUI, so it runs on Linux too
cargo test --workspace            # Also the transcript and the desktop app's UI tests
cargo run -p desktop --bin roundhouse-desktop -- --note "What to try"   # The app. Its sidebar and title name this checkout and say the note
cargo run -p desktop --example look -- <workspace> diff --note "What it shows"   # The app, opened on a view, for screenshots
cargo test -p desktop --test ui gallery   # Every screen, from made-up data, against its image in docs/spec/screens, and every size of both styles of the app icon. No hub, no window
ROUNDHOUSE_UPDATE_SCREENS=1 cargo test -p desktop --test ui gallery   # Accepts how they look now. A confirmed screen needs the owner
cargo run -p guide -- OUT         # Builds docs/guide as the site's /docs/ pages in OUT, which mustn't exist, and refuses a link or a picture that isn't there. Run it after changing a page, as tools/guide/README.md says, since CI does
cargo run -p spec-page            # The spec as a page in .context/spec/, each area's screens beside its rules, and which are confirmed. Open the path it prints
tools/install.sh                  # Installs this commit as the app to use every day, outside target/. Run it by hand
tools/package/package.sh          # Packages this commit as Roundhouse.app, unsigned, in target/package. Needs the .NET SDK
tools/package/package-windows.sh --prebuilt DIR   # Packages Windows binaries, built on Windows by tools/build.sh, as the installer and its updates, unsigned. On a Mac or Linux, with the .NET SDK. `--arch arm64` for ARM64 ones
tools/package/package-linux.sh --prebuilt DIR     # Packages Linux binaries, built in Ubuntu 22.04 by tools/build.sh, as the AppImage and its updates. On Linux, with the .NET SDK and squashfs-tools. `--arch arm64` for ARM64 ones
python3 -m unittest discover -s tools/package   # The scripts that publish releases, against a directory for the bucket, and the one that makes the Windows icon
tools/smoke/real-claude.sh        # The backend against the real CLI. About $0.42, mostly a plan and one reply on Sonnet, so run it by hand
tools/smoke/capture_claude_fixture.py   # After a CLI upgrade: re-capture its output, then run `cargo test -p agents`
cargo xwin clippy --workspace --locked --target x86_64-pc-windows-msvc -- -D warnings   # Windows, from a Mac: lints the product's code as built for it, and runs nothing. Needs cargo-xwin, LLVM and the rustup target
docker run --rm -v "$PWD":/src:ro -e CARGO_TARGET_DIR=/tmp/t -w /src rust:1.100.0 sh -c 'v=0.17.1; z=zig-$(uname -m)-linux-$v; curl -sSfL https://ziglang.org/download/$v/$z.tar.xz | tar -xJ -C /opt && export PATH=/opt/$z:$PATH; git config --global user.name t; git config --global user.email t@example.com; cargo test --locked'   # Linux, with the Zig that builds Ghostty's terminal core
```

Whenever you open the app, say why with `--note`, even if nobody is meant to look at it, since copies from several checkouts can be open at once. As it quits, it says why on stderr. "Quit by whoever had it open" means they've finished with it, so don't look into why it exited, and don't open it again unless they ask.

Roundhouse can be developed from inside Roundhouse, so nothing run from a checkout can reach the installed app's hub. `cargo run` and `cargo test` use this checkout's own hub, in `.context/dev`, through `.cargo/config.toml`. A debug binary run directly uses `roundhouse-dev`, and only a release build uses the real data, which is what `tools/install.sh` installs. Set `ROUNDHOUSE_HOME` and `ROUNDHOUSE_WORKSPACES` to choose another.

This checkout's tools write to four folders of `.context/`: `dev`, for its own hub's data and worktrees, `gallery`, for the screens and icons as the gallery last drew them, `spec`, for the spec's page, and `site`, for the site as `tools/guide/README.md` builds it. Leave them to those tools: put nothing of your own in them, and don't move or delete what's in them.

Tests that can't fail prove nothing. When a behavior matters, break it on purpose once and confirm a test notices. Undo the break with an edit, not by copying a saved file back: some `cp`s, such as the one in Homebrew's `uutils-coreutils`, keep the saved file's older modification time, so cargo doesn't rebuild and goes on testing the broken code.

Run the full checks once per PR that changes code, just before pushing. A PR that only changes Markdown or the marketing site, which is `site/` and `tools/site/`, needs none of them, since none of them reads either. Warnings count as failures. Clippy runs twice: on every target, and then on the product's code alone, as it ships, since the `testing` features that the tests turn on can hide an import or an item only the tests use. CI runs them too, on Linux for every PR that changes more than those, and on a Mac only when dependencies change, so it doesn't replace running them here. Windows has a workflow of its own, which lints the product's code as built for it for every PR that changes Rust, builds the hub and the CLI, and starts and stops a hub there. It doesn't block a merge, and it's the only place anything runs on Windows. On Linux, `--workspace` needs the packages `.github/workflows/ci.yml` installs for GPUI, and the gallery doesn't run:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --locked -- -D warnings
cargo test --workspace --locked
```

PRs merge through GitHub's merge queue, squashed, once CI's `checks` job passes on the PR merged onto `main` and whatever is ahead of it in the queue, so only each PR's final state reaches `main`. Add a PR to the queue only when the owner asks, but for a fix to a flaky or broken test, below. GitHub takes no push to a queued PR's branch, so to change one, take it out of the queue with GraphQL's `dequeuePullRequest`, which also turns off its auto-merge, then push, and queue it again once its CI passes. Between commits, `cargo check` and the tests for what changed are enough. Once the full checks have passed, a rebase onto `main` that applied cleanly needs only fmt, clippy, `cargo test -p policy` and the gallery before pushing, since the queue runs the tests on the PR merged onto `main`, and `main` often moves while the full checks run. Clippy builds every target, which catches what a clean rebase can still break, and the gallery runs nowhere else. A rebase whose conflicts were resolved by hand needs the full checks again.

For a flaky test, repeat it only enough to see how often it fails, find the cause by reading the code, then repeat it only enough to confirm the fix. Put the fix in a PR of its own from `main`, and when it changes only tests and the code that supports them, add it to the queue yourself once its CI passes, and say you did. Until it merges, the test fails PRs that didn't break it and throws them out of the queue, so it shouldn't wait for the owner to notice it. Making a test wait for the right thing is a fix. Skipping it, deleting it or loosening what it checks isn't, and waits for the owner like any other change.

## Rules

- **Put what you learn and decide where `docs/documentation.md` says,** as part of the same work. Not in agent memory, and not only in chat.
- **A change to what the product does changes what's written about it, in the same PR:** its rule in `docs/spec/`, the pages of `docs/guide/` that say it, and the README and the site where they say it too. A test fails when a screen no longer matches its picture, and nothing fails when a guide page goes stale, so search the guide for what you changed, and build it, as `tools/guide/README.md` says.
- **Owner docs, and the files that enforce the standards, change only with the owner's approval, but propose a change whenever you see a better way.** `docs/documentation.md` says which they are, how to propose, and how an agent's choice gets in before the owner confirms it.
- **Decide who owns the state before choosing where a change goes.** A bug in what a client shows often comes from a client keeping or working out what the hub should record, and the fix goes to the owner, with a protocol change if it needs one, even when fixing the client would be smaller. When the cause is an *Unconfirmed* design, question the design, not just the bug.
- **Leave the code the way you'd want to find it.** Agents copy what they find here, so a workaround, of the kinds under "Workarounds" in `docs/reviews.md`, becomes the pattern the next change follows. Restructure what's in the way rather than working around it, in a PR of its own at the bottom of a stack when it stands alone, and give any workaround you keep the reason `docs/reviews.md` asks for. Don't copy a workaround you find: fix it when it's part of the change, and otherwise give it an issue and list it under **Left open:**.
- **Have every change reviewed by agents that didn't build it,** as `docs/reviews.md` says, before the full checks and the PR, without being asked to.
- **Commit only when the user asks.**
- **Use GitHub's stacked PRs when changes build on each other.** If a change depends on one that isn't merged yet, or a large change splits into layers that can each be reviewed alone, make them a stack with `gh stack` rather than one big PR. A feature that reaches from the protocol through the hub and client-core to a client is one: a PR for each layer, from the bottom, so each can merge while the ones above it are reviewed, rather than one branch meeting everything that lands on `main`. Independent changes get their own PR from `main`.
- **Report only what you've verified.** Say what's unverified. "Running" only means an agent hasn't exited, so check processes and file timestamps before describing what it's doing.
- **Check primary sources for licenses.** Read `Cargo.toml` and the license terms, not GitHub's label, which reported `gpui-mobile` as AGPL when it's tri-licensed with Apache-2.0.
- **Keep strong copyleft out of shipped code.** No GPL, AGPL or LGPL, so Zed's GPL-3.0 crates are out and only GPUI's Apache-2.0 crates are in. `docs/architecture.md` has why, and the one exception, and `tools/policy` checks it.
- **Give Renovate every new dependency.** A crate goes in `[workspace.dependencies]` at its full version, as `docs/rust.md` says. A version pinned anywhere else, such as a tool a workflow installs, gets a `# renovate:` comment on the line above it. If Renovate can't read it, or can't finish its update alone, as with a checksum, add a custom manager or a rule to `.github/renovate.json5` in the same PR.
- **Conductor is an inspiration, not a spec.** This is a new product, so describe it on its own terms, and don't call it a clone or a replacement. Its look is a starting point for ours, so it feels familiar. Don't copy its code, icons or other assets, or use its name. Reading its settings and variables is a compatibility feature, nothing more.
- **Keep client data and credentials out of the repo.** Fixtures are synthetic, and docs use aggregate numbers only.
