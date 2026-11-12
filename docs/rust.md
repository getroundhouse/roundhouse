# Rust

How Rust is written in this repo, in every crate of the Cargo workspace. `mobile-poc/` is a standalone prototype and is exempt.

Two ideas sit behind the rules:

- **Make the failure impossible.** A type, an exhaustive match or a missing dependency beats a guard and a review comment.
- **Each recurring problem gets one mechanism.** If one doesn't fit a new case, change it rather than adding a second way.

## Toolchain and workspace

- `rust-toolchain.toml` pins Rust's version and components, and rustup installs them on first use. There's no `rust-version`: nothing is published, so the pin is the only version to keep.
- Every crate inherits `[workspace.package]`, sets `[lints] workspace = true`, and takes each dependency from `[workspace.dependencies]`, turning on only the features it uses.
- `Cargo.lock` is committed, and commands that check something use `--locked`.
- Tests run under `cargo test`, and CI runs them with nextest, which runs every test binary's tests in one pool where `cargo test` runs one binary after another. nextest skips doc tests, so CI runs those with `cargo test --doc`. nextest gives each test a process of its own, and `cargo test` runs a binary's tests in one process, so a test has to pass both ways. So a `OnceLock` in a test runs once per test under nextest. Work that should happen once per run, such as building a helper, needs a file lock in `CARGO_TARGET_TMPDIR`, keyed by `NEXTEST_RUN_ID`.
- Code is formatted with `rustfmt`, and `rustfmt.toml` stays at 120 columns.

## Lints

`Cargo.toml` and `clippy.toml` turn these on, and a warning counts as a failure. A policy test checks that every crate inherits them.

| Rule | Lint |
|---|---|
| Clippy's pedantic group, except three that mostly ask for noise | `pedantic`, minus `missing_errors_doc`, `missing_panics_doc` and `must_use_candidate` |
| No `#[allow]` | `allow_attributes`, `allow_attributes_without_reason` |
| No `unwrap`, `expect` or `dbg!` outside tests | `unwrap_used`, `expect_used`, `dbg_macro`, with `allow-*-in-tests` in `clippy.toml` |
| No `mod.rs` | `mod_module_files` |
| Nothing `pub` outside a crate's API | `unreachable_pub` |
| Every `unsafe` block says why it's sound | `undocumented_unsafe_blocks` |
| No `std::sync::Mutex` | `disallowed-types` in `clippy.toml`. See "Async and locks" |
| Text views are made as `docs/ui.md` says. *Unconfirmed.* | `disallowed-methods` in `clippy.toml` |
| No glob imports | `wildcard_imports`, part of pedantic. See "Crates and modules" |
| Functions of at most 60 lines | `too_many_lines`, with `too-many-lines-threshold` in `clippy.toml` |

Each library also sets `#![warn(clippy::wildcard_enum_match_arm)]` in `lib.rs`. It goes there because Cargo refuses `[lints] workspace = true` alongside a crate's own lints, and because this lint isn't useful in tests.

To silence a lint, put `#[expect(clippy::…, reason = "…")]` on the smallest item, with a reason that says why the code is right. `expect` warns once it's no longer needed, so stale ones get removed. Don't silence lints across a crate or module, with one exception: programs in `examples/` that exist for tests, under "Tests".

`missing_docs` stays off, because most of what it asks for would repeat a field's name, which "Docs" says not to write.

## Size limits

- **Files:** 400 lines, tests included, and aim for under 300. A policy test enforces this. It holds the few exceptions with their reasons, and fails when an exception is no longer needed.
- **Functions:** 60 lines.

At a limit, split by responsibility, not into halves or numbered parts. A flat `match` with one arm per case, such as a server's dispatch, can `#[expect(clippy::too_many_lines, reason = "…")]`, as long as each arm is little more than a call.

## Crates and modules

- **A new part is a module** unless it needs something a module can't give it: another build target, a heavy dependency other users shouldn't pull in, a binary, a proc macro, or a user that mustn't pull in the rest. Size is no reason either way.
- **A crate depends on what it calls.** A trait is for several implementations, or one the docs plan. A trait that only exists so a crate can call one above it shows the dependency points the wrong way.
- **`lib.rs` and `main.rs` hold no logic.** They declare modules, re-export the API, and wire things together.
- **Libraries don't read the environment.** `main` reads it and passes values down, such as by handing a function a lookup closure. Passing the environment on to a child process is fine.
- **Plan the module tree first.** Each file has one responsibility and is named for it. A module with children is `foo.rs` beside `foo/`.
- **No grab-bag modules** such as `utils`, `helpers`, `common` or `types`. A type lives with the code that owns its invariants.
- **Private by default.** Use `pub(crate)` within a crate, and `pub` only for the crate's API, re-exported from `lib.rs`.
- **Imports are explicit.** Only `prelude::*` and `super::*` in test modules can be globs, and Clippy allows both.
- **The main item comes before its helpers.**

