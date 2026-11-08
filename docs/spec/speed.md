# Speed

How fast the desktop app has to be. How to measure it, the test sizes, and what helps, are in `docs/performance.md`. What can't be measured is in `docs/manual-checks.md`.

*Unconfirmed:* every bar here.

| Test | What it does | Bar |
|---|---|---|
| Open | Opens a session from empty, at the tail, until real content paints | p95 at or under 500 ms for every size, with 100 ms of latency |
| Scroll | Flings up, jumps to items, flings back down | The frame bar below, and at most 1 px of anchor drift on jumps |
| Parallel | Streams into the visible session and background ones, while scrolling, following the tail and typing | The frame bar, and input p95 at or under 50 ms |
| Diff | Opens a 20,000-line diff, scrolls it, adds comments, scrolls again | The frame bar, with visible lines highlighted within 250 ms of scrolling stopping |
| Terminal | Sends a burst of output to a terminal beside the transcript, while typing | p95 frame at or under 33 ms, no frame over 100 ms, and all bytes processed |
| Resources | Keeps sessions open, flings through them, then sits idle | Recorded, with no pass or fail |

**The frame bar:** across phases, p99 frame interval at or under 18 ms, at most 1% of frames over 25 ms, and no frame over 250 ms. It's 18 ms rather than 16.7 because presentation timestamps jitter around vsync, so an app hitting every 60 Hz frame would otherwise fail on noise.
