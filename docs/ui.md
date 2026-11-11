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

The transcript is the kit's `TextView` in a list of its own, plus a small windowed element for large tool output, in `crates/transcript`.

The list is the transcript's own, rather than GPUI's, and is built to four rules, so that a long session costs no more to show than a short one:

- **Cost follows what's being looked at, never the length of the session.** Only the rows that show in the window are drawn. A row out of it is drawn only to find a height that isn't known, and one known to show nothing isn't drawn to lay the window out, at any width, since most of a session's rows show nothing.
- **A place in the transcript is a row and how far into it, not a distance down the session,** so nothing that changes a height out of sight moves what's in sight. The wheel moves the place by as much as it turns, and the next frame finds which row that's in, from the heights of the rows it passes. If the row at the top gets shorter than the list is far into it, such as a block someone closes, the list is at that row's end.
- **A row's height is kept, not worked out again.** It's exact for a row measured at the width the transcript is, and a guess for any other: the height it had at another width, or for a row never measured, what the measured ones come to on average. The scrollbar goes by all of them, so it reads the whole session without any of it being measured for that. A guess is put right without holding up a frame: the loaded rows out of sight are measured a few to a frame, so the scrollbar settles on the session's height a moment after a page loads or the width changes. None are measured while the width is changing on every frame, as a sidebar slides or the window is resized, since each would be out of date in the next. A page of history that loads or is let go puts every height out of date, as a change of width does, since what a row shows can depend on another being loaded: an event of a call is drawn in its call's row, or where it is once that row is gone.
- **What the list holds and what it has measured change together,** since it measures what it lays out, in the frame it lays it out. A list that's told of rows in one step and measures them in a later one takes them to have no height in between, and a turn of the wheel that lands there goes to the wrong place.

GPUI's list can't hold to them. It measures every row, or only those on screen. Measuring only those on screen counts every other row as nothing, so in a long session the thumb reaches the top within a screen or two and shakes as rows are drawn. Measuring every row keeps the scrollbar right, but the list forgets every height when its width changes at all, and measures every row again in that frame, so each frame of resizing the window cost a measure of the whole session. It takes no height it's given, and keeps none as a guess.

The kit's scrollbar and its bounce at the list's ends work on anything that says how far it's scrolled and how tall it is, so the transcript's list uses both. While the scrollbar's thumb is dragged, the scrollbar goes by every height as it was when the drag began, so a place on its track is the same row however long the pointer rests there, while rows load under the thumb and turn out other heights.

The transcript keeps the text view's state for any text over 4 KB that a row draws, and for the text the agent is still writing, from its first word. The kit reads text that long on another thread and gives it no height until that's done, and keeps what it read only while the text is on screen, so the list measured a long text's row as if the text weren't there, and when the row first showed at the top of the window, the text grew there and took the window up to its start. Every text view in the transcript is made in one place, `LongTexts`, which keeps a long text from the first time its row is drawn. That's soon after the row loads, since the list measures loaded rows out of sight, and the row is measured again as the text is read. So nothing has to say ahead of time which texts a row shows, and `clippy.toml` disallows the kit's own ways to make a text view anywhere else. A text view outside the transcript, where nothing keeps a height, says why with an `#[expect]`. *Unconfirmed.*

- **It meets every bar,** and on real content it nearly matches a custom renderer.
- **Giant rows are fine.** A windowed element of about 70 lines flings through a multi-megabyte row within the bars.
- **Markdown, tables, code highlighting, links and selection across messages come with it.** A custom renderer would also need bidi, accessibility and find.
- **Its text views are the same code on iOS,** where they've already run. The list is plain GPUI, and hasn't run there.

**What it costs:** at the extreme it's less smooth than a custom renderer, with an occasional frame over 25 ms, and it uses more memory and starts more slowly. Following the latest row, and where the wheel and the scrollbar take the list, are the transcript's own code to keep right. The renderer sits behind `crates/transcript`, so replacing it touches nothing else.
