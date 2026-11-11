# Performance

How to measure the desktop app's speed against the bars in `docs/spec/speed.md`, and what helps it meet them.

## Test sizes

| Size | Items | Why |
|---|---|---|
| `s` | 500 | Typical |
| `m` | 6,000 | The longest local session |
| `l` | 15,000 | The longest cloud session |
| `xl` | 50,000 | Headroom, at about 3.5x the real maximum |

Bars are decided through `l`, and `xl` is only reported. The sizes come from real use, which is heavy tailed: a worst case is about 15,000 items with a few over 1 MB, and tool calls and command output make up most of a transcript. `mobile-poc/test-server/gen.ts` generates sessions of these sizes.

## What helps

- Do layout off the main thread, and shape only what's on screen.
- Cache views per region. In the pinned GPUI, a cached view that renders again renders everything inside it, cached views included, so only the outermost boundary saves work.
- Keep any list that can grow without limit virtual, as the sidebar's is. Each frame a moving mark asks for rebuilds the whole view it's drawn in, every row of a list in it included, and in a list that isn't virtual, even a mark scrolled out of sight is drawn and asks for frames.
- That includes what a row opens to. A row of a virtual list is drawn whole, so one that opens to something long draws all of it on every frame it's on screen. An opened subagent of 500 calls, drawn inside its call's row, took about 17 ms a frame, against about 1.3 ms closed.
- Leave out of a list what isn't shown, rather than putting it in as rows that draw nothing. Rows with no height take no room, so the list draws every one of them whenever it draws the row beside them. A subagent's 1,100 events, left in the list as empty rows under its closed call, raised the p95 frame from about 1.7 ms to 7 ms.
- Scroll by anchor, with pinning, so jumping to an item doesn't drift.
- Measure a row when it's drawn, and keep its height. The transcript's list draws only the rows that show in the window, keeps each row's height as a guess once it's out of date, and measures the loaded rows out of sight a few to a frame. So a change of its width costs the rows in the window, where GPUI's list measures every row again in the frame its width changes. A loaded row costs about 40 µs to measure, most of it laying out text, which a release build barely shortens.
- Parse terminal output off the main thread, with a short lock per frame.
- Use the kit's `Diff` and `Textarea` rather than writing those.
- Highlight with Oniguruma rather than `fancy-regex`, which takes several times the CPU on a large diff.
- Stop the caret blinking after 10 s idle, so an idle window draws nothing.
- Redraw a moving status mark on every frame of the display, and only while it's on screen and its agent works or its workspace is set up. Smoothness comes first until a measurement shows it costs too much. Nothing in the window is cached, so each of those frames redraws the whole window, and an idle window still draws nothing.

## Measuring

- A covered window looks exactly like a freeze, because macOS stops GPUI's frame source. Don't trust frame data without a window visibility log.
- Run timing tests one app at a time on an otherwise idle Mac. Test windows take focus, and the user's typing can land in them.
- When the Mac can't be idle, such as while other checkouts build, time a draw by the thread's CPU time, from `clock_gettime(CLOCK_THREAD_CPUTIME_ID)`, as well as by the clock. Time the thread waits for a core counts on the clock but not in CPU time. On a Mac with a load average over 100, frames whose CPU time had a p95 of 3 to 10 ms had a clock p95 of 50 to 190 ms. CPU time still rises under load, by about 1.4 times there, so compare only runs made under the same load.
- A debug build already has GPUI and the kit optimized, as `Cargo.toml` says, so it's a fair guide wherever the time goes to them, and a release build is only about a third faster there.
- A test's window draws as soon as something in it changes, and a real one on the display's schedule. So in a real window a turn of the wheel or a move of the scrollbar can land between a change and the frame that draws it, and in a test it never does. `cargo run -p desktop --example look -- <workspace> scroll:900` turns the wheel in a real window by itself, and `drag:2` drags the scrollbar's thumb, each printing where the list is after every step, which shows what a test can't. A real window draws nothing while the display sleeps, so on a Mac left alone run them under `caffeinate -d -u`.
- A headless window can be timed instead, with no window on screen. Timing `Window::draw` measures building, laying out, prepainting and painting on the main thread, and not presenting.
- In a test, GPUI draws a dirty window as soon as the update that dirtied it ends. A timed `Window::draw` has to run in the same update as the change, or it times a window with nothing to redraw.
- The kit's `render_frame` refreshes the whole window, ignoring every cache, so a frame drawn with it can't show what caching saves or what a missed `notify` leaves stale.
