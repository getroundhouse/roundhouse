# Rust

How Rust is written in this repo, in every crate of the Cargo workspace. `mobile-poc/` is a standalone prototype and is exempt. So is our copy of gpui-kit in `third_party/gpui-kit/vendor`, which is upstream's code, in upstream's workspace, with its manifests and lints: a change to it is written as upstream would write it, to upstream's standards, so it can go upstream as it is, as `third_party/README.md` says.

Two ideas sit behind the rules:

- **Make the failure impossible.** A type, an exhaustive match or a missing dependency beats a guard and a review comment.
- **Each recurring problem gets one mechanism.** If one doesn't fit a new case, change it rather than adding a second way.

## Toolchain and workspace

- `rust-toolchain.toml` pins Rust's version and components, and rustup installs them on first use. There's no `rust-version`: nothing is published, so the pin is the only version to keep.
- Every crate inherits `[workspace.package]`, sets `[lints] workspace = true`, and takes each dependency from `[workspace.dependencies]`, turning on only the features it uses. A dependency only one platform's code calls goes in that platform's `[target.'cfg(…)'.dependencies]`, since Cargo's `unused_dependencies` fails the build on the others.
- `[workspace.dependencies]` gives each crate its full version, such as `serde = "1.0.229"` rather than `"1"`, so `Cargo.toml` says which version each is on. Renovate raises them as new ones come out, as `.github/renovate.json5` says, but for three: `gpui-kit`, which is taken from our copy by path, `gpui-pre-apple`, which has to be the kit's GPUI, and `gpui-pre-ios`, which has to be on that GPUI too. `tools/kit-sync` moves the first two to the versions the copy has, and `gpui-pre-ios` moves by hand when its copy is replaced, as `third_party/README.md` says. The crates under ours are only in `Cargo.lock`.
- A crate's version is three days old before Cargo takes it, as `.cargo/config.toml` has it, and before Renovate proposes it: time for scanners to find malware, and, for a new crate, the 72 hours in which crates.io lets its owner delete it. Renovate proposes a security fix at once, but Cargo still waits for it, unless it's taken by hand, as `.cargo/config.toml` says.
- `Cargo.lock` is committed, and commands that check something use `--locked`.
- Code is formatted with `rustfmt`, and `rustfmt.toml` stays at 120 columns.

## Lints

`Cargo.toml` and `clippy.toml` turn these on, and a warning counts as a failure. A policy test checks that every crate inherits them.

| Rule | Lint |
|---|---|
| Clippy's pedantic group, except four that mostly ask for noise | `pedantic`, minus `missing_errors_doc`, `missing_panics_doc`, `must_use_candidate` and `assert_is_empty`, which would have `assert!(ids.is_empty())` spelled `assert_eq!(ids, [] as [SessionId; 0])` |
| Cargo's own lints, such as `unused_dependencies`, which checks `[dependencies]` and `[build-dependencies]` but not dev-dependencies. | Its `style` and `suspicious` groups, at `deny`, since the checks' `-D warnings` reaches only the compiler's lints |
| No `#[allow]` | `allow_attributes`, `allow_attributes_without_reason` |
| No `unwrap`, `expect` or `dbg!` outside tests | `unwrap_used`, `expect_used`, `dbg_macro`, with `allow-*-in-tests` in `clippy.toml` |
| Modules with children use `mod.rs` | `self_named_module_files` |
| Definitions live in named files, not `mod.rs` | `definition_in_module_root` |
| Nothing `pub` outside a crate's API | `unreachable_pub` |
| Every `unsafe` block says why it's sound | `undocumented_unsafe_blocks` |
| No `std::sync::Mutex` | `disallowed-types` in `clippy.toml`. See "Async and locks" |
| Text views are made as `docs/decisions/transcript-keeps-its-texts.md` says. | `disallowed-methods` in `clippy.toml` |
| No reading a row's column by its number. The hub reads it by name, as `crates/hub/src/store/columns.rs` says | `disallowed-methods` in `clippy.toml` |
| A colour drawn over another is a role at a step of `look::Strength`, as `docs/spec/look.md` says | `disallowed-methods` in `clippy.toml` |
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
- **A rule two crates apply has one home, which both call:** the lowest crate both depend on, when the rule is about that crate's types, as what a hub and a fake hub each decide about a record is in `protocol`, or else a crate of its own that both depend on, as running a program is in `process`. Never a copy in each, which drift apart, and never a dependency that points up to reach it.
- **`lib.rs` and `main.rs` hold no logic.** They declare modules, re-export the API, and wire things together.
- **Libraries don't read the environment.** `main` reads it and passes values down, such as by handing a function a lookup closure. Passing the environment on to a child process is fine.
- **Plan the module tree first.** Each file has one responsibility. A module with children in separate files lives in `foo/mod.rs`, with its children beside it, so the whole module stays in one folder. A module with no children in separate files stays in `foo.rs`.
- **`mod.rs` is the module's table of contents.** It holds module declarations, imports and re-exports. Definitions and their implementations live in sibling files named for their responsibility, so code can be found by its file's name. This applies to small modules and tests too.
- **A module is the file its name says, in its own package.** Nothing names a module's file with `#[path]`, or takes source in with `include!`, but for what a build script writes to `OUT_DIR`. So another crate's code comes through a dependency, which Cargo and the policy tests see. And code a build script runs, whose tests Cargo never runs there, goes in a crate the build script depends on, as `windows-icon` is for the desktop app's. A policy test enforces this.
- **No grab-bag modules** such as `utils`, `helpers`, `common` or `types`. A type lives with the code that owns its invariants.
- **Private by default.** Use `pub(crate)` within a crate, and `pub` only for the crate's API, re-exported from `lib.rs`.
- **Imports are explicit.** Only `prelude::*` and `super::*` in test modules can be globs, and Clippy allows both.
- **The main item comes before its helpers.**

