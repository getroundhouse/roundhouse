# The worker holds the only copy of a terminal, and clients show the rows it sends

**Status:** Decided, and built.
**First recorded:** 2026-11-21, in #2129.

Each terminal's worker keeps the only copy of what it shows, in Ghostty's terminal core, and numbers its rows, history and screen together, so a number never names another row. While a client watches, the worker sends the hub pictures of the screen, each with only what changed, and the hub keeps the rows of each watched screen and sends each client what it lacks of the newest, one frame at a time. A client keeps the rows near its view, asks the worker for the rest of the history, a word's ends and a selection's text, and sends what's pressed and the size its view has room for as they come. Rows go as JSON, as every message does. A terminal has at most 600 columns and 200 rows. No client builds Ghostty.

## Chosen over

- **A copy of Ghostty in each client, sent every byte the program wrote,** as terminals were first built. Every client took in a flood whole, over its link, and one that fell behind was sent the whole history again, about 1 MiB of a build's log at 200 columns, and up to 39.5 MiB with a colour in every cell. Every client, the iPhone app too, built Ghostty with Zig.
- **A copy of Ghostty in each client, with history sent when it's scrolled to.** The copy still has to take in every byte to stay right, and Ghostty can't add history above what a copy holds.
- **A stream the worker makes for each client,** rather than the hub holding the screen. Each client's pace would have to reach the worker, and the worker's connection would carry the same rows once for each client.
- **Rows packed as bytes, in base64.** About a third the size with a colour in every cell, but with its own rules for growing and its own tests, where JSON follows the rules every message does.

## What it costs

- A double or a triple click's highlight, scrolling past the rows a client holds, copying, and resizing while scrolled back each wait a round trip.
- With no connection, the wheel reaches only the rows a client holds, and nothing can be copied.
- The worker does the work for every client, and the hub holds a screen for each watched terminal.
- The requests that sent a terminal's bytes were removed without a new version, so a client, hub or worker from before can't show a terminal with a newer peer, as `docs/principles/versions.md` says.
