# Performance

What the desktop app has to hit, how to measure it, and what helps it get there.

## Bars

| Test | What it does | Bar |
|---|---|---|
| Open | Opens a session from empty, at the tail, until real content paints | p95 at or under 500 ms for every size, with 100 ms of latency |
| Scroll | Flings up, jumps to items, flings back down | The frame bar below, and at most 1 px of anchor drift on jumps |
| Parallel | Streams into the visible session and background ones, while scrolling, following the tail and typing | The frame bar, and input p95 at or under 50 ms |
| Diff | Opens a 20,000-line diff, scrolls it, adds comments, scrolls again | The frame bar, with visible lines highlighted within 250 ms of scrolling stopping |
| Terminal | Sends a burst of output to a terminal beside the transcript, while typing | p95 frame at or under 33 ms, no frame over 100 ms, and all bytes processed |
| Resources | Keeps sessions open, flings through them, then sits idle | Recorded, with no pass or fail |

**The frame bar:** across phases, p99 frame interval at or under 18 ms, at most 1% of frames over 25 ms, and no frame over 250 ms. It's 18 ms rather than 16.7 because presentation timestamps jitter around vsync, so an app hitting every 60 Hz frame would otherwise fail on noise.

What can't be measured is in `docs/manual-checks.md`.

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
- Cache views per region.
- Scroll by anchor, with pinning, so jumping to an item doesn't drift.
- Parse terminal output off the main thread, with a short lock per frame.
- Use the kit's `Diff` and `Textarea` rather than writing those.
- Highlight with Oniguruma rather than `fancy-regex`, which takes several times the CPU on a large diff.
- Stop the caret blinking after 10 s idle, so an idle window draws nothing.

## Measuring

- A covered window looks exactly like a freeze, because macOS stops GPUI's frame source. Don't trust frame data without a window visibility log.
- Run timing tests one app at a time on an otherwise idle Mac. Test windows take focus, and the user's typing can land in them.
