# Look

The foundations of the app's design system, the rules every screen follows: how each client is designed for its device, colour, type, space and shape, layout and the pointer. The components built on them, such as buttons, the status mark and the app icon, are in `components.md`, and a rule about one place is in that place's file, even when it's about how it looks. Each area's file shows its own screens. `crates/look` holds them in code: every colour, kind of text and corner by what it's for, every step of space, and every size a control, a bar, a row and an icon come in, the theme made from them, the mark, the icon, plan mode's texture, the pointing hand and the baseline text of different sizes shares.

It's meant to feel familiar to the people Roundhouse is for, and it sets itself apart with a motif of its own, the status mark.

## Each client

Every client is designed on purpose for the device it's on, rather than another client fitted to it. A phone isn't the desktop made smaller, and a tablet isn't a phone made bigger. What makes them one product is the same records and behaviour, the same chat, and the same look.

- **Not one scale factor,** because a device is more than a size. A phone is held closer and touched rather than pointed at, and has room for one thing at a time, so the desktop scaled up for it would be a magnified desktop, with hover where nothing hovers and three panes where one fits.
- **Not a design system for each client,** because the brand would be kept twice and drift apart, and a component that means the same thing everywhere, such as a button, would be built twice.
- **The brand is shared exactly:** the colours, the status mark, the icons, the app icon's drawing and motion.
- **The foundations are names, with a value for each reach.** A screen asks for a kind of text, a step of space, or a size of control, bar or row, and gets the value for the client it's in. Each table below gives a desk's and a handheld's. A handheld's are chosen for a device held in the hand, starting from the platform's own guidance, such as iOS's body text at 17 points and its 44-point bars and rows, rather than worked out from a desk's, and are settled by eye on a device. Space is the one foundation that's the same at both, since its 4px steps already suit a phone. The app chooses its reach once, as it starts, as `look::Reach`, since a device's reach doesn't change. *Unconfirmed:* a handheld's values.
- **A component means the same on every client, and responds as its input asks.** On a touch screen a button has no tooltip, since nothing rests on it, takes a tap over at least 44 points, as iOS asks, whatever size it's drawn at, and shows that it's pressed rather than that something is over it. The app says which input it has as `look::Input`, which can change while it runs. *Not built:* the tap over more than what's drawn, and the press in place of what's over it, so a handheld's large control is drawn at 44 points, for what's tapped most.
- **Some components belong to one kind of client,** such as a handheld's bar across the top, a row that opens what it names and a sheet, or the desktop's sidebar, tabs and menus. *Not built:* a handheld's.
- **The transcript is the one view every client shares,** since a chat is the same everywhere, and it takes its sizes from the foundations and how it's touched from the input, such as selecting with a long press on a touch screen. *Not built:* it's sized for a desk, and selects only with a pointer.
- **Screens are never shared.** Each client lays out its own, for what it's used for there.
- **A layout is designed for a width, not a device.** A compact one serves a phone and a foldable that's closed, a regular one a tablet and a foldable that's open, and a wide one the desktop's sidebar, chat and changes. Each is designed on purpose for that room, and a client lays itself out again as its width changes. *Not built:* the regular layout, so the iOS app lays out only a phone's. *Unconfirmed.*
- **A change of width shows the same things open, scrolled where they were,** such as when a foldable opens: the same workspace, chat and panel, in the new layout, with nothing opened again. *Unconfirmed.*
- **Each client's screens are in the gallery at each width it lays out, and with each input it takes,** and the tables here and in `components.md` give a value for each reach. *Not built:* a desk's alone, wide, with a pointer.

What a client is, at any moment, is three traits rather than one kind of device, since one device can be several shapes: a foldable is a phone and nearly a tablet in one session, a tablet can have a trackpad attached, and a desktop window can be made narrow. Each decides one thing, and each value comes in with the first client that uses it. *Unconfirmed.*

| Trait | Its values | What it decides | Changes while the app runs |
|---|---|---|---|
| Reach | Handheld, Desk | The foundations' values | No: it's the device's |
| Input | Touch, Pointer | How components respond: how far a press reaches, what shows a press or what's over something, tooltips and gestures | Yes, as a trackpad is attached to a tablet |
| Width | Compact, Regular, Wide | The layout | Yes, as a foldable opens, a tablet shares its screen or a window is resized |