## Types

- **When in doubt, add a type.** More typing is almost always better. A value that means something specific gets its own type, even when nothing is confusing it yet.
- **IDs are newtypes, not strings,** including IDs that belong to another program. One that's serialized is `#[serde(transparent)]`, so its wire and stored forms stay a plain string.
- **Closed sets are enums,** not strings or booleans, and matched exhaustively, so a new variant makes the compiler list every place to review. Use `let … else` when you want one variant and everything else is the same failure, such as a reply to one request. The lint allows it, and a new variant has nothing to change there.
- **Invariants hold from construction.** A fallible constructor or parser returns a typed error, and no public field can bypass it. A record with no invariants keeps public fields.
- **Units go in the name or the type:** `created_at_ms`, `Duration`. Timeouts are named constants, with a comment on why that value.
- **Casts use `From` and `TryFrom`.** Where `as` is right, such as pixel math, put it in a small function that `#[expect]`s the lint and says why the value fits.
- **Derive only what means something for the type,** and borrow unless the function stores or transforms what it's given.

## Errors

- **Library crates define `thiserror` enums,** with a variant for each condition callers handle. Callers match variants, not messages. `anyhow` is for binaries and tests, whose errors only reach a person or a log. The one exception is an error type shared across a process boundary, which `docs/architecture.md` describes.
- **A condition has one home:** one type, in the lowest crate that has it. A caller passing it on wraps it with `#[error(transparent)]` and `#[from]`, rather than declaring it again.
- **Wrapping keeps the source and adds context** at I/O, database and process boundaries: which path, which command.
- **Panics are for programmer errors only.** Input from users, the network, storage and other programs is fallible.

## Async and locks

- **Blocking work in async code goes through `spawn_blocking`.**
- **Locks that aren't held across `.await` use `parking_lot::Mutex`.** It doesn't poison, so `.lock()` can't fail and needs no `unwrap`. The trade-off is that after a panic, other tasks see whatever state the panic left, so hold locks briefly and do no I/O under them. `clippy::await_holding_lock` catches a guard held across `.await`.
- **`tokio::sync::Mutex` is only for a lock that must be held across `.await`.**

## Unsafe

Every `unsafe` block has a `// SAFETY:` comment saying why it's sound, inside the smallest safe function that can hold it.

## Docs

A doc says what the signature can't: the contract, units, order, what absence means, and why. Leave out a doc that only repeats the item's name and type.

Public code that nothing calls yet names the doc whose plan will call it. Code that nothing calls and no plan names gets removed.

## Tests

- **Test each guarantee at the narrowest boundary that proves it.** Assert what can be observed: returned values, stored state, effects, and state left unchanged after a refusal. Separate setup, the action and assertions with blank lines.
- **Each level tests its own guarantees.** A behavior tested at one level appears at a higher one only as one example showing the wiring. `docs/testing.md` lists the levels.
- **A few unit tests sit beside their code.** When they grow, they move to a sibling `tests.rs`, declared with `#[cfg(test)] mod tests;`.
- **A crate keeps one integration test binary,** `tests/<name>/main.rs` with its modules beside it, since each file directly in `tests/` links separately.
- **Integration test files start with `#![cfg(test)]`.** Clippy only lets code unwrap under `#[test]` or `#[cfg(test)]`, so without it a helper in `tests/` gets flagged. A fixture shared beyond one file goes in a module declared `#[cfg(test)]`.
- **Programs in `examples/` that exist for tests are test code,** such as a fake of another program. They can unwrap, with one `#![expect(…, reason = "…")]` at the top.
- **What only tests read or set goes behind the crate's `testing` feature,** which the crate turns on for its own tests by naming itself in `[dev-dependencies]`, so the product never builds it.
- **A test makes a session, workspace, event or subagent from `protocol::fixtures`,** behind protocol's `testing` feature, and sets only what it's about, with struct update syntax. A field added to a record then changes one builder rather than every test that makes one. A test of how a record reads on the wire, such as one from an older peer, spells out its JSON instead.
- **Laws, invariants and round trips are property tests** with fixed seeds that shrink a failure to its smallest case. An example beside a law pins only what the law doesn't.
- **Errors are asserted by variant or code, not by message.** The exception is text the user sees, which is what a UI test is about.
- **Tests wait on a condition, not a duration.** Use a channel or barrier, or poll with a deadline, and never a fixed sleep.
