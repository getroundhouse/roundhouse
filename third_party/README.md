# Code from other projects

Other projects' code that the app builds from a copy kept here, on a release of the project's own, with our changes on top. There's one, gpui-kit.

## gpui-kit

`gpui-kit/` is our copy of [Longbridge's gpui-kit](https://github.com/longbridge/gpui-kit), the component kit the desktop app is built on, at the release `gpui-kit.toml` names. The app takes it as a path dependency, as `Cargo.toml` says.

The kit often has nearly what the app needs: a hook it keeps private, an option it doesn't have, a bug in what it draws. The change to it is often a few lines. With a copy, that change goes in the same pull request as the app code that needs it, and the app's tests, the gallery and the merge queue check the two together. Each change is also offered upstream, so the copy stays close to upstream's, and a new release brings less to merge. `docs/gpui.md` says when a change belongs in the kit rather than the app.

### What's here

`tools/kit-sync` makes the copy from upstream's repository, and keeps:

- `crates/`: gpui-kit and the crates under it, `gpui-base`, `gpui-component`, its macros and `gpui-kit-assets`, each whole but for `gpui-base`'s examples. Its manifest leaves them out of the crate it publishes too, and they read a font from upstream's `crates/story-web`, under the SIL Open Font License, which `tools/policy` doesn't allow.
- `Cargo.toml`: upstream's workspace manifest, which these crates take their versions, dependencies and lints from, listing only the packages the copy keeps.
- `.rustfmt.toml`: upstream's settings for rustfmt, which `cargo fmt --all` reads for these files, since they're nearer than ours.
- `LICENSE-APACHE` and upstream's `README.md`.
- The files elsewhere in upstream's repository that these crates' tests read, so they build here: `examples/fixtures/test.md` and `themes/aurora.json`. The tool refuses a release whose kept code reads a file the copy doesn't keep.

Upstream's other crates, its examples outside these crates, its website and its lockfile aren't kept. Nor is GPUI: the kit pins an exact snapshot of it, which Longbridge publishes unchanged from Zed's as `gpui-pre`, and which comes from crates.io like any other crate.

`gpui-kit.toml`, beside the copy, is the release and commit it was made from, and what it kept of them, which the tool writes, and reads to merge the next release with our changes.

The code is under Apache-2.0. The icons in `crates/assets` are Lucide's, under ISC, but for those Lucide took from Feather, which are under MIT, and a few of the kit's own, as `crates/assets/README.md` says. `tools/policy` checks each.

### Changing the kit

- **Write it as upstream would,** as `docs/rust.md` says, so it can go upstream as it is.
- **Change it in the same pull request as the app code that needs it,** or in a stack with the kit's change at the bottom, as `AGENTS.md` says of layers.
- **Say in each file it changes that it was changed,** with a comment near the top that holds `Changed in Roundhouse's copy of gpui-kit, as third_party/README.md lists.` as the tool's `NOTICE` has it, since Apache-2.0 asks a changed file to say so. `cargo run -p kit-sync -- diff` leaves the line out. A file that can't hold a comment, such as a JSON theme, is named in its entry under "Our changes" instead.
- **List it under "Our changes",** with the files it changes, why, and where it was offered upstream.
- **Leave no warnings.** Our builds show the compiler's warnings in the kit, but `-D warnings` and clippy don't reach it, so nothing fails on them. Run upstream's own checks on the crates it changes, in the copy's workspace:

  ```sh
  cargo clippy --manifest-path third_party/gpui-kit/Cargo.toml --target-dir target/gpui-kit -p gpui-component --all-targets --features test-support
  cargo test --manifest-path third_party/gpui-kit/Cargo.toml --target-dir target/gpui-kit -p gpui-component --features test-support
  ```

  The tests need `test-support`, which upstream's whole workspace turns on for them and the copy's doesn't. They build GPUI and the kit's dependencies a second time, with upstream's settings, under `target/`, where `cargo clean` reaches them, and resolve a `Cargo.lock` in the copy, which git ignores.

  Cargo also warns about upstream's manifests: the workspace's names crates the copy doesn't keep or use, in its dependencies and its profiles, and a `publish` no kept crate inherits, `gpui-kit-assets` has no `[lints]`, and `gpui-component` names two dependencies it doesn't use. Leave those to upstream, since each change to a manifest is one more to merge at every release. The warnings to clear are those in the code a change touches.

### Offering a change upstream

Upstream asks for a pull request that does one thing, as its `CONTRIBUTING.md` says, so each of our changes is offered in a pull request of its own, from a fork of upstream's repository on GitHub.

1. Make a patch of the change, from the root of upstream's repository. `cargo run -p kit-sync -- diff` prints all our changes against the release the copy is on, and `cargo run -p kit-sync -- diff <path>…` prints our changes to the files named, from the copy, such as `crates/base/src/text/node.rs`. Either leaves out the lines that say a file was changed.
2. Apply it to a branch from upstream's `main` in a clone of the fork, with `git apply -3`, and run upstream's checks there.
3. Open the pull request from the fork. One that adds, changes or removes anything public lists it under a `## Public API` heading, as upstream's `CONTRIBUTING.md` asks. Add its link to the change's entry here.

Once a release has the change, taking that release merges it as the same change on both sides. Its entry goes, with the line that said the file was changed.

### Taking a new release

The newest release is taken at least once a week, since upstream tags about one a week, so each brings little to merge. Each is taken in a pull request of its own, since it can change the kit's API and how the app looks.

1. Set `RELEASE` and `COMMIT` in `tools/kit-sync/src/pin.rs` to the release's tag and the commit the tag points at.
2. Run `cargo run -p kit-sync`. It merges what upstream changed since the copy's release with what we changed, as git merges two branches, then moves `gpui-kit` and `gpui-pre-apple` in `Cargo.toml` to the versions the copy has. Where both changed the same lines, git marks the conflict in the file, and the tool names each file. Settle each, then run it again, which refuses while a mark is left.
3. Run `cargo check`, which moves `Cargo.lock`, then the full checks and the gallery, as `AGENTS.md` says.

When a release adds a crate under gpui-kit, or drops one, `KEPT` in `tools/kit-sync/src/pin.rs` changes with it, and so does `VENDORED` in `tools/policy` if the crate has a license file of its own. An entry of `KEPT` that starts with `!` is a folder of a kept crate to leave out, as `gpui-base`'s examples are, whose members the workspace stops listing, and whose code isn't checked for files it reads. The tool refuses one that upstream no longer has.

Renovate opens a pull request for each new release, on its weekly schedule. It changes only `RELEASE`, and the policy test fails until the copy is on the release the tool pins, so it can't merge before the copy is brought to it, in that pull request or one that takes its place.

### Our changes

| Change | Files | Why | Upstream |
|---|---|---|---|
| The workspace lists only the packages the copy keeps | `Cargo.toml` | Cargo looks for every member, and the copy keeps few of upstream's. `tools/kit-sync` makes this change itself | Not offered, since upstream builds them all |
