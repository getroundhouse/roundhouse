# The transcript draws its text from the kit's Markdown blocks and keeps the selection itself

**Status:** Chosen by an agent, built, and confirmed by the owner.
**First recorded:** 2026-11-12, in #669 and #670, for the agent's replies. The rest of the chat's Markdown followed by 2026-11-13, in #1032, and the plain text a call shows in #1129.

The transcript makes none of the kit's text views. Its Markdown, such as the agent's replies and the person's prompts, is parsed by the kit's Markdown parser and drawn by the kit's drawing of blocks, through `editor_host`, the layer under the kit's text view. The kit says where it drew each paragraph, heading, code block and table cell, and paints the ranges it's given behind them, which is how what's selected and what's found are drawn.

The transcript keeps the selection. Each end of it is a place in a text, which outlasts the text's row being drawn, and a copy reads the texts between the ends from their parses, drawn or not.

The plain text a call shows, such as a command and what it printed, is in the same selection. The transcript lays it out itself, as GPUI's text, and paints what's selected behind it.

`clippy.toml` disallows the kit's ways to make a text view and a selectable text. One made anywhere else in the app, where none of the faults below matters, says why with an `#[expect]`.

## Chosen over

- **The kit's text view.** A list that draws only what's in sight can't work with the first three of these, and nothing that shows an agent's words can work with the last:
  - It reads a long text on another thread, and gives the text no height until that's done.
  - It keeps what it read only while the text is on screen.
  - It keeps what's selected of a text only while the text is painted.
  - It ends the app on a text its Markdown parser panics on.
- **The kit's selection,** which its text view and its selectable text share. It belongs to the window and covers only what was painted in a frame. It lets go once the text holding either end of a selection isn't painted, which in the transcript's list is as soon as that text's row leaves the window. So a drag past the window's edge stopped and lost what it had selected, and a copy held only what was in sight.
- **The kit's code blocks, for plain text.** They would bring the kit's geometry and its painting of what's selected. But two things a call shows can't be the kit's blocks: output over 16 KB, which an element of the transcript's own draws a line at a time as it comes into sight, and a file's numbered lines, with their gutter and tints. The transcript has to lay those out itself, so it lays out all its plain text one way.

## What it costs

The selection takes a mouse's press, drag and release, and Shift with an arrow key, which moves the end that moved last. It doesn't take a touch screen's long press and handles, which the kit's selection does.