## Colour

Each colour means one thing.

- **The greys are one ramp,** so the steps between them look even, and they read as one set made on purpose rather than colours picked one at a time. There are eleven stops at even steps of lightness, as Oklab measures it, since its steps look even to the eye, from 0.175 to 0.304, all on the brand's hue at one faint chroma, 0.0105, so each is as warm as the next. Every surface, edge and grey under the pointer is a stop: the pane is the darkest and the sidebars the next, the composer and code blocks are four stops up, what's raised six, the thin edge eight, and the edge of something to use the lightest. `look::Colors` makes them from these few numbers, so tuning the palette is changing one of them.
- **Three warm near-black surfaces,** so the app reads as one calm field with a few raised things on it. The pane is `#140f0c`, the sidebar `#17120f`, and anything raised, such as a card or a popover, is `#26211e`. The composer and code blocks are `#201b18`. A prompt is a wash of the brand's copper, 18% of it over the pane, `#36251a`, so what was asked is easy to find when scrolling.
- **Two edge colours.** The thin edge that separates, around a card, along a header or between rows, is `#2d2724`. Something to use, such as the composer, a chip, a key, a field or a button that opens something, has a lighter edge, `#332d2a`, so it reads as something to use rather than as a line between two things. So does the button back down to a chat's end, which sits over the chat's text, as `chats.md` says.
- **One shadow, under what opens over the rest,** such as a card, the outline, the find bar or what a hover shows, so it reads as on top of what it covers. It's soft, so it lifts what's over the rest without drawing the eye: black at 22.5%, 8px below and blurred by 12px, and black at 15%, 2px below and blurred by 3px, as GPUI measures a blur, which is half what CSS calls it. What's raised in place, such as a pill or the update card, has none. A menu, a tooltip and a dialog cast it too. *Not built:* the kit draws a dialog with a fainter shadow of its own.
- **A dialog dims everything behind it to 40% of its brightness,** under 60% black, so it's plainly the one thing to deal with, and what's behind it reads as out of reach. *Unconfirmed.*
- **Three text colours, each as readable on the pane as its job needs:** foreground `#eee6e1`, muted `#b8b1ac`, and faint `#7c7671` for meta, line numbers and hints. Each is made to a contrast on the pane as APCA measures it, which judges dark colours as the eye does where WCAG 2's ratio doesn't: the foreground at 92, where body text reads best, muted at 60, the least for text that's read, and faint at 30, the least for any text. They're on the greys' hue and chroma, past the lightest stop. On a lighter surface they have a little less, down to 89, 57 and 27 on something raised under the pointer.
- **Copper, `#d08a5c`, is the brand.** It's for the mark over an empty pane, the app icon, keyboard focus, the edge of a comment being written, the wash behind a prompt and, quieter, a composer that's planning, and never a status.
- **Pale copper, `#d8c2b3`, marks the chat showing,** as the line under its tab. It's the brand's hue, paled toward the text, so it says where you are without reading as a status.
- **A composer that's planning is in a quieter copper.** Its surface takes a tenth of copper, its edge is copper at a little over half strength over that, and its texture's dots are copper. It's the brand's own colour, so plan mode reads as a way of working you chose rather than a status, and its edge stays quieter than keyboard focus, which the composer also shows while files are dragged over it. `chats.md` has the texture. *Unconfirmed.*
- **Amber is for the card that needs you, and red means the agent stopped short.** A plan waiting to be approved is washed in amber, with its heading in amber, and a failed request's card the same in red, as `components.md` says. A question isn't a card, and neither is a request to use a tool, since each takes the composer's place, and has the thin edge colour on the pane's own surface, as `chats.md` says. Amber isn't on the mark, where the merge queue's yellow would be taken for it.
- **The diff uses the colours review tools do,** because they're about a file rather than about you or the agent: lines added and removed are green and red, and a modified file's icon is yellow.
- **A file's icon is in its kind's colours,** as Catppuccin's Mocha flavour draws them, in thin lines, which editors have made familiar, and a folder's is the plain folder in the muted text colour, whatever it holds, as is the plain file's, for a file with no icon of its own. They say what a file is, never how anything stands, so they sit beside a name and nowhere a status would.
- **A pull request's state is shown in GitHub's colours,** which the people Roundhouse is for already know: green when it's open, purple when it's merged, a muted red when it's closed, and a grey of its own when it's a draft.
- **The pull request's bar says more with the same colours.** It's green only once nothing is in the way of merging, and on the theme's raised surface, in the text colour, while there's something to do first. A place in the merge queue is GitHub's yellow, which GitHub uses for what's waiting on it, and the bar always says in words what it's waiting for, so it isn't taken for amber's turn of yours. The fan is yellow there too, and green for every other open pull request, since it's too small to say what's in the way of merging. Amber is never on the mark, so the queue's yellow isn't taken for it. `look::Work` chooses them from `look::Colors`, so a workspace's fan and the bar can't disagree. *Unconfirmed.*
- **Every colour the app draws with has a name for what it's for,** so a colour means the same wherever it's drawn, and two that look alike and mean different things, such as an approval's amber and a warning's, can change apart. What's drawn takes one of them by its name, or one of them at a strength chosen where it's drawn, such as a removed line's wash, which is `removed` at 12%, rather than a colour by its value. The app is dark only, and a light theme would be a second set of values for the same names. `look::Colors` holds them, and the kit's theme is made from them, so the kit's own parts, such as its buttons and menus, draw with the same ones. The few that only the kit's own parts draw with, such as the text on its danger button, are given where the theme is made. Code's colours, a file's icon and an image's pixels are their own, as above. *Unconfirmed.*

