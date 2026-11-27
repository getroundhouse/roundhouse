# Brilliant

How Brilliant, the design tool, behaves where the design depends on it, as found by experiment with version 1.0.0.120 of its Mac app and 0.1.11 of `@brilliant-hq/mcp`, or as its own docs say where that's noted. `docs/standards/design.md` has how it's used.

## Reaching it

- An agent reaches it through `npx -y @brilliant-hq/mcp`, over the app's server on `127.0.0.1:3333`, or a tab of `brilliant.design`.
- The bridge stays with what it first connected to. A tab with a project open comes before the app, by its code. Once every such tab is closed, the next call reaches the app, with no restart.
- A tab's project can be public, and what's drawn in it is live at once. An account on the free plan publishes publicly unless told otherwise, by its docs.
- `init` takes a project's path. A call that names a project's path opens it, without `init`. A worktree's path is one.
- An edit in a session bound to no project can be refused, when it would make a canvas.
- Each `init` returns the whole design system and manual, which is long, so an agent calls it once.
- An edit of several blocks applies those before the one that fails.
- The app refuses to write an export into a hidden folder, such as `.context`. It writes into the project's `Exports/`.
- A video, a UI capture and an edit read from a file need the app rather than a tab, by its docs. `render_ui` is refused in a tab.

## Files

- A project is a folder. A canvas is a `.bl` file, a design system a `.ds` in `Styles/`, and a film a `.bm` in a `Motion/` folder, all text. A film's `Motion/` can be at the project's root or in the folder of the canvas it draws from, such as `Screens/Motion/`, which names that canvas by its name alone or by its path. One at the root names a canvas in a folder by its path, such as `Screens/Frames`, and is refused naming it by its name alone. A film anywhere else, such as beside its canvas with no `Motion/`, or in `Screens/Films/`, isn't a film to Brilliant: `edit` refuses to write one there, and `export` to play it. `edit` refused a film in `Screens/Desktop/Chats/Motion/` that named its canvas by its name alone, which `export` played, and wrote it once it named the canvas by its path.
- A row of a `.bl` is one element, led by an ID it keeps. A copy of a component is one row naming its master, but for a copy on a canvas edited since its master changed, which also holds the master's layout and children.
- A canvas written by a program needs no position or size that Brilliant measures, such as `s(hug,hug)` in place of `s(hug:62.84,hug:29)`, nor a token's value beside its name, such as `tok(color.text)`, nor a text's font when it has a type's token, such as `t("Index") $typography.PageTitle`. Brilliant lays it out and draws it from the design system when it reads it, and didn't write it again.
- A row holds what was measured, such as `s(360,hug:132)`, and a token's value beside its name, such as `tok(color.primary,#0080FF)`. Changing a token or a master doesn't rewrite the canvases that use it until each is next edited.
- A line height given to a type's token, as `lineHeight: font.lineHeight.Detail` in `typography.Detail`, didn't reach a text on a canvas open in the app that already had the token: it kept Brilliant's own lines until its row took the token again, as `<id> $typography.Detail`. A canvas opened after the change drew the new lines, and so did an export of it, but its file kept the old ones. An edit that leaves a row as it was doesn't write the file, even renaming an element and back in one edit, so a canvas's file took the new lines once one edit renamed an element and the next renamed it back.
- A project is named by its folder, so each workspace's is "design". A name in `.brilliant/settings.json` is ignored, a symlink to the folder is followed to it, and links inside a project aren't. Opening the whole worktree takes the worktree's name, but writes a design system and a `.gitignore` there, and components' paths no longer resolve. So the setup script in `.conductor/settings.toml` makes a folder such as `@ sants-2` in `design/`, which is first in Brilliant's list of files, and which git ignores.
- A design system edited on disk, outside Brilliant, was read again: a canvas's next edit took a token added there by hand.
- `.brilliant/`, `Styles/.gen/` and each `Assets/.thumbs/` are made again. A thumbnail changes with every edit, and conflicts in a merge.
- `execute_commands` deletes a canvas and its file with `delete_canvas` and `{"canvasId": "<path>"}`, and refuses a `path`. Its folder's empty `Assets/` stays.

## The design system's pages

