# Rust

How Rust is written in this repo, in every crate of the Cargo workspace. `mobile-poc/` is a standalone prototype and is exempt.

Two ideas sit behind the rules:

- **Make the failure impossible.** A type, an exhaustive match or a missing dependency beats a guard and a review comment.
- **Each recurring problem gets one mechanism.** If one doesn't fit a new case, change it rather than adding a second way.

## Toolchain and workspace

- `rust-toolchain.toml` pins Rust's version and components, and rustup installs them on first use. There's no `rust-version`: nothing is published, so the pin is the only version to keep.
- Every crate inherits `[workspace.package]`, sets `[lints] workspace = true`, and takes each dependency from `[workspace.dependencies]`, turning on only the features it uses. A dependency only one platform's code calls goes in that platform's `[target.'cfg(…)'.dependencies]`, since Cargo's `unused_dependencies` fails the build on the others.
- `[workspace.dependencies]` gives each crate its full version, such as `serde = "1.0.229"` rather than `"1"`, so `Cargo.toml` says which version each is on. Renovate raises them as new ones come out, as `.github/renovate.json5` says. The crates under ours are only in `Cargo.lock`.
- A crate's version is three days old before Cargo takes it, as `.cargo/config.toml` has it, and before Renovate proposes it: time for scanners to find malware, and, for a new crate, the 72 hours in which crates.io lets its owner delete it. Renovate proposes a security fix at once, but Cargo still waits for it, unless it's taken by hand, as `.cargo/config.toml` says.
- `Cargo.lock` is committed, and commands that check something use `--locked`.
- Tests run under `cargo test`, and CI runs them with nextest, which runs every test binary's tests in one pool where `cargo test` runs one binary after another. nextest skips doc tests, so CI runs those with `cargo test --doc`. nextest gives each test a process of its own, and `cargo test` runs a binary's tests in one process, so a test has to pass both ways. So a `OnceLock` in a test runs once per test under nextest. Work that should happen once per run, such as building a helper, needs a file lock in `CARGO_TARGET_TMPDIR`, keyed by `NEXTEST_RUN_ID`.
- A test builds a program it runs with `cargo build --workspace`, naming it with `--example`, and a `--bin` only beside an example. Cargo turns on the features the packages a build selects ask for, and their dev-dependencies' features only for a build that names an example, test or bench, so built any other way, a program needs a second copy of every crate whose features differ from those of the tests' build for the whole workspace, and of every crate above them. Built this way, everything it needs is among what CI's and the full checks' builds of the tests made, so it compiles nothing. Any other run of the tests, such as `cargo test -p agents` or plain `cargo test`, compiles that second copy instead, and again for each crate under the program that's edited, and downloads every member's dependencies, the desktop app's included. Each such program's name is unique in the workspace, since `--example` builds every member's example of that name. *Unconfirmed.*
- Code is formatted with `rustfmt`, and `rustfmt.toml` stays at 120 columns.

## Lints

`Cargo.toml` and `clippy.toml` turn these on, and a warning counts as a failure. A policy test checks that every crate inherits them.

| Rule | Lint |
|---|---|
| Clippy's pedantic group, except four that mostly ask for noise | `pedantic`, minus `missing_errors_doc`, `missing_panics_doc`, `must_use_candidate` and `assert_is_empty`, which would have `assert!(ids.is_empty())` spelled `assert_eq!(ids, [] as [SessionId; 0])` |
| Cargo's own lints, such as `unused_dependencies`, which checks `[dependencies]` and `[build-dependencies]` but not dev-dependencies. *Unconfirmed.* | Its `style` and `suspicious` groups, at `deny`, since the checks' `-D warnings` reaches only the compiler's lints |
| No `#[allow]` | `allow_attributes`, `allow_attributes_without_reason` |
| No `unwrap`, `expect` or `dbg!` outside tests | `unwrap_used`, `expect_used`, `dbg_macro`, with `allow-*-in-tests` in `clippy.toml` |
| Modules with children use `mod.rs` | `self_named_module_files` |
| Definitions live in named files, not `mod.rs` | `definition_in_module_root` |
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
- **Plan the module tree first.** Each file has one responsibility. A module with children in separate files lives in `foo/mod.rs`, with its children beside it, so the whole module stays in one folder. A module with no children in separate files stays in `foo.rs`.
- **`mod.rs` is the module's table of contents.** It holds module declarations, imports and re-exports. Definitions and their implementations live in sibling files named for their responsibility, so code can be found by its file's name. This applies to small modules and tests too.
- **No grab-bag modules** such as `utils`, `helpers`, `common` or `types`. A type lives with the code that owns its invariants.
- **Private by default.** Use `pub(crate)` within a crate, and `pub` only for the crate's API, re-exported from `lib.rs`.
- **Imports are explicit.** Only `prelude::*` and `super::*` in test modules can be globs, and Clippy allows both.
- **The main item comes before its helpers.**

## Platforms

The app, the hub and the workers run on macOS, Windows and Linux. Where a platform reaches a goal its own way, the difference stays out of the code that needs the goal, so that code reads the same everywhere, and each platform has to say how it reaches it. A step one platform leaves out is then a function that platform doesn't have, which doesn't build.