**Every colour, by its name.** Each is the field of that name in `look::Colors`. *Unconfirmed.*

| Name | Colour | For |
|---|---|---|
| **Surfaces** | | |
| `pane` | `#140f0c` | The workspace's pane, Home's and Settings' pages, and the window's ground |
| `sidebar` | `#17120f` | The sidebars |
| `inset` | `#201b18` | What's set into a surface: the composer, a prompt waiting in the queue, a call's output, a code block |
| `raised` | `#26211e` | Anything raised, whether it opens over the rest, such as a card, a menu, a tooltip or what a hover shows, or sits in place, such as a pill, a file's header, the update card or an open pull request's bar with something to do |
| `prompt` | `#36251a` | A prompt's bubble |
| `icon_tile_top` | `#211d1b` | The top of the app icon's tile, the icon's own colour rather than a stop on the ramp, since it's there to make the icon look right |
| **Under the pointer, pressed or chosen** | | |
| `row_hover` | `#201b18` | The kit's list's row under the pointer. A list's rows, such as Home's, a file's or a question's option, were once this, and are now washed as a borderless button is, as `components.md` says |
| `line_hover` | `#1a1512` | A call's line under the pointer, and the compaction's |
| `menu_hover` | `#2a2421` | Behind code within a line of text, which the kit draws in its accent's grey. A menu's row and a page in Settings' sidebar were once this, and are now washed as a borderless button is, as `components.md` says |
| `raised_hover` | `#2d2724` | Something raised, under the pointer |
| `raised_pressed` | `#332d2a` | Something raised, pressed, such as the hub switcher |
| **Edges and the shadow** | | |
| `edge` | `#2d2724` | The thin edge that separates: around a card, along a header, between rows |
| `control_edge` | `#332d2a` | Something to use: the composer, a chip, a key, a field, a button that opens something, the button back down to a chat's end |
| `shadow` | `#000000` | Under what opens over the rest, faint |
| **Text** | | |
| `text` | `#eee6e1` | What to read |
| `text_muted` | `#b8b1ac` | What's secondary, and a name that needs nothing from you |
| `text_faint` | `#7c7671` | Line numbers, meta, hints, such as a field's placeholder, and a mark with no pull request |
| **Buttons** | | |
| `button` | `#201b18` | A button with an edge, such as a dialog's Cancel |
| `main` | `#f2eee9` | An area's one main action, such as Send |
| `main_hover` | `#ffffff` | The main action under the pointer |
| `main_pressed` | `#ddd8d2` | The main action pressed |
| `on_main` | `#1a1512` | What's on the main action, and on any other light fill |
| **The brand** | | |
| `brand` | `#d08a5c` | Copper: the mark over an empty pane, the icon, keyboard focus, the caret, a comment being written, a composer that's planning |
| `current` | `#d8c2b3` | Pale copper: the line under the tab showing, and, faint, under a tab the pointer's over, and what's found |
| `link` | `#dba27c` | A link |
| `link_hover` | `#e8b896` | A link under the pointer |
| **What happened** | | |
| `needs_you` | `#e9b24c` | The card that needs you, such as an approval |
| `warning` | `#e9b24c` | A warning, such as setup that failed |
| `done` | `#6cc488` | Something ready, or done, such as dictation's model once it's prepared |
| `stopped` | `#ee6f64` | The agent stopped short: its mark's turntable, and a turn that failed |
| `error` | `#ee6f64` | Something else that went wrong, such as a call that failed, and the mark over a pane that can't show what it's for |
| `added` | `#6cc488` | Lines and counts added to a file |
| `removed` | `#ee6f64` | Lines and counts removed from a file |
| `modified` | `#e9b24c` | A file that's changed, and what changed in an image |
| **A pull request** | | |
| `pr_open` | `#3fb950` | Open |
| `pr_merged` | `#a371f7` | Merged |
| `pr_closed` | `#b9605a` | Closed |
| `pr_draft` | `#8b949e` | A draft |
| `pr_queued` | `#d29922` | Waiting on GitHub, such as in the merge queue |
| **The window** | | |
| `dim` | `#00000099` | Over everything behind a dialog |
| `scroll_thumb` | `#ffffff1f` | A scrollbar's thumb |
| `scroll_thumb_hover` | `#ffffff33` | A scrollbar's thumb under the pointer |
| `windows_close` | `#c42b1c` | Windows' close button under the pointer |
| `on_windows_close` | `#ffffff` | Its glyph |

