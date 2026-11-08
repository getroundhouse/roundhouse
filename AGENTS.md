# Trestle

An open source app for running many coding agents in parallel, each in its own workspace. Trestle is a working name.

## Docs

Start with `README.md`. `docs/documentation.md` lists every doc, and says what goes in a doc and what goes on GitHub, so read it before writing either. Read `docs/rust.md` before writing Rust, `docs/gpui.md` before writing UI code, and `docs/performance.md` before running a timing test.

## Building and testing

```sh
cargo test                        # The backend, with a fake agent. Needs no GPUI, so it runs on Linux too
cargo test --workspace            # Also the transcript and the desktop app's UI tests
cargo run -p desktop --bin trestle-desktop   # The app
cargo run -p desktop --example look -- <workspace> diff   # The app, opened on a view, for screenshots
cargo test -p desktop --test ui gallery   # Every screen, from made-up data, to images in .context/gallery. No hub, no window
tools/install.sh                  # Installs this commit as the app to use every day, outside target/. Run it by hand
tools/smoke/real-claude.sh        # The backend against the real CLI. About $0.42, mostly a plan and one reply on Sonnet, so run it by hand
tools/smoke/capture_claude_fixture.py   # After a CLI upgrade: re-capture its output, then run `cargo test -p agents`
docker run --rm -v "$PWD":/src:ro -e CARGO_TARGET_DIR=/tmp/t -w /src rust:1.98.1 sh -c 'git config --global user.name t; git config --global user.email t@example.com; cargo test --locked'   # Linux
```

Trestle can be developed from inside Trestle, so nothing run from a checkout can reach the installed app's hub. `cargo run` and `cargo test` use this checkout's own hub, in `.context/dev`, through `.cargo/config.toml`. A debug binary run directly uses `trestle-dev`, and only a release build uses the real data, which is what `tools/install.sh` installs. Set `TRESTLE_HOME` and `TRESTLE_WORKSPACES` to choose another.

Tests that can't fail prove nothing. When a behavior matters, break it on purpose once and confirm a test notices.

Run the full checks once per PR that changes code, just before pushing. A PR that only changes Markdown needs none of them. Warnings count as failures. On Linux, drop `--workspace`, since the transcript and desktop app need GPUI:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

PRs are squash-merged, so only each PR's final state reaches `main`. Between commits, `cargo check` and the tests for what changed are enough.

For a flaky test, repeat it only enough to see how often it fails, find the cause by reading the code, then repeat it only enough to confirm the fix.

## Rules

- **Put what you learn and decide where `docs/documentation.md` says,** as part of the same work. Not in agent memory, and not only in chat.
- **Owner docs, and the files that enforce the standards, change only with the owner's approval, but propose a change whenever you see a better way.** `docs/documentation.md` says which they are, how to propose, and how an agent's choice gets in before the owner confirms it.
- **Commit only when the user asks.**
- **Use GitHub's stacked PRs when changes build on each other.** If a change depends on one that isn't merged yet, or a large change splits into layers that can each be reviewed alone, make them a stack with `gh stack` rather than one big PR. Independent changes get their own PR from `main`.
- **Report only what you've verified.** Say what's unverified. "Running" only means an agent hasn't exited, so check processes and file timestamps before describing what it's doing.
- **Check primary sources for licenses.** Read `Cargo.toml` and the license terms, not GitHub's label, which reported `gpui-mobile` as AGPL when it's tri-licensed with Apache-2.0.
- **Keep strong copyleft out of shipped code.** No GPL, AGPL or LGPL, so Zed's GPL-3.0 crates are out and only GPUI's Apache-2.0 crates are in. `docs/architecture.md` has why, and `tools/policy` checks it.
- **Conductor is an inspiration, not a spec.** This is a new product, so design and describe things on their own terms, and don't call it a clone or a replacement. Don't copy Conductor's code or assets, or use its name. Reading its settings and variables is a compatibility feature, nothing more.
- **Keep client data and credentials out of the repo.** Fixtures are synthetic, and docs use aggregate numbers only.