- A token's page comes from its prefix: a `boldness(color(…))` is on Colours, as a palette of nine steps, and `color.` names roles. A `color(#e9b24c)` with no `boldness` is on Colours as one colour with no ramp, and a role that names it, such as `color.status.done: green`, follows it. A plain `amber: #e9b24c` isn't on Colours. `spacing.`, `radius.`, `stroke.width.` and `font.` are the scales. A number under any other prefix, such as `size.` or `motion.`, is on Scales too, under Other numbers, in the order of the file. The file Brilliant makes in `Styles/.gen/` heads it Opacity, which no page does.
- Roles is divided by the word after `color.`, so `color.pane` is a section of its own and `color.surface.pane` is in one with the other surfaces. The heading is that word with a capital, so `pull_request` was headed "Pull_request" and `PR` is headed "PR". A name can't hold a space, and a token refuses `$label`, `$name`, `$title` and `$group`. It takes `$description`, and where that shows wasn't looked into. A step of a palette set by its number is listed there under Other. A colour with an opacity is drawn with it there, and written without it.
- Scales puts a number under its heading only when it's a step of a scale, such as `spacing: tshirt(number([4, 8]))`, whose steps are named `xs`, `sm` and on. A number of its own, such as `spacing.Chip`, was listed under Other numbers. Brilliant makes more steps of a scale of corners or line widths than it's given, such as `radius.xl`, and neither `unset` nor `hide` takes them off the page.
- Editing canvas files on disk switches the app's window to one of them, and editing one with `edit` didn't. `open_file` opens the design system's file at Colours, and no command was found that opens its other pages.
- A palette's colour makes nine steps, `hint` to `intense`, of which the middle, `mid`, is the colour. The app pins one to a colour of its own as `grey.soft { $default: #201b18 }`, and marks it on Colours. The eleven numbered steps, such as `grey.400`, are still made from the palette's colour, and aren't what a pinned step is.
- A role that names a palette's step follows it. A canvas's drawing followed a changed step, and a canvas file changed on disk was read again.

## Modes

- A design system can declare an axis of its own, as `modes { reach: [desk, handheld] }`, and a token a value for each of its values, as `font.size.Body { $default: 14, reach.handheld: 17 }`. A number, a size, a line height and a token that names a step of space, as `spacing.control.Large { $default: spacing.lg, reach.handheld: spacing.xl }`, each took the handheld's value in a frame set to it. A type's token built on them, such as `typography.Body`, followed them, its letter spacing too, since that's a share of its size.
- A frame takes a mode with `ds(, reach(handheld))`, and what's in it follows. A copy of a component drawn in a frame of the default mode, on its canvas or on another, took the handheld's sizes in such a frame: a large control 44 tall rather than 32, and as wide as the one drawn there.
- A canvas takes a mode with `active { modes: { reach: handheld } }` in a `ds` fence, which writes it in the canvas's header and leaves the design system's file as it was. Switched after its elements were drawn, the canvas drew its text at the handheld's size in boxes still measured at the desk's, and laying it out again didn't change that. Opened afresh in another project, it was laid out at the handheld's sizes.
- Brilliant's docs give no guidance for designing for several devices, and name no breakpoints. Their example of an axis of one's own is a `platform` one, and they say a switch of mode resolves colours, opacity, font families and sizes again. How the design system's pages show a token with a value for each mode wasn't looked at.

## Drawing what the app draws