Three strengths have names of their own, since several places draw with them: what's selected in text is `brand` at 30%, the most the kit lets a selection show, every match of what's found is `current` at 22%, and the match being shown is `brand` at 55%. *Unconfirmed.*

**Every colour by its name, as the app draws it.** *Unconfirmed.*

![Every colour by its name](screens/colours.png)

## Type

- **The system font, at 14px for body text.** Secondary text is muted, at the same size or at 12px for details. Sizes in rems are measured from 16px, as the web's and the kit's are, so the kit's spacing comes out as the web's does. It's dense enough to watch many agents, and still easy to read. A tool call's line is at body size, since what it did is the main thing it says. *Unconfirmed.*
- **The app's emphasis is medium weight, and what someone wrote marks as strong, such as the agent's bold and its Markdown headings, is semibold, never bold,** as the owner chose from pictures of three ways. Medium is for what the app has stand out from what's around it, such as the title that names what a card or section is about, since bold shouts in a dense layout. Semibold lets a line the agent meant to stand out do so among the app's quieter titles, as GitHub draws Markdown's. Medium was drawn as regular until the app asked for the weight a Mac's text system finds the system font's Medium face at, and a test holds each weight to a face of its own. `look::Typeset::emphasis` and `look::STRONG` set them. *Unconfirmed.*
- **In a list of one kind of thing, such as the workspaces in the sidebar or the chat tabs, a name's colour and weight change only for state that needs you,** such as a reply you haven't read. Which one is selected or open shows in its background or underline instead, and its name looks as it does when it isn't. Names are read side by side, so any change to one reads as news, and macOS's font smoothing thickens brighter text, so a name that brightened would look heavier too.
- **Code is Menlo at 12px,** set once, as `look::Type` has it, and given to the kit's theme from there.
- **Code's lines are 20px apart where they're read line by line, and 17px apart among the chat's prose,** as the owner chose from pictures of three ways: a file, a diff and a call's numbered lines are read one line at a time, often beside their numbers, and a code block in a reply, or what a call was given and put out, is glanced at, where denser shows more. The same output is spaced the same however long it is. The terminal lays out a grid of its own. `look::CodeLines` holds them. *Unconfirmed.*
- **Every kind of text the app sets has a name for what it's for,** so a size means the same wherever it's set, and two kinds that are the same size today, such as details and code, can change apart. What's set takes one of them by its name, rather than a font or a size by its value, and text that stands out from what's around it takes emphasis rather than a weight. `look::Type` holds them, and the kit's theme is given the rem, and code's font and size, from there, so the kit's own parts agree.
- **A kind of text takes its lines from where it's set, as it takes its weight,** such as a field, a menu's row, a diff or a question, since each of those sets how far apart its lines are. A key's kinds have lines of their own, as tall as what the key says, since it's set on its cap, and what's held with it sits on the key's line. Code's lines are below. *Unconfirmed.*
- **Text in different sizes or fonts along one line sits on one baseline,** as the words of a sentence do, such as the workspace header's title beside its branch, in the code font, and where that started, both smaller. Centred one by one, smaller text would ride higher than what's beside it. An icon stays centred, and so does text on a shape, such as a pill, a chip or a button, since it's the shape that's lined up. `look::LineText` sets them. *Unconfirmed.*

