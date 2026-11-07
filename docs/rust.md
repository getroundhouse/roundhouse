# Rust

How Rust is written in this repo. `docs/architecture.md` owns which crate may depend on which, and this doc owns what goes inside a crate. It covers the Cargo workspace. `mobile-poc/` is a standalone prototype and is exempt.

**Status: agreed with the user on 2026-11-06, and not adopted yet. New code follows it, but nothing here is enforced, and much of the existing code doesn't follow it.** The [Adopt the Rust standards](https://github.com/zlangbert/trestle/milestone/7) milestone closes those gaps.

Two ideas sit behind the rules:

- **Make the failure impossible.** A type, an exhaustive match or a missing dependency beats a guard and a review comment.
- **Each recurring problem gets one mechanism.** If one doesn't fit a new case, change it rather than adding a second way.

## Toolchain and workspace

- `rust-toolchain.toml` pins Rust's version and components, and rustup installs them on first use. The Docker command in `AGENTS.md` uses the same version. There's no `rust-version`: nothing is published, so the pin is the only version to keep.
- Every crate inherits `[workspace.package]`, sets `[lints] workspace = true`, and takes each dependency from `[workspace.dependencies]`, turning on only the features it uses. `third_party/` is exempt, because it mirrors another crate's API.
- `Cargo.lock` is committed, and commands that check something use `--locked`.
- `rustfmt.toml` stays at 120 columns.

## Checks

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

On Linux, drop `--workspace`, since the desktop app and transcript need GPUI. There's no CI yet, so run these before calling work done. Warnings count as failures. When the repo gets CI, it runs these same commands.

`tools/policy/tests/policy.rs` holds what Clippy can't check: the dependency rules, licenses, file length, and that every crate inherits the lints. That way `cargo test` fails on them, on any platform.

## Lints

`Cargo.toml` and `clippy.toml` turn on:

| Rule | Lint |
|---|---|
| Clippy's pedantic group, except three that mostly ask for noise | `pedantic`, minus `missing_errors_doc`, `missing_panics_doc` and `must_use_candidate` |
| No `#[allow]` | `allow_attributes`, `allow_attributes_without_reason` |
| No `unwrap`, `expect` or `dbg!` outside tests | `unwrap_used`, `expect_used`, `dbg_macro`, with `allow-*-in-tests` in `clippy.toml` |
| No `mod.rs` | `mod_module_files` |
| Nothing `pub` outside a crate's API | `unreachable_pub` |
| Every `unsafe` block says why it's sound | `undocumented_unsafe_blocks` |
| No `std::sync::Mutex` | `disallowed-types` in `clippy.toml`. See "Async, locks and processes" |
| No `use gpui_kit::*` | `wildcard_imports`, part of pedantic. See "Crates and modules" |
| Functions of at most 60 lines | `too_many_lines`, with `too-many-lines-threshold` in `clippy.toml` |

Each library also sets `#![warn(clippy::wildcard_enum_match_arm)]` in `lib.rs`. It goes there because Cargo refuses `[lints] workspace = true` alongside a crate's own lints, and because this lint isn't useful in tests.

To silence a lint, put `#[expect(clippy::…, reason = "…")]` on the smallest item, with a reason that says why the code is right. `expect` warns once it's no longer needed, so stale ones get removed. Don't silence lints across a crate or module, with one exception: the test programs in `examples/`, under "Tests".

`missing_docs` stays off. `protocol` alone would need 420 new docs, most of them repeating a field's name, which "Docs" says not to write.

## Size limits

- **Files:** 400 lines, tests included, and aim for under 300. A policy test enforces this. It holds the few exceptions with their reasons, and fails when an exception is no longer needed.
- **Functions:** 60 lines.

At a limit, split by responsibility, not into halves or numbered parts. A flat `match` with one arm per request, such as `hub::server`'s dispatch, can `#[expect(clippy::too_many_lines, reason = "…")]`, as long as each arm is little more than a call.

## Crates and modules

