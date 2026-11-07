# Performance

What the desktop app has to hit, how to measure it, and what's known about getting there.

**Nothing measures the real app yet.** These bars were written for prototypes that have since been deleted, along with the harness that ran them. The next step is new tests against the app itself, in #50, and speed targets, in #64.

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

Bars are decided through `l`, and `xl` is only reported. The shape comes from the measurements in `docs/usage.md`. `mobile-poc/test-server/gen.ts` generates sessions of these sizes.

## What made the prototypes fast

- Doing all layout off the main thread, and shaping only what's on screen.
- Caching views per region.
- An anchor-based scroll model with pinning. Drift was 0 px on all 100 jumps.
- Parsing terminal output off the main thread, with a short lock per frame.
- Using the kit's `Diff` and `Textarea` rather than writing those.
- Oniguruma rather than `fancy-regex`, which took highlighting a large diff from 12.2 s of CPU to 2.6 s.
- A caret that stops blinking after 10 s idle. Blinking forever cost 29 draws per 15 s, and this brought it to 0.

## Measuring

- A covered window looks exactly like a freeze, because macOS stops GPUI's frame source. Don't trust frame data without a window visibility log.
- Run timing tests one app at a time on an otherwise idle Mac. Test windows take focus, and the user's typing can land in them.
- Flags that exist only for tests must stay out of the product.
- Only one M4 Max at 120 Hz has been tested so far.