**Every kind of text, by its name.** Each is a kind in `look::Type`, at a desk at the app's scale, where a rem is 16px, and on a handheld in pixels, as iOS's own kinds of text are. One with no weight of its own takes what's around it, which is regular unless it's given emphasis.

| Name | Font | Size | Weight | On a handheld | For |
|---|---|---|---|---|---|
| `PageTitle` | System | 24px | Medium | 28px | A page's name, such as each of Settings' pages: the largest kind, since it says where you are |
| `Heading` | System | 16px | Medium | 17px | What an empty pane says is missing or went wrong, above why |
| `Body` | System | 14px | As around it | 17px | What to read: what the agent wrote, a prompt, a call's line, a row's name, and most of the rest |
| `Detail` | System | 12px | As around it | 15px | Details beside or under what to read, such as a time, a count, a hint or a heading over a list |
| `Code` | Menlo | 12px | As around it | 14px | Code: a command, a path, a file, or what a call put out |
| `Key` | System | 10px | As around it | 12px | What a key says, such as the `F` of Settings' `⌘ F` |
| `KeyHeld` | System | 7.5px | As around it | 9px | What's held with a key, such as Settings' `⌘`, smaller than the key that's pressed, as on the keys themselves |

**Every kind of text, as the app sets it.**

![Every kind of text](screens/type.png)

**How far apart code's lines are, by where it's read.** Each is a kind in `look::CodeLines`. *Unconfirmed.*

| Name | Height | For |
|---|---|---|
| `File` | 20px | Read line by line, often beside their numbers: a file, a diff, and a call's numbered lines |
| `Chat` | 17px | Glanced at among the chat's prose: a code block in a reply, and what a call was given and put out, however long it is |

**Every spacing of code's lines, as the same code.** *Unconfirmed.*

![The same code at each spacing](screens/code-lines.png)

## Space and shape

### Space

- **Every space is one of six steps on a 4px grid: 4, 8, 12, 16, 24 or 32px.** A space is the room left between and around things, as a gap, a padding or a margin, to set them apart or hold them together. What belongs together is closer than what's apart, as a project's row and its workspaces are in the sidebar, closer to each other than to the next project. `look::Space` holds the steps, and what's drawn takes one by its name, rather than a length by its value. *Unconfirmed.* A length set for something else isn't a space, even where it's a gap, a padding or a margin, so it isn't a step, but is set by what it's for: one that lines a thing up, such as an icon level with the middle of the line of text beside it, by what it lines up with; one that keeps clear of what the app doesn't lay out, such as the scrollbar's thumb or macOS's window buttons, by what it keeps clear of; a drawing's own, such as the gap between the two parts of the context card's bar, by the drawing; the 2px around and between the buttons of the diff's switch of views, by a regular button's height, which the switch is as tall as; the room above and below a pill's and a chip's words, by the height they share; and the 6px at a chip's start and between its parts, by the chip, as `components.md` says. *Unconfirmed.* Settings' search is a regular field, as `components.md` says, on the grid as every field is, with a key 16px tall and 4px corners. The button back down to a chat's end is a 28px square, as `chats.md` says.

**Every step of space, by its name.** Each is a step in `look::Space`, from the smallest, in pixels, so a space stays as it is whatever size text is set at, and is the same on a handheld as at a desk. *Unconfirmed.*