- **A new part is a module** unless it needs something a module can't give it: another build target, a heavy dependency other users shouldn't pull in, a binary, a proc macro, or a user that mustn't pull in the rest, as with GPUI and `client-core`. Size is no reason either way.
- **A crate depends on what it calls.** A trait is for several implementations, or one the docs plan, such as `Provider` and `AgentProcess`. A trait that only exists so a crate can call one above it shows the dependency points the wrong way.
- **`lib.rs` and `main.rs` hold no logic.** They declare modules, re-export the API, and wire things together.
- **Plan the module tree first.** Each file has one responsibility and is named for it. A module with children is `foo.rs` beside `foo/`.
- **No grab-bag modules** such as `utils`, `helpers`, `common` or `types`. A type lives with the code that owns its invariants.
- **Private by default.** Use `pub(crate)` within a crate, and `pub` only for the crate's API, re-exported from `lib.rs`.
- **Imports are explicit.** Only `prelude::*` and `super::*` in test modules can be globs, and Clippy allows both. `use gpui_kit::*` is out because it also brings in GPUI's `test` attribute, which makes `#[test]` recurse. Explicit imports remove that trap.
- **The main item comes before its helpers.**

## Types

- **IDs are newtypes, not strings.** `SessionId(String)` is `#[serde(transparent)]`, so the wire and stored forms don't change. They're declared together in `protocol::model` by one macro. `protocol` can't depend on rusqlite, so `hub::store` converts them at its edge.
- **Closed sets are enums,** not strings or booleans, and matched exhaustively, so a new variant makes the compiler list every place to review. Use `let … else` when you want one variant and everything else is the same failure, such as a reply to one request. The lint allows it, and a new variant has nothing to change there.
- **Invariants hold from construction.** A fallible constructor or parser returns a typed error, and no public field can bypass it. A record with no invariants, which is most of `protocol`, keeps public fields.
- **Units go in the name or the type:** `created_at_ms`, `Duration`. Timeouts are named constants, with a comment on why that value.
- **Casts use `From` and `TryFrom`.** Where `as` is right, such as pixel math, put it in a small function that `#[expect]`s the lint and says why the value fits.
- **Derive only what means something for the type,** and borrow unless the function stores or transforms what it's given.

## Protocol changes

The wire format is in `docs/architecture.md`. These rules keep it compatible:

- **Each shape has one form.** It's the serde form in `protocol`, on the wire and in the hub's SQLite columns, so changing a type also changes how old rows read.
- **A new field is `Option` or `#[serde(default)]`,** so a message from an older peer still decodes. The same goes for old rows.
- **Every enum read as data has `#[serde(other)] Unknown`:** statuses, kinds, events and blocks. Without it, one new value fails the whole frame, and `client-core` silently drops frames it can't decode, including replies. Code that receives `Unknown` skips it or shows something neutral. It never panics.
- **Envelopes and requests have no `Unknown`.** That covers `ClientRequest`, `HubFrame` and the like, since there's nothing sensible to do with one. Today a hub closes a connection that sends a request it can't decode, so adding a request needs a version bump unless the hub learns to refuse it politely.
- **The worker protocol follows the same rules once workers can differ in version.** Today a hub and its worker are one binary.
- **Agent output is untrusted input.** `agents::claude::normalize` takes any line without panicking, and keeps the raw line. After a CLI upgrade, re-capture the fixtures with the script in `AGENTS.md`.

## Errors

- **Library crates define `thiserror` enums,** with a variant for each condition callers handle. Callers match variants, not messages. `anyhow` is for binaries in `apps/` and for tests.
- **`ApiError` is the exception.** Whatever crosses the hub or worker protocol is `protocol::client::ApiError`. Its `ErrorCode` is a closed set of meanings, such as `NotFound` and `Conflict`, not a transport status, so a second enum mapping onto it would only be a copy. Hub and worker operations can return it directly. Callers and tests match `code`, and `message` is for people. A condition callers must tell apart gets a new `ErrorCode`.
- **A condition has one home:** one type, in the lowest crate that has it. A caller passing it on wraps it with `#[error(transparent)]` and `#[from]`, rather than declaring it again.
- **`hub::store` has its own enum.** `ApiError` holds only text, so using it at the bottom would throw away the SQLite error.
- **Lower errors become `ApiError` in one place per crate,** an `impl From<…> for ApiError`, so the mapping reads as one table. `Internal` failures get logged there, with their source.
- **Wrapping keeps the source and adds context** at I/O, database and process boundaries: which path, which command.
- **Panics are for programmer errors only.** Input from users, the network, storage, git and the agent CLI is fallible.

## Layers