- A fill draws a role at a strength that's a token of its own, as `f[(solid($color.status.needs_you,o($strength.XSoft)))]` with `strength.XSoft: 0.09` in the design system, with the same pixels as a colour token with that opacity, `#e9b24c17`. A token under `visibility.` did the same, and so did a stroke, `st[(solid($color.x,o($strength.y)),w(…))]`, and a span of text, `spans[("MM",solid($color.x,o($strength.y)))]`, each within a level of the colour worked out. Written as `f[($color.x,o($y))]`, without `solid`, the opacity is dropped, and said so. A colour given as `rgba()` was refused as unknown.
- An edit in the `default` design system refuses a bare number in `o()` and a bare colour, and one in `none` refuses every token. So a value no token holds yet, such as an option drawn to decide, goes in an edit of its own in `none`, on rows drawn first in `default`. Each edit names its design system in `edit`'s `designSystem`.
- `tools/boards/board.py` writes a foundation's board's rows in the design system's tokens: text by its kind, its sections, its rail, and an open question's own board, with its options side by side. It writes the edit in `none` for the values no token holds, so a board's options are drawn in two edits, in one session.
- A fill of eight hex digits is taken as CSS's `#rrggbbaa` and written as `solid(#rrggbb,o(N))`, with a note that Brilliant's own order is `#aarrggbb`.
- A drop shadow's blur is twice the number the app gives GPUI for it: at 24 and 6, where the app has 12 and 3, the gallery's own shadow was matched at every point sampled.
- A shadow is its token's name alone on a row, as `$shadow.Raised`, and the row is written with each of its layers beside it, as `shadow(o(0.22),y(8),blur(24))`. `shadow.Raised`'s layers, which are black, named no colour, so `color.edge.shadow` is on a canvas only where it fills something. `fx[…]` and `e[…]` aren't properties: a row with one was applied without it, and a note said so.
- A gradient is a fill, as `f[(linear(180,$color.surface.icon_tile_top,$color.surface.pane))]`, written as a stop for each colour. At 180 the first is at the top and the last at the bottom.
- A size can be a token, as `s($size.icon.Large,$size.icon.Large)` or `s(fill,$size.row.Small)`, kept as `sref(…)` in the element's row. It takes a token of space or size, and refuses a line's width, as `s($stroke.width.QuoteRule,40)`, so a rule as wide as a width token is drawn as a vector line, `v(nodes[…],edges[…]) st[($color.x,w($stroke.width.QuoteRule))]`. A film's times can't be, by its docs, so each is a number. An icon made with `svg(icon:…)` and sized as `s($size.icon.Regular,$size.icon.Regular)` was drawn 208 square, and a number sized it.
- A stroke takes no room, where the app's edge does, so what has an edge is inset a pixel more.
- Text keeps the fraction of a pixel it measures, where the app rounds its width up to a whole one, so a button, a chip or a pill that hugs its words is up to a pixel narrower. GPUI does the rounding, in `elements/text.rs`, and it rounds what it measures up to a device pixel after that in any case, in `taffy.rs`. Brilliant's snapping to the pixel grid is for moving, resizing and making an element, and no setting was found that rounds a text's own width.
- A colour on an inline `svg()` is a token once its stroke is written with a width, such as `st[($color.x,w($edge.width))]`. A colour in the SVG's own text is a plain value that no test of the tokens sees.
- A text set by its font's tokens one by one, with no type's token, keeps each of them in its row, as `t("A",SF Pro Display:$font.family.title,24:$font.size.PageTitle,m:$font.weight.emphasis,lh(1.6071:$font.lineHeight.Body),ls(0.324px:$font.letterSpacing.PageTitle))`. With a type's token as well, such as `$typography.Strong`, the row kept the type's token and none of the font's.
- An inline SVG is fitted to its size by the middle of its lines, with neither its view box nor half a line's thickness around it. So an icon is as the app has it when it's placed and sized by what its lines' middles span of the view box. A size that counts the thickness draws it bigger, by a tenth for a file's icon. One icon's flat line was first measured as having a height and later as having none, for no reason found, so check the size an icon was given. Four icons placed so landed within a device pixel of the app's, two of them on the same pixels.
- Text in SF Pro Text sits about a third of a point higher than the app's, in a line of the same height, since the system font the app draws with has other metrics. Text drawn by an export has a few lone faint pixels near its letters, which a comparison of pixels has to ignore.
- No way was found to align on a baseline, so a line with code in it has the code's box placed by its distance from the line's top: 4.63 points, in a line of 22.5, and the box is 14.88 tall, as the kit works it out from Menlo's metrics in `code_background`.
- `delete()` takes one ID to a row. A list of them is taken as one name, and answered as already absent.
- `delete()` is a row of its own. Written among a row's properties, as `<id> delete()`, it deleted nothing, and nothing said so.
- A row that's only an ID and `parent(<id>)` moves that element into another, after its children.
- In a text, `\n` is a line break, and `\\n` is the two characters.
- A frame laid out with `x(sb)` sets a lone child in its middle.
- A file sent with `edit` again within two minutes answers the first run and changes nothing. A `#ref` from an earlier call still names its element, and a row that names it changes that element.
- A copy of a component in another canvas keeps the old master's row in its file until that canvas is next edited.