| Name | Room | For |
|---|---|---|
| `XSmall` | 4px | Between a name and what's said under it, such as an account's; between the lines of one list, such as a project's row and its workspaces or Settings' pages; between the buttons on a row, such as a prompt's waiting in the queue, and Home's choices and Create in its bar; where a row of tabs starts; and around a list's rows, such as All files', Changes', Home's, the outline's and a web search's results' |
| `Small` | 8px | Between an icon or a mark and its words, such as a sidebar row's, a page's in Settings or a file's in a tree; at a sidebar row's ends and around a sidebar's list, the app's and Settings' pages'; above and below the chat's rows; between a comment's Cancel and Save; above and below a prompt waiting in the queue or a question's option; and above a terminal's grid |
| `Medium` | 12px | Around what's in a card, such as the update card, the workspace's card or a turn's card, and between a card's parts, as `components.md` says, or a question's; at the end of every bar, as `components.md` says, and where a tab's name starts in it; at the sides of a call's block, and at the left of a terminal's grid; at the ends of a large row, such as Home's, and between what's in it; and between an account's picture, its name and what's said of it |
| `Large` | 16px | At the sides of the chat's column and under the composer; where what's in every bar starts, such as the workspace's header, the pull request's bar, the new workspace's card's and the bar under an image's comparison, as `components.md` says; around Agents' panel; between the settings on a page; and between an account's row and its menu |
| `XLarge` | 24px | Around what's in Agents' panel, and around a picture in the file viewer or an image's comparison; under a setting; between the two sides in the bar under an image's comparison's; and the least room between the two ends of a line laid out to its ends, such as a setting's name and its switch |
| `XXLarge` | 32px | At the sides of a Settings page, and between its lists, such as Agents' |

**Every step of space, as the app leaves it.** *Unconfirmed.*

![Every step of space](screens/spaces.png)

### Corners

- **Corners are 6px on controls, and 10px on cards and the composer.**
- **Every corner the app rounds has a name for what it's on,** so a corner means the same wherever it's drawn, and two that are the same size today can change apart. What's drawn takes one by its name, rather than a radius by its value. What's set across a chat's column, such as a card in the chat, a call's block or the queue, has a card's corners, as the composer it sits above does, and what's set small in it, such as a prompt's bubble, a pill or a chip, has a control's. A row to choose in a list inside a card, such as the outline's, has a control's, so its corner sits inside the card's. What's round all the way, such as a bar's track or a dot, is round all the way. Every edge is 1px, and a quote's rule, such as the one at the left of what a subagent was asked or of a block a proposal's revision changed, and the line under the tab that's showing are 2px. `look::Corner` holds the corners, and `look::EDGE`, `look::QUOTE_RULE` and `look::TAB_LINE` the widths. The kit's theme is given a control's and a card's corners from there, so the kit's own parts agree, and a tooltip has a control's. A menu has a card's, with its rows a control's inside it, as `components.md` says. *Unconfirmed.*

**Every corner, by its name.** Each is a corner in `look::Corner`, in pixels, so a corner stays as round whatever size text is set at. *Unconfirmed.*

| Name | Corners | On |
|---|---|---|
| `Control` | 6px | Something to use, such as a button, a field, a switch, a row to choose in a list, as Settings' pages and the outline's are, or a menu's, a menu's row and a tooltip, and what's set small beside them, in a chat's column or inside a card, such as a pill, a chip, a prompt's bubble, a prompt waiting in the queue, an attachment, a question's option or what a question's tool was given |
| `Card` | 10px | A card, and what the app draws over the rest, such as the outline, the find bar or what a hover shows, a menu, a dialog, a composer, and the question panel in its place, and what's set across a chat's column, such as a card in the chat, a call's block, the compaction's summary or the queue |
| `InlineControl` | 4px | A control about as tall as a line of text, such as Settings' key or a question's checkbox |
| `Glyph` | 2px | What's drawn a few pixels across, or as a fine line around something, such as the square in Stop, the image diff's swatch, or its outline around what changed between two images |

**Every corner, as the app rounds it.** *Unconfirmed.*

![Every corner](screens/corners.png)

### Controls

- **Every control has a size for where it sits,** whether it's a button or a field, so the controls in the same kind of place are the same size, wherever it is, and a field is as tall as the buttons beside it. Each leaves a little room at its ends, around what it says. A small one is set in a line of text or a row one line tall, a regular one is a bar's or a card's own, set apart from its text, and a large one is on its own. `look::ControlSize` holds the sizes, and `components.md` says how each control takes them. *Unconfirmed.*

**Every size of a control, by its name.** Each is a size in `look::ControlSize`, from the smallest. What a control says, such as a button's words or what's typed in a field, is the kind of text its row says. *Unconfirmed.*