## Types

- **When in doubt, add a type.** More typing is almost always better. A value that means something specific gets its own type, even when nothing is confusing it yet.
- **IDs are newtypes, not strings,** including IDs that belong to another program. One that's serialized is `#[serde(transparent)]`, so its wire and stored forms stay a plain string.
- **Closed sets are enums,** not strings or booleans, and matched exhaustively, so a new variant makes the compiler list every place to review. Use `let … else` when you want one variant and everything else is the same failure, such as a reply to one request. The lint allows it, and a new variant has nothing to change there.
- **A function that picks out two or three variants, and treats every other the same, may leave the rest to a `_` arm.** Its `match` names each variant it picks out and ends in `_`, and the function `#[expect]`s `wildcard_enum_match_arm` with a reason that names those variants. It's a method beside the enum, named for the question it answers, as `EventBody::sets_background` is, which picks out `AgentTasks`, `AgentStarted` and `AgentExited`. Its test goes through each variant it picks out and one it doesn't, and its callers match what it returns exhaustively. A function that acts on more variants, or treats the rest in more than one way, lists every variant. The reason is that a new variant makes the compiler stop at every match, and at a function like this the stop nearly always gets only an arm that does nothing. What's lost is that stop: a new variant that the function should pick out, such as a second event that changes what runs in the background, is caught only by reading the functions beside the enum. So they stay few, and stay there.
- **Invariants hold from construction.** A fallible constructor or parser returns a typed error, and no public field can bypass it. A record with no invariants keeps public fields.
- **Units go in the name or the type:** `created_at_ms`, `Duration`. Timeouts are named constants, with a comment on why that value.
- **Casts use `From` and `TryFrom`.** Where `as` is right, such as pixel math, put it in a small function that `#[expect]`s the lint and says why the value fits.
- **Derive only what means something for the type,** and borrow unless the function stores or transforms what it's given.

## Errors

- **Library crates define `thiserror` enums,** with a variant for each condition callers handle. Callers match variants, not messages. `anyhow` is for binaries and tests, whose errors only reach a person or a log. The one exception is an error type shared across a process boundary, which `docs/principles/layers.md` describes.
- **A condition has one home:** one type, in the lowest crate that has it. A caller passing it on wraps it with `#[error(transparent)]` and `#[from]`, rather than declaring it again.
- **Wrapping keeps the source and adds context** at I/O, database and process boundaries: which path, which command.
- **Log as `docs/principles/observability.md` says.**
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

