# Why GPUI and gpui-kit

Why the desktop app is built on GPUI and gpui-kit, and why its transcript is built on the kit's views. How code that uses them is written is in `docs/gpui.md`.

## GPUI

The desktop app is native Rust on GPUI, Zed's UI framework, rather than a web view.

- **It meets the bars in `docs/spec/speed.md` at 120 Hz.** A web view started and opened diffs more slowly, and WKWebView caps animation near 60 Hz, with no public API to change that.
- **It's Rust end to end,** like the hub, the worker and `client-core`.
- **It can reach a phone.** GPUI has experimental iOS backends, so a mobile client can share the transcript with the desktop app.
- **It has AccessKit integration,** for VoiceOver.
- **Its own crates are Apache-2.0.** Zed's other crates are GPL-3.0, so they're out.

**What it costs:**

- Its mobile backends are experimental.
- It's pre-1.0, and the kit pins exact snapshots of it, so an upgrade can change the API.
- There's more code to own than with a web view. Neither GPUI nor the kit has a terminal, so the app's terminal builds on `alacritty_terminal`.

## gpui-kit

Longbridge's component kit for GPUI, used in their shipping Longbridge Pro app.

- **It has most of what the app needs:** messages, a message scroller, diffs, a sidebar, docks, a command palette, settings, native menus, variable-height virtual lists, a code editor, and Markdown.
- **It runs UI tests headless,** so the app's tests click and type without a window on screen.
- **It's Apache-2.0.**

## The transcript

The transcript is the kit's `TextView` in GPUI's virtual list, plus a small windowed element for large tool output, in `crates/transcript`.

The list measures every row that's loaded, not only those on screen, so the scrollbar goes by the session's real height. A list that measures only what's been on screen counts every other row as nothing, so in a long session the thumb reaches the top within a screen or two and shakes as rows are drawn, and no guess at the rest holds still either. A row not loaded yet counts as the average of the loaded ones, so the thumb moves a little when an older page arrives. The kit's `MessageScroller` keeps its list to itself, so the transcript lays out GPUI's list the way `MessageScroller` does instead. *Unconfirmed.*

The transcript keeps the text view's state for any message over 4 KB. The kit reads text that long on another thread and gives it no height until that's done, and keeps what it read only while the text is on screen, so the list measured a long message as a few pixels and scrolled past it in one turn of the wheel. *Unconfirmed.*

- **It meets every bar,** and on real content it nearly matches a custom renderer.
- **Giant rows are fine.** A windowed element of about 70 lines flings through a multi-megabyte row within the bars.
- **Markdown, tables, code highlighting, links and selection across messages come with it.** A custom renderer would also need bidi, accessibility and find.
- **It's the same code on iOS,** where `MessageScroller` has already run.

**What it costs:** at the extreme it's less smooth than a custom renderer, with an occasional frame over 25 ms, and it uses more memory and starts more slowly. The renderer sits behind `crates/transcript`, so replacing it touches nothing else.