| Name | Tall | Room at its ends | Text | Tall on a handheld | Room on a handheld | For |
|---|---|---|---|---|---|---|
| `Small` | 24px | 8px | Detail | 32px | 12px | Set in a line of text or a row one line tall: a row of a list, a tab, the heading over a list, a call's line in the chat, a line of a card's text, the composer's bar, the find bar, a banner and a fold, and the window's own buttons in its corner, beside the Mac's. A switch's segments are small, so the switch, with the room around them, is as tall as a regular button |
| `Regular` | 28px | 12px | Body | 36px | 14px | A bar's own, along a pane's edge, such as a header's, a tab bar's, the pull request's bar's or Home's bar's, or a card's or a page's own action, set apart from its text, such as the update card's, a comment's on a diff, a setting's or a list's of accounts, and the button over the chat back to its end |
| `Large` | 32px | 16px | Body | 44px | 16px | On its own: a dialog's buttons, a menu's rows, the one thing an empty pane says to do, and what answers a card in the chat, such as approving a plan or retrying a request |

**Every size of a control, as the app draws it.** *Unconfirmed.*

![Every size of a control](screens/controls.png)

### Bars and rows

- **Every bar has a size for where it sits.** A bar reaches from one edge of a pane or a card to the other, with an edge under it. A regular bar is along the top of a pane or a card, and every one is as tall, so the bars side by side across the window line up, whichever pane each is in: the workspace's header, the pull request's bar and Home's bar, in the strip along the window's top, which is as tall as they are, both rows of tabs under them, the bar over Changes' files that sends the comments waiting, a file's path, a subagent's tab's bar, and the bar along the top of the new workspace's card. A small bar is under a bar, and says more of what that one shows, in a detail's text, such as what each side of an image's comparison is. A bar holds buttons of its own size, with room above and below them. What else is called a bar, such as the find bar over the chat or the bar along the composer's bottom, isn't one of these. `look::BarSize` holds the sizes. *Unconfirmed.*

**Every size of a bar, by its name.** Each is a size in `look::BarSize`, from the smallest, with the edge under it. *Unconfirmed.*

| Name | Tall | On a handheld | For |
|---|---|---|---|
| `Small` | 32px | 36px | Under a bar, saying more of what that one shows, in a detail's text, such as what each side of an image's comparison is |
| `Regular` | 40px | 44px | Along the top of a pane or a card: the workspace's header, the pull request's bar, Home's bar, both rows of tabs, the bar over Changes' files that sends the comments waiting, a file's path, a subagent's tab's bar, and the new workspace's card's |

- **Every row of a list that's one line tall has a size for what the list is,** so the rows of the same kind of list are as tall, wherever it is. A regular row is for most lists, such as the places to go in a sidebar, and a large one is a row of a table. A small row is used sparingly, for a long list to look down, where more room would show less of it at once, such as a file tree or a chat's calls, and whatever the list, for the heading over it, and for a line on its own, set apart from a list, such as a fold among a diff's lines or the build line under the sidebar. Each holds a small button, with room above and below it. A row taller than a line, such as a prompt in the outline or in the queue, is as tall as what it says, with room above and below it. `look::RowSize` holds the sizes. *Unconfirmed.*

**Every size of a row, by its name.** Each is a size in `look::RowSize`, from the smallest. *Unconfirmed.*

| Name | Tall | On a handheld | For |
|---|---|---|---|
| `Small` | 28px | 36px | Used sparingly: a long list to look down, where more room would show less of it at once, such as a file in a tree, a call's line in the chat, or a page a search found, and whatever the list, the heading over it, and a line on its own, set apart from a list, such as a fold among a diff's lines or the build line under the sidebar |
| `Regular` | 36px | 44px | Most lists, such as the places to go in a sidebar: the sidebar's own places, its projects and their workspaces, and Settings' pages |
| `Large` | 44px | 56px | A row of a table: Home's workspaces, and the accounts in Settings' Agents and Git, which have a line between each and the next |

**Every size of a bar and of a row, as the app draws them.** *Unconfirmed.*

![Every size of a bar and of a row](screens/heights.png)

### Icons

