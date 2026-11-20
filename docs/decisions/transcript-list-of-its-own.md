# The transcript's rows are in a list of its own, not GPUI's

**Status:** Decided.
**First recorded:** 2026-11-11, in #530.

The transcript draws its rows in a list of its own, `transcript::list`, built from GPUI's elements, so that a long session costs no more to show than a short one. The list is built to four rules:

- **Cost follows what's being looked at, never the length of the session.** Only the rows that show in the window are drawn. A row out of the window is drawn only to find a height that isn't known.
- **A place in the transcript is a row and how far into it, not a distance down the session.** So nothing that changes a height out of sight moves what's in sight.
- **A row's height is kept, not worked out again.** It's exact for a row measured at the transcript's width, and a guess for any other. The scrollbar goes by all of them, so it covers the whole session without any row being measured for it.
- **What the list holds and what it has measured change together.** The list measures a row in the frame that lays the row out. A list that's told of rows in one step and measures them in a later one takes them to have no height in between, and a turn of the wheel that lands there goes to the wrong place.

The list still uses the kit's scrollbar and its bounce at a list's ends. Both work on anything that says how far it's scrolled and how tall it is.

## Chosen over

- **GPUI's list.** It measures every row, or only the rows on screen. It takes no height it's given, and keeps none as a guess.
  - Measuring only the rows on screen counts every other row as no height. In a long session the scrollbar's thumb then reaches the top within a screen or two, and shakes as rows are drawn.
  - Measuring every row keeps the scrollbar right. But the list forgets every height when its width changes at all, and measures every row again in that frame. So each frame of resizing the window cost a measure of the whole session.

## What it costs

Following the latest row, and where the wheel and the scrollbar take the list, are the transcript's own code to keep right.