- **Code that needs something done calls one function for it, which every platform has with the same name and arguments,** so a caller builds against any platform's. It's named for what it does, such as `process::signal_group`, and nothing around the call checks the platform, with `#[cfg]` or `cfg!()`. A call marked `#[cfg(unix)]` builds on Windows without it, so nothing tells a step Windows doesn't need from one that was forgotten.
- **A platform with nothing to do still has the function, and its doc says why,** as `process::keep_stdio_from_children` does nothing on Unix, where what a child is given takes the place of this process's own. One that only returns `Ok` or `Some`, and isn't public outside its crate, which is where Clippy raises `clippy::unnecessary_wraps`, `#[expect]`s it, and the reason says whose place it takes.
- **What a platform can't do at all is said where the product decides what to do without it.** Where it decides beforehand, that's a constant named for what the platform can do, such as the hub's `KEEPS_WORKERS`, which is `cfg!(unix)`, and `docs/architecture.md` says what the product does without it, as it does for a worker that can't outlive its hub on Windows. Where the caller goes on either way, the function does nothing there and its doc says what that platform goes without, as the desktop app's `speak` says only a Mac plays a sound. Otherwise it returns an error, or `None` where its doc says what that means, so a caller that needs it to have happened knows it didn't.
- **A check that builds on every platform is `cfg!()`, inside the function for the goal or in a constant named for what differs,** such as `protocol::paths::HOME_VAR`, or the hub's `Stimulant::for_this_machine`, which finds none but on a Mac. Then each platform's branch is built and checked wherever the code is, and a mistake in one shows on the machine that made it. In the app, what the kit knows by the platform it runs on, such as which keys to bind, goes by `cx.platform_info()`, as `docs/gpui.md` says.
- **`#[cfg]` is for what doesn't build elsewhere, such as a call into `std::os::unix`, or what only one platform uses. It goes on an item, such as a function, method, type, `impl`, const or module, and on the imports only those use, never on anything inside a function's body, such as a statement or a match arm.** Each platform's version of a function is whole, beside the others. What only one platform's versions need goes in a module for that platform, as the desktop app's `window_place/system/bounds.rs` keeps its calls into Windows in `win32`. A module whose items all differ has a module for each platform with the same names. Each is a file of its own, which the module above picks with `#[cfg]` and re-exports, as the hub's `listener` does, or, at a few lines each, a module of the same name for each platform, as the desktop app's `lost_release::system` is. A field only one platform reads stays on every platform, and `#[expect]`s `dead_code` on the others through `#[cfg_attr]`, as the worker's `Closing` does, since `#[cfg]` on a field would need `#[cfg]` where it's set.
- **What only one platform's own code calls is declared for that platform alone,** such as the desktop app's `window_handle` on Windows, or the worker's `listen` on Unix. Nothing every platform runs calls it, so no step is left out without it.
- **A check names the family, `unix` or `windows`, where every system in it does the same, and `target_os` where one system does its own,** such as macOS's menu bar in the desktop app's `menus/system_menu.rs`, so the check says why the code differs: by what the family shares, such as Unix sockets, or by what one system has.
- **Logic one platform uses that needs none of its APIs is built for tests on every platform too,** as `#[cfg(any(target_os = "windows", test))]` builds the arithmetic for placing the window on Windows in the desktop app's `window_place/system/bounds.rs`. Then the tests that run elsewhere cover it.
- **A test of a goal is written for every platform, so each platform's way is tested.** It doesn't take a path from `/` for an absolute one, which it isn't on Windows, it compares paths as paths rather than as text, and it stands a program in `examples/` in for another program, rather than a shell script. A test of what only one platform does, such as a Unix mode, is `#[cfg]` for that platform, and so is one whose expected results are one platform's, such as the gallery's pictures, which are exact to one Mac.

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
- **A few unit tests sit beside their code.** When they grow, they move to `foo/tests.rs`, declared with `#[cfg(test)] mod tests;` in `foo/mod.rs`.
- **A crate keeps one integration test binary,** `tests/<name>/main.rs` with its modules beside it, since each file directly in `tests/` links separately.
- **Integration test files start with `#![cfg(test)]`.** Clippy only lets code unwrap under `#[test]` or `#[cfg(test)]`, so without it a helper in `tests/` gets flagged. A fixture shared beyond one file goes in a module declared `#[cfg(test)]`.
- **Programs in `examples/` that exist for tests are test code,** such as a fake of another program. They can unwrap, with one `#![expect(…, reason = "…")]` at the top.
- **What only tests read or set goes behind the crate's `testing` feature,** which the crate turns on for its own tests by naming itself in `[dev-dependencies]`, so the product never builds it.
- **A test makes a session, workspace, event, subagent or child worktree from `protocol::fixtures`,** behind protocol's `testing` feature, and sets only what it's about, with struct update syntax. A field added to a record then changes one builder rather than every test that makes one. A test of how a record reads on the wire, such as one from an older peer, spells out its JSON instead.
- **Laws, invariants and round trips are property tests** with fixed seeds that shrink a failure to its smallest case. An example beside a law pins only what the law doesn't.
- **Errors are asserted by variant or code, not by message.** The exception is text the user sees, which is what a UI test is about.
- **Tests wait on a condition, not a duration.** Use a channel or barrier, or poll with a deadline, and never a fixed sleep.