- **Every icon has a size for where it's drawn,** so the icons in the same kind of place are the same size, wherever it is. One set among text, as one of its words, is about as big as the text: small among a detail's or code's, and regular among body text. One that stands for what a row, a tab, a bar, a card or a call's line is, at its start, is large, as big as a mark beside text, since that's the place a workspace or a chat has its mark, so a file tab's icon is as big as a chat tab's mark, and a call's as the working line's. A button's icon is sized by the button, as its table in `components.md` says, and one inside a control about as tall as a line of text, such as the check in a checkbox, is small, so it has room inside it. Each is in pixels, so an icon stays as it is whatever size text is set at. `look::IconSize` holds the sizes. *Unconfirmed.* What's drawn rather than an icon, such as the mark, a task's ring or a cache's mark, keeps a size of its own. *Unconfirmed.* *Not built:* the kit's own parts size their icons themselves, but for a menu's, which `components.md` sizes.

**Every size of an icon, by its name.** Each is a size in `look::IconSize`, from the smallest. *Unconfirmed.*

| Name | Size | On a handheld | For |
|---|---|---|---|
| `Small` | 12px | 14px | Among a detail's or code's text: a rule's across the chat, such as a compaction's, and the compaction's in the outline; the speaker that says how a reply was heard, on its turn's line; the build line's answer; a changed file's state at its row's end, and a folder's chevron, in Changes; the check in a checkbox; and the `+` that adds a comment to a diff's line, on a button as tall as the line. Beside a small button's words |
| `Regular` | 14px | 18px | Among body text: a warning before the body text it warns of, such as an account's standing; the microphone at the start of a prompt said out loud; a project's chevron after its name; the arrow before an answer to a question, and the check on an option chosen; a file sent with a prompt waiting in the queue; a draft's pencil at a workspace's row's end; the glass in a search field; the arrow after a pull request's number; and the update card's details. Beside a regular button's words, and a small or regular button's own, as a square, such as Send, a tab's close or closed chats |
| `Large` | 16px | 22px | Standing for what a row, a tab, a bar, a card or a call's line is, at its start: a file's or a folder's, in All files or Changes; a place's or a project's in the sidebar; a page's in Settings; an account's; a file tab's, and a subagent's, on its tab and its bar; a terminal's, on its tab; a file's card, sent with a prompt; a failed request's; and a call's kind, or the chevron that takes its place, and the robot of a subagent at work. Beside a large button's words, and a large button's own, and on a menu's row, whether it's the row's icon or its check |

**Every size of an icon, as the app draws it.** *Unconfirmed.*

![Every size of an icon](screens/icons.png)

## Layout

- **A sidebar lists things to choose from, and what they open to is read in the workspace's pane,** as a file from Changes or All files opens in the file tab. A sidebar is too narrow for anything to read at length, such as a plan, a table or options compared side by side, and widening it far enough takes the room from the chat it would sit beside. So such things open in a tab in the pane, and never in a sidebar, even to keep them in sight of the chat.

## The pointer

- **The pointer is a hand over anything that can be pressed:** a button, a tab, a switch, a link, and a row that opens something, such as a workspace's or a file's. What can be pressed is then plain before it's pressed, even where a borderless button looks like a label until it's hovered. It's a hand whether or not a press would change anything just then, as over the tab that's already showing or Send with nothing typed, so the hand says what a thing is and doesn't flicker as its state changes. The kit's own controls, such as the button that removes an attached file and a dialog's close button, keep its arrow. *Unconfirmed.*
- **It stays an arrow over what's greyed out,** such as a button that can't be pressed yet, and over one that's waiting, so the pointer agrees with how the button looks. A switch that's greyed out has the kit's barred circle instead. *Unconfirmed.*
- **It stays an arrow over what only shows something while the pointer rests on it,** such as the context's ring, since a hand there would ask for a click that does nothing. *Unconfirmed.*
- **It's an open hand over what can be dragged to another place, such as a project's row in the sidebar, and a closed one while it's dragged,** even where a press does something too, since a hand that points would say nothing of the drag. *Unconfirmed.*
- **A menu's rows keep the arrow, as the system's menus do,** since an open menu is nothing but things to click, and the row under the pointer is lit. *Unconfirmed.*
- **It's an I-beam over text that can be typed in or selected,** such as the composer and what the agent wrote, as it is everywhere else on a Mac, so it's plain that dragging there selects. *Unconfirmed.*