## Merging

- Canvases that each of two branches adds merge cleanly in git. Changes to one canvas conflict when their rows are next to each other, such as two frames added at its end, and so do neighbouring statements of a `.ds`. So do an item each branch adds at the same place in a list, two changes to one element, and deleting an element that the other branch changed something in.
- A three-way merge by row and by property, tried with a script that wasn't kept, merged frames added at the same place cleanly and still found one text changed on both sides. It also kept both of two elements added at the same place with the same name, and said nothing.
- A merged canvas keeps the sizes it had on one side when it's opened. Changing an element's layout and back lays it out again.
- A canvas that uses a token its design system no longer has is drawn with the value stored in the row, and nothing says so. Only a new row that uses it is refused.
- A change to a component's master reaches a copy on a canvas that another branch added, once they're merged.

## Moving a canvas

- A canvas moved or renamed in Brilliant, with `move_canvas` or `rename_canvas`, which takes `canvasId` and `newName`, the name without its folder, has every copy of its components on every canvas pointed at its new path, as `inst("Status Ready for review",canvas(Board/Status))`, in the files on disk. The moved file is otherwise unchanged. Some copies were written with their master's layout and fill beside them, as a copy on a canvas edited since its master changed is.
- A master renamed with `edit` had every copy's row renamed with it, in each file, in a project open in the app. In a project open only in the background, the copies kept the old name until their rows were renamed by hand, after which they drew the new master. A copy's body stored beside its row kept the old words, and didn't show them.
- A film isn't pointed at a canvas that moved: its `canvases[…]` keeps the old path.
- A copy that names a path no canvas has draws nothing, and nothing says so, though its row names its master's ID. `read` shows the row as it is, with no warning. A canvas added on a branch cut before a move is one once they're merged: in a project opened afresh, its copy naming the old path drew nothing, and the same copy naming the new path drew. A film that names a path no canvas has is refused, with the path it named.
- Until the app quits, what names a path a canvas was moved from in Brilliant can still find it, so a check of a move is made in a project opened afresh.

## Fonts

- `.AppleSystemUIFont` is drawn in Inter.
- `SF Pro`, with Apple's fonts not installed, is drawn in San Francisco about 10% narrower at 14px than macOS draws it. Its widths are those of the font at its default optical size, which is for headings.
- `SF Pro Text`, with Apple's fonts installed, is drawn at the font's own widths. At 12px those are the system font's. At 14px macOS sets the system font 0.15px a letter tighter, at 16px 0.31px, and at 20px 0.72px. With `ls(-0.15)` at 14px, a line of 32 letters in regular, medium and semibold is within 0.05px of the width macOS gives it.
- The system font is San Francisco too, built into macOS as `SFNS.ttf`, where Apple's download is `SF Pro Text`. In a line of 22.5 points at 14px, its baseline is at 16.54 and SF Pro Text's at 16.23.
- Brilliant fills letters' outlines as they are. Measured on the plan screen, by how much of a line is ink, its text is within 1% of them. macOS thickens the letters it draws unless it's told not to, by 31% for the app's light text and 17% for its dark text, which is why the app's was heavier until `gpui-pre-apple` had it not to.
- An edit's note that a font isn't available appears for `SF Pro Text` and `SF Mono` even where they're drawn.
- A weight drawn before its file arrived stays in the weight that stood in for it until the app restarts, and text measured meanwhile keeps that size after. Settings' Fonts page lists families, so it doesn't show this.
- `SF Pro Display` at 24px, with 0.324px a letter, is as wide as the system font macOS draws, as the gallery's page title is, where `SF Pro Text` isn't. At 10px the system font is 0.117px a letter wider than `SF Pro Text`, and at 7.5px 0.22px.
- `font:` in an HTML block is dropped with no note. `font-family`, `font-size` and `font-weight` are kept.
- `convert_html`, and an edit's answer, can give a text's box as 100 by 100, before its font is measured. A later read gives its size.
