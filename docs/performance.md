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
- Scroll by anchor, with pinning, so jumping to an item doesn't drift.
- Parse terminal output off the main thread, with a short lock per frame.
- Use the kit's `Diff` and `Textarea` rather than writing those.
- Highlight with Oniguruma rather than `fancy-regex`, which takes several times the CPU on a large diff.
- Stop the caret blinking after 10 s idle, so an idle window draws nothing.
- Redraw a moving status mark on every frame of the display, and only while it's on screen and its agent works or its workspace is set up. Smoothness comes first until a measurement shows it costs too much. Nothing in the window is cached, so each of those frames redraws the whole window, and an idle window still draws nothing.

## Measuring

- A covered window looks exactly like a freeze, because macOS stops GPUI's frame source. Don't trust frame data without a window visibility log.
- Run timing tests one app at a time on an otherwise idle Mac. Test windows take focus, and the user's typing can land in them.
- A headless window can be timed instead, with no window on screen. Timing `Window::draw` measures building, laying out, prepainting and painting on the main thread, and not presenting.
- In a test, GPUI draws a dirty window as soon as the update that dirtied it ends. A timed `Window::draw` has to run in the same update as the change, or it times a window with nothing to redraw.
- The kit's `render_frame` refreshes the whole window, ignoring every cache, so a frame drawn with it can't show what caching saves or what a missed `notify` leaves stale.
