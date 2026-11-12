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
- Draw a block's text as one text where it can be. Each text in a row costs about 3 µs on every frame the row is on screen, so a command's output is drawn a line at a time, with space between, only up to 200 lines. Opened to 3,000 lines that way, a frame took about 13 ms, against about 4 ms as one text, and at 200 lines about 0.4 ms more than one text. Those are clock times on a Mac with a load average near 40, so they're a guide to the ratio rather than the cost.
- Scroll by anchor, with pinning, so jumping to an item doesn't drift.
- Measure a row when it's drawn, and keep its height. The transcript's list draws only the rows that show in the window, keeps each row's height as a guess once it's out of date, and measures the loaded rows out of sight a few to a frame. So a change of its width costs the rows in the window, where GPUI's list measures every row again in the frame its width changes. A loaded row costs about 40 µs to measure, most of it laying out text, which a release build barely shortens.
- Keep the blocks a text is drawn from only while it's drawn lately. The agent's recorded words are drawn from the kit's parsed Markdown blocks, which, with the kit's layout of them once they're drawn, take 18 to 23 bytes for each byte of the text. Kept for every loaded reply, 200 replies of about 3 KB came to 11.3 MB, and 200 of about 0.8 KB to 3.6 MB. The transcript keeps the blocks of the short texts drawn most recently, up to 128 KB of them, and what every loaded text shows, for a copy, which brought those to 4.3 MB and 3.0 MB. A short text drawn again is parsed again, at about 0.2 ms a KB, which is what the kit's text view did for one out of sight. A long one keeps its blocks, since it's parsed on another thread and would draw nothing meanwhile.
- Keep a sidebar at its whole width while it slides, and clip it, so nothing in it is laid out again as it moves. Its width stays the same all the way, even in a window too narrow for both sidebars, where the other one gives up or takes back room as it slides and is laid out again on each frame. What's beside it is laid out on every frame of the slide, which costs the rows in the window.
- Nothing in the window is cached, so every frame something moving asks for redraws the whole window. Whatever moves has to end, and then ask for no more frames.
- Parse terminal output off the main thread, with a short lock per frame.
- Use the kit's `Textarea` rather than writing one.
- Draw a diff in a list that knows its rows' heights. Every row of the file tab's diff is a line or a fold tall, apart from a comment's, so its list adds them up without drawing them, draws only the rows in the window, and keeps a comment's height by which comment it is. A change of its width costs only the comments in the window.
- Highlight with Oniguruma rather than `fancy-regex`, which takes several times the CPU on a large diff.
- Stop the caret blinking after 10 s idle, so an idle window draws nothing.
- Redraw a moving status mark on every frame of the display, and only while it's on screen and its agent works or its workspace is set up. Smoothness comes first until a measurement shows it costs too much. Nothing in the window is cached, so each of those frames redraws the whole window, and an idle window still draws nothing.

## Measuring

- Reading a big file a page at a time is measured as it's used, in each workspace's worker log, `instances/<instance>/worker.log` in the data directory. The worker says there how big each file it opens to page through is, how many lines it has, how much it streamed, whether it stopped before the end and how long it took, and for each read of its pages, which pages, up to which, and the same, each naming the file and the commit, so a read can be put with its file. It says so for any read over 100 ms too. A file small enough to send whole says so only when the log is asked for more, with `ROUNDHOUSE_LOG=debug`. Those lines say whether streaming each read from git keeps up, or the worker should keep an index of what it opens. *Unconfirmed.*
- A covered window looks exactly like a freeze, because macOS stops GPUI's frame source. Don't trust frame data without a window visibility log.
- Run timing tests one app at a time on an otherwise idle Mac. Test windows take focus, and the user's typing can land in them.
- When the Mac can't be idle, such as while other checkouts build, time a draw by the thread's CPU time, from `clock_gettime(CLOCK_THREAD_CPUTIME_ID)`, as well as by the clock. Time the thread waits for a core counts on the clock but not in CPU time. On a Mac with a load average over 100, frames whose CPU time had a p95 of 3 to 10 ms had a clock p95 of 50 to 190 ms. CPU time still rises under load, by about 1.4 times there, so compare only runs made under the same load.
- A debug build already has GPUI and the kit optimized, as `Cargo.toml` says, so it's a fair guide wherever the time goes to them, and a release build is only about a third faster there.
- A test's window draws as soon as something in it changes, and a real one on the display's schedule. So in a real window a turn of the wheel or a move of the scrollbar can land between a change and the frame that draws it, and in a test it never does. `cargo run -p desktop --example look -- <workspace> scroll:900` turns the wheel in a real window by itself, and `drag:2` drags the scrollbar's thumb, each printing where the list is after every step, which shows what a test can't. A real window draws nothing while the display sleeps, so on a Mac left alone run them under `caffeinate -d -u`.
- A headless window can be timed instead, with no window on screen. Timing `Window::draw` measures building, laying out, prepainting and painting on the main thread, and not presenting.
- In a test, GPUI draws a dirty window as soon as the update that dirtied it ends. A timed `Window::draw` has to run in the same update as the change, or it times a window with nothing to redraw.
- To measure what something holds, count the bytes allocated and freed with a counting global allocator in the UI tests' binary, run alone, and see how much letting go of it frees. That's how the cost of keeping texts' blocks was found, with the counter added for the run and taken out after.
- The kit's `render_frame` refreshes the whole window, ignoring every cache, so a frame drawn with it can't show what caching saves or what a missed `notify` leaves stale.
