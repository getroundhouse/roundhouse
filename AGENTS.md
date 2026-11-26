# Roundhouse

An open source app for running many coding agents in parallel, each in its own workspace. Roundhouse is a working name.

This file holds what every session needs. `README.md` says what the product is, and `docs/README.md` lists every doc.

## Rules

- **Commit only when the user asks,** and add a PR to the merge queue only when the owner asks.
- **Report only what you've verified,** and say what's unverified. "Running" only means an agent hasn't exited, so check processes and file timestamps before describing what it's doing.
- **Ask the owner before building on a decision that's theirs,** with the agent's question tool: one decision to a question, two to four options with what each costs, and the one you recommend first. `docs/standards/who-decides.md` says which decisions are theirs, and dropping a requirement to make the work simpler is one. Don't ask about anything else, or for leave to carry on.
- **Put what the owner must see where they'll see it.** What needs an answer is a question, which stops the turn and marks the chat in the sidebar. Anything else goes in the turn's last message, never earlier in the turn, since the owner may be in another workspace.
- **Put what you learn and decide in a doc or an issue,** as part of the same work, where `docs/standards/documentation.md` says. Not in agent memory, and not only in chat.
- **Owner docs, and the files that enforce the standards, change only with the owner's approval.** Propose a change whenever you see a better way, as `docs/standards/who-decides.md` says.
- **A change to what the product does changes what's written about it, in the same PR:** its rule in `docs/spec/`, the pages of `docs/guide/` that say it, and the README and the site where they say it too. No test fails when a guide page goes stale, so search the guide for what you changed, and build it.
- **Leave the code the way you'd want to find it.** Agents copy what they find here, so a workaround becomes the pattern the next change follows. Restructure what's in the way, and don't copy a workaround you find. `docs/standards/workarounds.md` says what counts as one.
- **Keep client data, credentials and private matters out of the repo.** Fixtures are synthetic, and docs use aggregate numbers only. `docs/standards/documentation.md` says what's private.
- **Conductor is an inspiration, not a spec.** Describe this product on its own terms. Don't copy Conductor's code, icons or other assets, use its name, or call this a clone or a replacement.

## Running things

Building needs Zig 0.17 on the PATH: `brew install zig` on a Mac.

```sh
cargo test                        # The backend, with a fake agent
cargo test --workspace            # Also the transcript and both apps' UI tests
cargo run -p desktop --bin roundhouse-desktop -- --note "What to try"   # The app
cargo test -p desktop --test ui gallery      # Every screen, against its image in docs/spec/screens
cargo test -p mobile-ios --test ui gallery   # Every screen of the iPhone app, against docs/spec/screens/phone
cargo run -p guide -- OUT         # Builds docs/guide, and refuses a link or a picture that isn't there
```

`docs/reference/commands.md` has every other command, and `docs/standards/merging.md` the checks to run before pushing.

- **Say why with `--note` whenever you open the app,** since copies from several checkouts can be open at once. As it quits, it says why on stderr. "Quit by whoever had it open" means they've finished with it, so don't look into it, and don't open it again unless they ask.
- **Nothing run from a checkout reaches the installed app's hub.** `cargo run` and `cargo test` use this checkout's own, in `.context/dev`. A debug binary run directly uses `roundhouse-dev`, and only a release build uses the real data.
- **Leave `dev`, `gallery`, `clips`, `spec` and `site` in `.context/` to the tools that write them.** Add, move or delete nothing there.

## Read before you…

| Before you… | Read |
|---|---|
| write a doc, an issue, a PR or a comment | `docs/standards/writing.md` and `docs/standards/documentation.md` |
| decide where a change goes | `docs/principles/state.md`, and the rest of `docs/principles/` for the part you're changing |
| write Rust | `docs/standards/rust.md` and `docs/standards/platforms.md` |
| write a test | `docs/standards/testing.md` |
| design or write UI | `docs/standards/design.md`, `docs/standards/gpui.md` and `docs/standards/components.md` |
| change the kit | `third_party/README.md` |
| add a dependency | `docs/standards/dependencies.md` |
| run a timing test | `docs/reference/performance.md` |
| split a change | `docs/standards/stacks.md` |
| leave something undone, or file an issue | `docs/standards/issues.md` |
| open a PR | `docs/standards/pull-requests.md` |
| push, queue or merge | `docs/standards/merging.md` |
| review a change | `docs/standards/reviews.md` |
| use one of the project's words | `docs/reference/terms.md` |
