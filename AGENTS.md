# Trestle

An open source app for running many coding agents in parallel, each in its own workspace. Trestle is a working name, and #8 tracks clearing it before the project goes public.

## Docs

The repo is the source of truth for how things work and why. GitHub tracks what's left: issues for questions, risks, gaps and tasks, and milestones for the build order. Start with `README.md`.

| Doc | Its one purpose |
|---|---|
| `README.md` | What this project is, why it exists, and how to try it |
| `docs/architecture.md` | How the system fits together: parts, repo layout, dependency rules, state ownership, wire format and providers |
| `docs/foundations.md` | Each recurring problem and its one mechanism. Check it before solving one |
| `docs/terms.md` | What the project's words mean, and which to write |
| `docs/rust.md` | How Rust is written here. Read it before writing Rust. Agreed, and not enforced yet |
| `docs/decisions.md` | What's been decided, and why |
| `docs/inspiration.md` | What we've learned from Conductor, the app that inspired this one |
| `docs/usage.md` | Aggregate measurements of what parallel agent work looks like |
| `docs/claude-cli.md` | How the Claude CLI behaves, and how the worker drives it |
| `docs/ui.md` | Why GPUI and gpui-kit, and the evidence behind that |
| `docs/testing.md` | What's been tested, and what that found |
| `docs/performance.md` | The desktop app's speed: the bars, how to measure against them, and what's known about hitting them. Read it before running a timing test |
| `docs/manual-checks.md` | What a person has to judge by hand |
| `mobile-poc/PLAN.md` | Mobile proof of concept plan and decision rules |
| `mobile-poc/README.md` | Mobile proof of concept results so far |
| `mobile-poc/test-server/README.md` | The simulated hub for testing clients on a phone |

## Building and testing

```sh
cargo test                        # The backend, with a fake agent. Needs no GPUI, so it runs on Linux too
cargo test --workspace            # Also the transcript and the desktop app's UI tests
cargo run -p desktop --bin trestle-desktop   # The app
cargo run -p desktop --example look -- <workspace> diff   # The app, opened on a view, for screenshots
tools/smoke/real-claude.sh        # The backend against the real CLI. About $0.42, mostly a plan and one reply on Sonnet, so run it by hand
tools/smoke/capture_claude_fixture.py   # After a CLI upgrade: re-capture its output, then run `cargo test -p agents`
docker run --rm -v "$PWD":/src:ro -e CARGO_TARGET_DIR=/tmp/t -w /src rust:1.98 sh -c 'git config --global user.name t; git config --global user.email t@example.com; cargo test --locked'   # Linux
```

Tests that can't fail prove nothing. When a behavior matters, break it on purpose once and confirm a test notices.

In `use gpui_kit::*` files, test modules must import names explicitly: the glob brings in GPUI's `test` attribute, and `#[test]` then recurses.

## Rules

- **Keep project knowledge in repo docs.** Don't use agent memory, and don't leave decisions only in chat. When a decision or a fact changes, update the relevant doc as part of the same work. Decisions go in `docs/decisions.md`. What's still open or left to do goes in a GitHub issue, not a doc.
- **Each doc has one clear purpose.** If something else needs writing down, it gets its own doc rather than a new section in one that's nearby.
- **Commit only when the user asks.**
- **Report only what you've verified.** Say what's unverified. "Running" only means an agent hasn't exited, so check processes and file timestamps before describing what it's doing.
- **Check primary sources for licenses.** Read `Cargo.toml` and the license terms, not GitHub's label, which reported `gpui-mobile` as AGPL when it's tri-licensed with Apache-2.0.
- **Keep shipped code free of copyleft.** Apache-2.0, MIT and similar only. Zed's GPL-3.0 crates are out, and only GPUI's Apache-2.0 crates are in.
- **Conductor is an inspiration, not a spec.** This is a new product, so design and describe things on their own terms, and don't call it a clone or a replacement. Don't copy Conductor's code or assets, or use its name. Reading its settings and variables is a compatibility feature, nothing more.
- **Keep client data and credentials out of the repo.** Fixtures are synthetic, and docs use aggregate numbers only.
