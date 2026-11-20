# Code from other projects

Other projects' code that the apps build from a copy kept here, on a release of the project's own, with our changes on top: gpui-kit, in `gpui-kit/`, and three crates of GPUI's and Zed's, in `gpui-pre-apple/`, `gpui-pre-ios/` and `zed-font-kit/`.

## gpui-kit

`gpui-kit/vendor/` is our copy of [Longbridge's gpui-kit](https://github.com/longbridge/gpui-kit), the component kit the desktop app is built on, at the release `gpui-kit/gpui-kit.toml` names, with our patches in `gpui-kit/patches/` applied on top. The app takes it as a path dependency, as `Cargo.toml` says.

The kit often has nearly what the app needs: a hook it keeps private, an option it doesn't have, a bug in what it draws. The change to it is often a few lines. With a copy, that change goes in the same pull request as the app code that needs it, and the app's tests, the gallery and the merge queue check the two together. Keeping each change in its own patch file beside the copy shows how each one moves when a new release comes in, and keeps each ready to offer upstream on its own. `docs/standards/gpui.md` says when a change belongs in the kit rather than the app.

### What's here

`tools/kit-sync` makes `gpui-kit/vendor/` from upstream's repository, and keeps:

- `crates/`: gpui-kit and the crates under it, `gpui-base`, `gpui-component`, its macros and `gpui-kit-assets`, each whole but for `gpui-base`'s examples. Its manifest leaves them out of the crate it publishes too, and they read a font from upstream's `crates/story-web`, under the SIL Open Font License, which `tools/policy` doesn't allow.
- `Cargo.toml`: upstream's workspace manifest, which these crates take their versions, dependencies and lints from, listing only the packages the copy keeps.
- `.rustfmt.toml`: upstream's settings for rustfmt, which `cargo fmt --all` reads for these files, since they're nearer than ours.
- `LICENSE-APACHE` and upstream's `README.md`.
- The files elsewhere in upstream's repository that these crates' tests read, so they build here: `examples/fixtures/test.md` and `themes/aurora.json`. The tool refuses a release whose kept code reads a file the copy doesn't keep.

Upstream's other crates, its examples outside these crates, its website and its lockfile aren't kept. Nor is GPUI: the kit pins an exact snapshot of it, which Longbridge publishes unchanged from Zed's as `gpui-pre`, and which comes from crates.io like any other crate.

Beside `vendor/` in `gpui-kit/`:

- `patches/`: one `<name>.patch` file for each of our changes on top of the release, from the root of upstream's repository and without the notice lines the copy carries.
- `gpui-kit.toml`: the release, commit and base digest `vendor/` was made from, what it kept of them, and the order in which the patches in `patches/` apply, which the tool writes, and reads to check the copy and bring the next release in.

The code is under Apache-2.0. The icons in `crates/assets` are Lucide's, under ISC, but for those Lucide took from Feather, which are under MIT, and a few of the kit's own, as `crates/assets/README.md` says. `tools/policy` checks each.

### Changing the kit

- **Write it as upstream would,** as `docs/standards/rust.md` says, so it can go upstream as it is.
- **Change it in the same pull request as the app code that needs it,** or in a stack with the kit's change at the bottom, as `docs/standards/stacks.md` says of layers.
- **Record it in its own patch,** after editing `third_party/gpui-kit/vendor/`, by running `cargo run -p kit-sync -- patch <name>`. That writes `third_party/gpui-kit/patches/<name>.patch` (adding `<name>` to `gpui-kit.toml`, or updating it in place when it's already listed, and refreshing any patches after it) and puts `Changed in Roundhouse's copy of gpui-kit, as third_party/README.md lists.` in a comment at the top of each `.rs` and `.toml` file that differs from upstream, as Apache-2.0 asks of a changed file. `cargo run -p kit-sync -- drop <name>` takes a patch out of the copy and deletes its file. A file that can't hold a comment, such as a JSON theme, is named in its entry under "Our changes" instead.
- **List it under "Our changes",** with its patch file, the files it changes, why, and the issue that tracks offering it upstream, or where it was offered.
- **Leave no warnings.** Our builds show the compiler's warnings in the kit, but our `-D warnings` and clippy don't reach it. Upstream's own clippy and tests do, which CI's `kit` job runs on `gpui-base` and `gpui-component` for a pull request that changes the copy, and which fail on a warning. Run them before pushing, in the copy's workspace:

  ```sh
  cargo clippy --manifest-path third_party/gpui-kit/vendor/Cargo.toml --target-dir target/gpui-kit -p gpui-base -p gpui-component --all-targets --features test-support -- -D warnings
  cargo test --manifest-path third_party/gpui-kit/vendor/Cargo.toml --target-dir target/gpui-kit -p gpui-base -p gpui-component --features test-support
  ```

  The tests need `test-support`, which upstream's whole workspace turns on for them and the copy's doesn't. They build GPUI and the kit's dependencies a second time, with upstream's settings, under `target/`, where `cargo clean` reaches them, and resolve a `Cargo.lock` in the copy, which git ignores. CI copies ours there first, so the kit is checked on the versions the app ships, and `cp Cargo.lock third_party/gpui-kit/vendor/` does the same here.

  Cargo also warns about upstream's manifests: the workspace's names crates the copy doesn't keep or use, in its dependencies and its profiles, and a `publish` no kept crate inherits, `gpui-kit-assets` has no `[lints]`, and `gpui-component` names two dependencies it doesn't use. Leave those to upstream, since each change to a manifest is one more to merge at every release. The warnings to clear are those in the code a change touches.

### Offering a change upstream

Upstream asks for a pull request that does one thing, as its `CONTRIBUTING.md` says, so each of our changes is offered in a pull request of its own, from a fork of upstream's repository on GitHub.

For now, as the owner decided, each change we'd offer has an issue that tracks it, which its entry under "Our changes" links, and none is offered yet, since there's no fork. Once there is, these steps offer one:

1. Apply its patch file in `third_party/gpui-kit/patches/<name>.patch` to a branch from upstream's `main` in a clone of the fork, with `git apply -3`, and run upstream's checks there.
2. Open the pull request from the fork. One that adds, changes or removes anything public lists it under a `## Public API` heading, as upstream's `CONTRIBUTING.md` asks. Add its link to the change's entry here.

Once a release has the change, taking that release drops its patch file and removes the notice line from any file no other patch changes, and its entry here goes with it.

### Taking a new release

The newest release is taken at least once a week, since upstream tags about one a week, so each brings little to merge. Each is taken in a pull request of its own, since it can change the kit's API and how the app looks.

1. Set `RELEASE` and `COMMIT` in `tools/kit-sync/src/pin.rs` to the release's tag and the commit the tag points at.
2. Run `cargo run -p kit-sync`. It cherry-picks each patch in `gpui-kit.toml` onto the release, rewrites each file in `third_party/gpui-kit/patches/` against it (dropping any whose change is now upstream), stamps the notice lines on the copy, and moves `gpui-kit` and `gpui-pre-apple` in `Cargo.toml` to the versions the copy has. Where a patch conflicts with the release, git marks the conflict in `third_party/gpui-kit/vendor/`, and the tool names the patch and its files. Settle them there, then run `cargo run -p kit-sync` again, which refuses while a mark is left, writes that patch, and applies any patches after it.
3. Run `cargo check`, which moves `Cargo.lock`, then the full checks and the gallery, as `docs/standards/merging.md` says.

When a release adds a crate under gpui-kit, or drops one, `KEPT` in `tools/kit-sync/src/pin.rs` changes with it, and so does `VENDORED` in `tools/policy` if the crate has a license file of its own. An entry of `KEPT` that starts with `!` is a folder of a kept crate to leave out, as `gpui-base`'s examples are, whose members the workspace stops listing, and whose code isn't checked for files it reads. The tool refuses one that upstream no longer has.

Renovate opens a pull request for each new release, on its weekly schedule. It changes only `RELEASE`, and the policy test checks offline that `vendor/` is on the release the tool pins, with only the patches in `patches/` on top and no conflict left to settle, so neither an unfinished sync nor an unrecorded change to the copy can merge.

### Our changes

| Patch | Change | Files | Why | Upstream |
|---|---|---|---|---|
| — | The workspace lists only the packages the copy keeps | `Cargo.toml` | Cargo looks for every member, and the copy keeps few of upstream's. `tools/kit-sync` makes this change itself | Not offered, since upstream builds them all |
| `code-block-actions.patch` | `RenderContext::with_code_block_actions`, and `CODE_BLOCK_GROUP`, the hover group a code block with actions is | `crates/base/src/text/editor_host.rs`, `crates/base/src/text/mod.rs`, `crates/base/src/text/node.rs`, `crates/base/src/text/text_view.rs`, `crates/base/tests/text_host.rs` | The transcript draws its texts through the editor host, which had no way to give a code block the actions the kit's text view gives it, so a code block couldn't have a button that copies it. The group lets that button show only while its block is hovered | Not offered yet: #1518 |
| `strong-style.patch` | `TextViewStyle::with_strong` and `strong` | `crates/base/src/text/style.rs`, `crates/base/src/text/node.rs` | Strong emphasis was hardcoded to bold. A style per text view lets thinking keep regular weight while replies keep bold, without changing the document's marks | Not offered yet: #1534 |
| `ghost-and-outline-button-tokens.patch` | Theme tokens for a ghost button's hover, active and selected backgrounds, and an outline button's background, hover and active ones, each falling back to what the button worked out before | `crates/component/src/button/button.rs`, `crates/component/src/theme/schema.rs`, `crates/component/src/theme/theme_color.rs` | The button worked these out from other colours, such as `accent` at half its strength, so a theme couldn't name them. The app's theme names every colour it draws with, as `docs/spec/look.md` says | Not offered yet: #1647 |
| `button-content-style.patch` | Makes `Button::content_style` public, which styles what's inside a button over what its size gives it and sets its icon's size | `crates/component/src/button/button.rs` | A button's words and icon took their size only from the button's own size, so the app's sizes had to be matched to whichever of the kit's drew them as big, and held there by a test. The kit's input group already used it, crate-private | Not offered yet: #1663 |
| `input-placeholder-color.patch` | A theme token for an input's placeholder, `input.placeholder`, falling back to `muted.foreground`, and `InputEditorStyle::placeholder`, falling back to `muted_foreground` | `crates/base/src/input/base/element.rs`, `crates/base/src/input/editor/highlighting.rs`, `crates/component/src/input/input.rs`, `crates/component/src/theme/schema.rs`, `crates/component/src/theme/theme_color.rs` | An input drew its placeholder in the muted text colour it uses for much else, so a theme couldn't make a placeholder fainter without making that fainter too. The app draws a placeholder in the colour it gives hints, as `docs/spec/look.md` says | Not offered yet: #1697 |
| `switch-thumb-unchecked.patch` | A theme token for a switch's thumb while it's off, `switch.thumb.unchecked.background`, falling back to `switch.thumb.background` | `crates/component/src/switch.rs`, `crates/component/src/theme/schema.rs`, `crates/component/src/theme/theme_color.rs` | A switch drew its thumb in one colour on and off, so a theme couldn't light an off switch's thumb without lighting an on one's, on its light track, too. The app draws an off toggle's thumb muted, as `docs/spec/components.md` says | Not offered yet: #1723 |
| `popup-menu-style.patch` | `PopupMenuStyle`, a global that sets how every menu is drawn: a row's height, room, gap and corners, the room around the rows and between them, a row's icon's size and colour, the light under the pointer, a separator's width, margin and overhang, which side a check is on for a menu that doesn't choose, and the menu's corners, edge and shadow, each keeping the menu's own where it's `None`; a label as tall as a row where rows are given a height; a check at a row's end after its key, rather than between its label and its key; and a row of the app's own no wider than the menu, so it can cut itself short | `crates/component/src/menu/menu_item.rs`, `crates/component/src/menu/mod.rs`, `crates/component/src/menu/popup_menu.rs` | A menu set its 26px rows, the 4px around them, its 12px icons and the accent under the pointer inside itself, where nothing outside reaches, so the app could give its menus more room or a light of its own only by drawing a menu of its own. A check at the end sat between a label and its key, so it moved with the key. The app's menus have rows as tall as a large control and the row under the pointer washed as a borderless button is, as `docs/spec/components.md` says | Not offered yet: #1738 |
| `tooltip-style.patch` | `TooltipStyle`, a global that sets how every tooltip is drawn: a style that refines the tooltip's own before what's given to that tooltip, and a function that draws its key, each keeping the tooltip's own where it's `None`; and `Kbd::stroke`, which gives what a key binding presses | `crates/component/src/tooltip.rs`, `crates/component/src/kbd.rs` | A tooltip set its text size, shadow, room and key inside itself, where nothing outside reaches, and a button's tooltip, which the kit's button makes, couldn't be given a style at all. The app's tooltips are raised, with the one shadow, a detail's text and a widest of 320px, and write Escape as `Esc`, as `docs/spec/components.md` says | Not offered yet: #1812 |
| `a11y-disabled-ext.patch` | Makes `A11yDisabledExt` and `A11yDisabled` public, which mark a control's accessibility node disabled | `crates/base/src/a11y_disabled.rs`, `crates/base/src/lib.rs` | GPUI has no way to say an element is disabled, and the kit kept its own crate-private, so a control the app draws on an element of its own, such as a list's row, couldn't tell assistive apps it's off, as the kit's button does. The sidebar's Create is such a row, off while there's no project | Not offered yet: #1916 |
| `inline-object-fallback-line.patch` | An inline object drawn as its text is as tall as the flow's body line, snapped to device pixels, rather than `line_height_in_pixels`, which rounds to whole ones | `crates/base/src/text/inline_object.rs` | A custom node a plugin leaves as text, such as a link to a pull request the hub hasn't found, made its line half a pixel taller at 2x, so a reply that linked to one moved everything under it | Not offered yet |
| `markdown-link-node.patch` | A node an inline plugin makes of a link keeps the link's mark, so it's drawn and opened as that link, and its own Markdown is the link's words, around which the mark writes the link | `crates/base/src/text/format/markdown.rs` | A plugin that made a node of a link lost the link, so the node, drawn as its text, was neither coloured nor opened as one, and a copy as Markdown lost where it went. The transcript makes a node of a link to an issue or a pull request, to draw it as a chip, and leaves it the link while it's looked up | Not offered yet |
| `layout-direction-fmt.patch` | Formats `LogicalStyledExt`'s `logical_edge!` calls with `rustfmt` | `crates/base/src/layout_direction.rs` | `v0.10.0` shipped `layout_direction.rs` unformatted under upstream's `.rustfmt.toml`, which `cargo fmt --all --check` checks | Drops once upstream formats it |
| `core-text-22.patch` | `gpui-component` pins `core-text` 22.0.0 rather than 21.0.0 | `crates/component/Cargo.toml` | GPUI's copy of its text system, below, is on 22, which reads a font trait CoreText gives as a boolean. The kit pins it on macOS without using it, so the pin only keeps one `core-text` in the build | Drops once upstream's GPUI is on 22, as the kit follows it |

## GPUI's and Zed's crates

Three crates the apps build from crates.io are kept here as copies, each at the release the app's `Cargo.toml` asks for, which it puts in place of the published one under `[patch.crates-io]`:

- `gpui-pre-ios/`, the backend Zed publishes that runs GPUI on iOS. Only the iPhone app builds it.
- `gpui-pre-apple/`, GPUI's text system for Apple's systems, which both apps build.
- `zed-font-kit/`, Zed's fork of Servo's font-kit, which that text system finds and loads fonts with.

They're copied whole and changed in place, rather than patched as the kit is, since their changes are few and each goes once a release has it. Each is marked `Changed for Roundhouse` where it's made, with why. Taking a new release, as `tools/kit-sync` moves GPUI with the kit, means replacing each GPUI copy with the release's crate, and making again each change it doesn't have yet. The policy test fails while the app builds a release rather than a copy, which is what Cargo does, with only a warning, when the two versions differ.

### Our changes

| Change | Copy | Files | Why | Upstream |
|---|---|---|---|---|
| Asks the display link for 120 frames a second, and no fewer than 60, with `objc2-quartz-core`'s `CAFrameRateRange` | `gpui-pre-ios` | `Cargo.toml`, `src/ios/application.rs` | An iPhone runs a display link at 60 Hz unless it asks for more, so scrolling on a 120 Hz screen stuttered. 60 leaves the system room to slow it, such as in Low Power Mode | Not offered yet |
| An input that reports neither its editable range nor its length still says how much of an unbounded range it holds | `gpui-pre-ios` | `src/ios/text_input.rs` | gpui-kit's input reports neither, and with no document range iOS never shows the keyboard | Not offered yet |
| `core-text` 22 and the `core-graphics` 0.25 it takes, rather than 21 and 0.24 | `gpui-pre-apple`, `zed-font-kit` | `Cargo.toml` | CoreText gives a font's trait that's zero as a boolean on iOS, as macOS Ventura does for some fonts, and 21 panics reading it. 22 reads it as 0 or 1 | Zed's Renovate update to 22 waits for approval |
| A trait CoreText gives as a boolean counts as read | `gpui-pre-apple` | `src/text_system.rs` | The text system skipped any face whose traits weren't all numbers, to avoid 21's panic. Every face of Menlo has one on iOS, so none loaded, and with no font found GPUI stopped the app | Not offered yet |