| Layer | What it does | Here |
|---|---|---|
| Handlers | Parse a request, call one operation, map its result | `hub::server`, `worker::serve` |
| Operations | One module per area, holding its logic | `hub` |
| Stores | All SQL | `hub::store` |
| Integrations | One module per outside system. Nothing else knows its flags, wire format or retries | `agents::claude`, `worker::git`, `hub::worker_link`, `providers::local` |
| Edges | Configuration and telemetry | `main` in each app |

Dependencies point inward, from handlers to operations to stores and integrations. A layer answers with a lower layer's type wherever it means the same, rather than copying it.

Libraries don't read the environment. `main` reads it and passes values down, as `protocol::paths::resolve` already does by taking a lookup function. Passing the environment on to a child process is fine.

## Async, locks and processes

- **The hub and worker run on tokio.** Blocking work goes through `spawn_blocking`, as `hub::store` does.
- **`client-core` is blocking calls and channels with no runtime,** and GPUI calls it through `cx.background_spawn`, never on the main thread.
- **Locks that aren't held across `.await` use `parking_lot::Mutex`.** It doesn't poison, so `.lock()` can't fail and needs no `unwrap`. The trade-off is that after a panic, other tasks see whatever state the panic left, so hold locks briefly and do no I/O under them. `clippy::await_holding_lock` catches a guard held across `.await`.
- **`tokio::sync::Mutex` is only for a lock that must be held across `.await`,** such as `start_lock` and `recording`.
- **Processes run in their own process group,** so a stop reaches everything they started. Signalling a group and checking a PID's start time are one mechanism each, wrapped once, and that's where the `unsafe` lives.
- **Every `unsafe` block has a `// SAFETY:` comment,** inside the smallest safe function that can hold it.

## GPUI

GPUI stays in `transcript`, `terminal-view` and `apps/`, as `docs/architecture.md` says.

- **`render` does no I/O or blocking.** It reads state the view already holds.
- **The 60-line limit applies to `render`.** Split it by region of the screen into functions that return elements.
- **Flags that exist only for tests stay out of the product.**

## Docs

A doc says what the signature can't: the contract, units, order, what absence means, and why. Leave out a doc that only repeats the item's name and type.

Public code that nothing calls yet names the doc whose plan will call it. Code that nothing calls and no plan names gets removed.

## Tests

The rule from `AGENTS.md` comes first: tests that can't fail prove nothing, so when a behavior matters, break it on purpose once and confirm a test notices.

- **Test each guarantee at the narrowest boundary that proves it.** Assert what can be observed: returned values, stored state, effects, and state left unchanged after a refusal. Separate setup, the action and assertions with blank lines.
- **Each level tests its own guarantees.** A behavior tested at one level appears at a higher one only as one example showing the wiring:

| Level | Tests | Where |
|---|---|---|
| A module | Its own rules, without a process or a socket | Unit tests |
| The agent adapter | Normalizing the CLI's captured output | `crates/agents/tests/` |
| The whole backend | What spans processes: restarts, crashes, a real worker, with a fake agent | `apps/daemon/tests/` |
| The window | Clicking and typing through to what's shown | `apps/desktop/tests/` |
| The real CLI | That the fake agent is still honest. By hand, since it costs money | `tools/smoke/` |

- **A few unit tests sit beside their code.** When they grow, they move to a sibling `tests.rs`, declared with `#[cfg(test)] mod tests;`.
- **A crate keeps one integration test binary,** `tests/<name>/main.rs` with its modules beside it, since each file directly in `tests/` links separately.
- **Shared fixtures go in a module declared `#[cfg(test)]`.** Clippy only lets code unwrap under `#[test]` or `#[cfg(test)]`, so a bare helper in `tests/` gets flagged.
- **The test programs in `examples/` are test code,** such as `fake_claude` and `look`. They can unwrap, with one `#![expect(clippy::unwrap_used, reason = "…")]` at the top.
- **What only tests read or set goes behind the crate's `testing` feature,** which the crate turns on for its own tests by naming itself in `[dev-dependencies]`, so the product never builds it.
- **Laws, invariants and round trips are property tests** with fixed seeds that shrink a failure to its smallest case. `protocol::tail` and `client-core::session_log` are the first candidates. An example beside a law pins only what the law doesn't.
- **Errors are asserted by code or variant, not by message.** The exception is text the user sees, which is what a UI test is about.
- **Tests wait on a condition, not a duration.** Use a channel or barrier, or poll with a deadline, and never a fixed sleep.
