# The transcript is drawn with GPUI and gpui-kit, not by a renderer of its own

**Status:** Decided.
**First recorded:** 2026-11-06, in #78, as the kit's text view in a list.

A chat's transcript is built from GPUI's elements and the kit's Markdown, in `crates/transcript`. Its rows are in a list, its Markdown is parsed and drawn by the kit, and tool output too long to draw whole is drawn by a small element that shapes only the lines in sight.

- **It meets every bar in `docs/spec/speed.md`,** and on real content it nearly matches a renderer of our own.
- **A very large row is fine.** The element for long tool output flings through a row of several megabytes within the bars.
- **Markdown, tables, code highlighting and links come with the kit,** from its parser and its drawing of blocks. *Unconfirmed.*

Two parts of it are no longer GPUI's or the kit's own views, and each has a record: [the list is the transcript's own](transcript-list-of-its-own.md), and [the transcript draws its text from the kit's Markdown blocks and keeps the selection itself](transcript-keeps-its-texts.md).

## Chosen over

- **A renderer of our own.** It would also need bidi and accessibility. *Unconfirmed.*

## What it costs

- At the extreme it's less smooth than a renderer of our own, with an occasional frame over 25 ms.
- It uses more memory and starts more slowly.

The renderer is behind `crates/transcript`, so replacing it changes nothing else.
