# The component library

How code makes and uses the components screens are built from. `docs/spec/components.md` says how each component looks and what each of its states means, `docs/spec/look.md` the foundations they're built on, and `docs/gpui.md` everything else about how code that draws is written.

## What it is

- **The components are in `look`, beside the foundations they're built on,** since both the app and `transcript` draw with them, and `transcript` can't depend on the app. Each is drawn with the kit's own part where the kit has one, from the kit's copy in `third_party/gpui-kit`. Where the kit doesn't do what a component needs, the kit's copy is changed if upstream would take the change, and the component draws that itself otherwise, as `docs/gpui.md` says.
- **A component is a type of `look`'s own, which gives its caller none of the kit's or GPUI's styling.** It holds the kit's part, and offers only what the component is: what it's for, its size, what it shows, what it does and what state it's in. So every component of a kind and a size looks the same wherever it's made, the kit stays behind one module, where it can be changed or replaced without touching what uses it, and a picture of every look a component has can be drawn from the same calls the app makes. *Unconfirmed:* that a component's own options say how it's laid out among what's around it, as below.
- **`look` knows nothing of the app's areas.** An area chooses which component it draws, and its kind, its size and its state, from what it holds itself, and the component draws that. A component that needs to say more takes a value of `look`'s own, as a button takes a `look::Work` for where a pull request stands, rather than one of an area's.
- **A component knows the client it's drawn in, so no screen does.** It takes its sizes from the foundations, which have a value for each reach, and how it responds from the input, such as a touch area of 44 points and no tooltip on a touch screen, as "Each client" in `docs/spec/look.md` says. A component only one kind of client has, such as a handheld's bar across the top, is in `look` with the rest, under the same rules. *Not built.*
- **What the library doesn't name, the app doesn't draw.** A look an area needs that no component has is added to the component, with its row in the spec's table and its place on the component's sheet, rather than made where it's drawn. Until then, the area draws with the kit's part, and the policy check lists it and says why.

## How a component's API is laid out

Each component follows the same pattern, so one that's learned reads like the others. *Unconfirmed.*

- **It's made by a function that takes what it can't be without:** an ID, which tests find it by, for one that's pressed or typed in, its kind, its size, for one that comes in sizes, and what it shows, such as a button's words, a square's icon or the key a key names. So none is made without them, and the function says its shape: `look::button` makes one with words, and `look::icon_button` one that's only an icon.
- **Its kinds are one enum of what each is for, declared with `named!`,** such as `look::ButtonKind`. A kind says everything about how the component looks but its size and its state, so each look it has is one name in one list, which the spec's table, the component's sheet and the design system's page all go through. A look that differs, such as a button's muted words, is a kind of its own rather than an option on top of the kinds, since an option would make every kind come with it and without it, including some that mean nothing, such as a main action with muted words.
- **Its sizes are a type of its own while no other component has them, and a foundation once another component shares them,** as `docs/spec/README.md` says, as `look::ControlSize` is, a control's sizes, which a button and a field share.
- **It takes no size in pixels from what draws it,** such as a height that place needs, since that changes how it looks without changing what's in it, so its words and its mark crowd its edges, as a chip squeezed among a reply's words once did. A size it needs is a value of its sizes, which sets what's in it in proportion, as `look::ChipSize` does. A width that only lays it out among what's around it is an option of how it sits, as below.
- **Its states are methods named for what they mean,** such as `on`, `off` and `waiting`, rather than the kit's `selected`, `disabled` and `loading`, each taking whether it's in that state, so the state can come straight from what the area holds.
- **What every one of a kind does is built in,** rather than asked for at each place it's made, such as the hand over a button while a press does something, and the arrow while it's off or waiting.
- **What it shows beyond what it's made with goes in named slots,** such as a button's icon beside its words, and what comes after them.
- **How it sits among what's around it is an option named for what it does there,** such as standing out past its own room to line up with text, or growing to share a row. The room around it is its container's.
- **What it does is a handler,** such as what a click or a press does, and what it opens, such as a menu, which the component builds with the kit's.

## Checks

- **Each of a component's tables in `docs/spec/components.md` is held to its names in `look`** by a test, through `written`, as the foundations' tables are.
- **Each component has a sheet in the gallery of every kind in every state,** drawn with the same calls the app makes. A state the pointer brings, such as under the pointer or pressed, is drawn by pointing at each one or pressing it, for real, one at a time, and the sheet takes its pixels from that picture, with `headless::draw_pointing`. The design system's page shows each kind beside its name, cut from the sheet.
- **A test in `look` holds what's built in,** such as which buttons have the hand.
- **`tools/policy`'s `design_system.rs` refuses each of the kit's parts that a component holds outside `look`,** with a test for each, such as `only_the_component_library_uses_the_kits_button`, but in the places its list names, each with why: what a later component will draw, such as the hub switcher, and the few buttons with a look of their own.

## Adding a component, a kind or a size

A new size or option of a component goes through these steps as a kind does, and is on the component's sheet beside the rest before anything uses it, so how it looks beside them is seen first. A change that would alter what a rule the owner decided comes to, such as a component's height, is asked about before it's made.

1. Write its rules, its table and its sheet's caption in `docs/spec/components.md`, marked *Unconfirmed* until the owner confirms them.
2. Make its type in `look`, with its kinds in a `named!` enum, the way to make one that takes what it can't be without, and its options.
3. Hold the spec's table to the names with a test, and test what's built in.
4. Draw its sheet in `apps/desktop/tests/ui/gallery`, with each kind's place for the design system's page, and accept it as `AGENTS.md` says.
5. Move every place that draws it with the kit onto it, and have the policy check refuse the kit's part outside `look`.

## Buttons

`docs/spec/components.md` has a button's kinds, sizes and states, and `crates/look/src/controls` the code.

| To | Call |
|---|---|
| Make one with words, and maybe an icon beside them | `look::button(id, kind, size, words)` |
| Make one that's only an icon | `look::icon_button(id, kind, size, icon)` |
| Make a square with something drawn in it that isn't an icon, such as Stop's filled square | `look::glyph_button(id, kind, size, glyph)` |
| Put an icon beside its words, or something after them, such as a detail or a key | `icon`, `after` |
| Say what it does while the pointer rests on it, and what an assistive app calls it | `tooltip`, `tooltip_with_action`, `accessibility_label` |
| Do something on a click, or as a press goes down | `on_click`, `on_press` |
| Keep a press from also pressing what it's in, such as a row | `keeps_its_press` |
| Open a menu, with a caret after its words | `menu`, then the menu's `on_open_change` |
| Mark it on, off or waiting | `on`, `off`, `waiting` |
| Draw it in the colours of where a pull request stands | `of_work` |
| Set how wide it is, or share a row | `width`, `full_width`, `grows` |
| Line its words or icon up with what's beside it, past its own room | `stands_out` |
| Show it only while the pointer is over a group, such as a heading | `shown_while_over` |

A button's hand is its own: it has the hand while a press does something, and the arrow while it's off or waiting. A quiet or faint button's words take the kit's greyed-out colour while it's off.

## Keys

`docs/spec/components.md` has a key's kinds, and `crates/look/src/keys` the code.

| To | Call |
|---|---|
| Show a key to press, as a binding names it, such as `secondary-f` or `enter`, written as the system it's drawn on writes keys | `look::key(kind, binding)` |
| Show it after a button's words | the button's `after`, with a `Plain` key |

A key in one of the kit's menus is the kit's, from the action it's bound to, so it isn't made with `look::key`. A tooltip's key comes from its action too, and is drawn as a `Plain` key.

## Fields

`docs/spec/components.md` has a field's kinds and states, and `crates/look/src/fields` the code. A field holds the kit's `InputState`, which the place that keeps what's typed makes, with what the field says while it's empty, and the kit's input draws what's typed inside a frame of `look`'s own, since the kit sets its own room at an input's end, past what a style from outside reaches.

| To | Call |
|---|---|
| Make one, of what an `InputState` holds | `look::field(id, input, kind, size)` |
| Put an icon before what's typed, such as a search's, or something at its end, such as a key | `icon`, `after` |
| Say what an assistive app calls it, where nothing beside it says | `accessibility_label` |
| Mark it off | `off` |
| Share a row, rather than fill the width it's in | `grows` |


## A list's rows

`docs/spec/components.md` has how a list's row looks, and `crates/look/src/list_rows` the code. A row draws its wash under the pointer and while it's chosen, its corners and its room itself, by its size, so a list sets only how far in its rows are, with `look::ROWS_INSET` around them.

| To | Call |
|---|---|
| Make one, of a size, found by an ID | `look::list_row(id, size)` |
| Put what it holds in it, in a line from its start | `child`, `children` |
| Wash it as chosen, such as the file showing | `chosen` |
| Set it in after its start, such as a file in a folder | `indent` |
| Let it grow to what it holds, such as a prompt of two lines | `wraps` |
| Make it an option of a list to choose from, which assistive apps say is chosen, such as a page of Settings | `listed` |
| Make it a link, such as a page a web search found | `link` |
| Make it a button, such as a workspace in the sidebar | `button` |
| Turn it off for now, which assistive apps say, such as Create with no project | `off` |
| Do something when it's clicked, or say more while the pointer rests on it | `on_click`, `tooltip` |
| Follow the pointer coming and going, or a left press going down and let go, over it or elsewhere | `on_hover`, `on_press`, `on_let_go`, `on_let_go_outside` |
| Open a menu where it's right-clicked | `right_click_menu` |

## Menus

`docs/spec/components.md` has how a menu looks and a note's kinds, and `crates/look/src/menus` the code. A menu is filled each time it opens, with `look`'s rows, so none draws a row of its own, and the kit's menu draws them in the style `look::apply` sets once for the whole app.

| To | Call |
|---|---|
| Open one from a button | `look::Button::menu(anchor, build)`, where `build` fills a `look::Menu` |
| Open one where something is right-clicked | `right_click_menu(build)`, from `look::RightClickMenu` |
| Open one from something that isn't `look`'s, such as the kit's own button | `look::menu(build)`, which gives what the kit opens |
| Add a row, a line between groups of rows, or a label over some | `row`, `separator`, `label` |
| Make a row that says words | `look::menu_row(words)` |
| Put an icon before a row's words, its key or a note of a kind at its end, or say what it does while the pointer rests on it | `icon`, `key`, `note`, `tooltip` |
| Check a row as the one chosen, or grey it out | `chosen`, `off` |
| Do something as a row is chosen | `on_choose` |
| Find the keys rows show from where the keyboard's focus is, such as the field a menu is beside | `keys_from` |
| Set how wide a menu is, such as as wide as what opens it | `width` |
| Have a long menu to choose from scroll, and open on the row that's chosen | `scrolls` |
| Know when a menu closes, such as to hold something still while it's open | `open`, then the `look::OpenMenu`'s `on_close` |

## Dialogs

`docs/spec/components.md` has how a dialog is laid out and its widths, and `crates/look/src/dialogs` the code. A dialog is built each time it's drawn, by the closure that opens it, so it shows what's changed, such as an answer that's waiting. What it keeps, such as a field's state, is made before it opens.

| To | Call |
|---|---|
| Open one over the window | `look::open_dialog(window, cx, build)`, where `build` makes a `look::Dialog` |
| Make one that asks or says something | `look::dialog(title)` |
| Say something under its title, or put something in it, such as a field | `lead`, `child` |
| Add an answer that closes it, the one that confirms it, or one that does what its own handler says | `closes`, `confirms`, `answer`, each with a `look::Button` |
| Put something at the start of its foot, such as a link | `start` |
| Do something as it's confirmed, keeping it open if that says so, or once it's closed | `on_confirm`, `on_close` |
| Set how wide it is | `width`, with a `look::DialogWidth` |
| Close it once what it waited on has come, without what's done as someone closes it | `look::close_dialog` |
| Ask before what would stop or lose something, so Enter keeps things as they are | `careful` |
| Know whether one is open, or close every one, as when the hub they asked of has gone | `look::dialog_open`, `look::close_every_dialog` |
| Open one that's only what's in it, which draws all of itself, such as the new workspace's composer | `look::open_plain_dialog(window, cx, build)`, where `build` makes a `look::plain_dialog(shown, width)` |
| Put a plain one in the middle of the window, have a click on it close it, or do something on Enter or once it's closed | `centered`, `click_closes`, `on_confirm`, `on_close` |

The app asks and tells only in these, never in the system's alert, and `tools/policy` refuses the kit's dialog outside `look`, and the system's alert anywhere.

## Tooltips

`docs/spec/components.md` has how a tooltip looks and what it holds, and `crates/look/src/tooltips` the code. The kit draws a tooltip under the pointer, in the style `look::apply` sets once for the whole app, so each looks the same.

| To | Call |
|---|---|
| Say a few words while the pointer rests on something | `.tooltip(look::tooltip(words))`, on what's pointed at |
| Say what a button or a menu's row does, with the key that does the same | the button's `tooltip` or `tooltip_with_action`, and the row's `tooltip` |

What needs more than a few words, such as a path, goes on a hover card, not a tooltip. `tools/policy` refuses the kit's tooltip outside `look`.

## Cards

`docs/spec/components.md` has a card's kinds and widths, how its parts are spaced and what its heading holds, and `crates/look/src/cards` the code. A card's kind says its surface, edge, shadow and heading's colour, so none is drawn on `look::raised`, which is for what opens over the rest and isn't a card, such as the find bar.

| To | Call |
|---|---|
| Make one, of where it sits and how wide it is | `look::card(kind, width)` |
| Give it a heading, with the icon that stands for what it is, what's said after it and what's said faint at its end | `heading`, with `look::card_heading(words)` and its `mark`, `after` and `end` |
| Say something under its heading, as part of it, such as a hover card's lines | `under_heading` |
| Add a part, or a rule between two | `child`, `rule` |
| Lay it out as a dialog is, for a form set in place | `roomy` |
| Say what a test finds it by | `region`, `id` |

What's done on a card as a whole, such as keeping the keyboard's focus or measuring where it's drawn, is done on what it's set in.

## Small things in a line

`docs/spec/components.md` has the five kinds of small thing that sit in a line, what each is for and its kinds, and `crates/look/src/pills`, `chips`, `badges` and `counts` the code. Only a chip will open what it names, which isn't built, so none takes a handler yet.

| To | Call |
|---|---|
| Show a thing the app knows, such as a file or a subagent, with its kind's mark | `look::chip(kind, names)`, with a `look::ChipKind`, where `names` is a file's or a folder's path, of which it shows the last part, or the words it shows |
| Say which of a file's lines it's about, what a change did to it, or that it's gone | `lines`, `changed`, `gone` |
| Show a value as it was written, such as a command, or what went wrong | `look::pill(kind, value)`, with a `look::PillKind`, where `value` is its words, or its words with what a find marked |
| Say what's true of the thing beside it, in a word or two | `look::badge(kind, words)`, with a `look::BadgeKind`, and `id` for a test to find it and an assistive app to read it |
| Show a tag someone chose, in their colour | `look::label(words, color)` |
| Say how many | `look::count(kind, how_many)`, with a `look::CountKind`, and `said` for what it counts, as an assistive app reads it |
| Show the lines a file gained and lost | `look::lines_changed(added, removed)` |
| Set a count or lines on the baseline of the line they're in | `placed`, with the `look::LineText` part for their text |

## Bars

`docs/spec/components.md` has a bar's kinds, and `crates/look/src/bars` the code. A bar sets how far in what it holds starts and ends, and how far apart it is, by its kind, and draws its edge, so a place puts in only what the bar says and does.

| To | Call |
|---|---|
| Make one, of a kind and a size | `look::bar(kind, size)` |
| Put what it says in it, from its start, and what's done about it at its end | `child`, `children`, `end` |
| Keep what it holds clear of what's over its ends, such as the window's buttons | `clear` |
| Tint it in a pull request's colour, as the pull request's bar is | `of_work` |
| Have a button show only while the pointer's on it | `group`, with the button's `shown_while_over` |
| Find it in a test | `region`, `id` |

## Lines

`docs/spec/components.md` has a line's rules, and `crates/look/src/lines` the code. A line is drawn in the edge's colour and width, so no place sets either. A line is between two things. The edge around something, such as a card or a field, is part of that thing. *Unconfirmed.*

| To | Call |
|---|---|
| Put a line between two things in a column, such as the parts of a call's block | `look::line()` |
| Run one down what it's in, such as a guide beside a folder's files | `look::line().down()` |
| Say what happened in its middle, as a line across the chat does | `look::line().middle(element)` |
| Set a line on a side of an element, such as under a bar, between panes or under a row of Settings | `line_under`, `line_over`, `line_left`, `line_right`, from `look::Lined` |
| Set one over an element inside a card, in the colour a line on that card is | `line_over_on(kind)` |

## Banners

`docs/spec/components.md` has a banner's kinds, and `crates/look/src/banners` the code. A banner sets its colour, its icon, its room and its buttons' place by its kind, so a place gives it only what it says and what's done about it.

| To | Call |
|---|---|
| Make one, of a kind | `look::banner(kind)` |
| Put what it says in it, after its icon | `child`, `children` |
| Add a small borderless button that does something about it, at its end; the last stands out past its own room | `answer`, with a `look::Button` |
| Add one that opens a menu of what to do, in the same place | `answer_menu`, with a `look::MenuButton` |
| Keep what it holds clear of what's over its ends, such as the window's buttons | `clear` |
| Find it in a test | `region`, `id` |

## Empty states

`docs/spec/components.md` has an empty pane's kinds, and `crates/look/src/empty_states` the code. An empty state fills what it's in and puts what it says in its middle, so a place gives it only its words and what to do.

| To | Call |
|---|---|
| Fill a pane that has nothing yet, or can't show what it's for, under a heading | `look::empty(kind, heading)` |
| Say why under its heading, a paragraph at a time | `says` |
| Add a button that does something about it, the main action last | `answer`, with a `look::Button` |
| Say in one muted line that a list or a pane has nothing to show, where nothing needs doing | `look::nothing_to_show(words)` |
| Find either in a test | `id` |

## Tabs

`docs/spec/components.md` has a tab's widths, and `crates/look/src/tabs` the code. A tab draws its name, its line and its close button itself, so none is drawn on the kit's tab. The bar it's in is still the place's, until the bars are components.

| To | Call |
|---|---|
| Make one, of a width, found by an ID | `look::tab(id, name, width)` |
| Put what stands for what it names before its name, such as a chat's mark | `mark` |
| Count what it shows after its name, or say something else there, such as a chat's cache about to expire | `count`, `after` |
| Underline it as the tab showing | `showing` |
| Make its name stronger, for what needs a look | `needs_a_look` |
| Italicise its name, for a tab that's replaced rather than added to | `replaced` |
| Give it a close button, shown while the pointer is over it | `closes` |
| Do something when it's clicked, or right-clicked | `on_click`, `right_click_menu` |
| Say more than its name, to assistive apps or while the pointer rests on it | `label`, `tooltip` |

## Toggles

`docs/spec/components.md` has a toggle's states, and `crates/look/src/toggles` the code. A toggle draws whether it's on from what the place that holds the setting gives it, and says when it's flipped, so the place sets what it holds and draws it again.

| To | Call |
|---|---|
| Make one, on or off | `look::toggle(id, on)` |
| Do something as it's flipped | `on_flip` |
| Say what an assistive app calls it, such as the setting's name | `accessibility_label` |
| Grey it out, so it can't be flipped | `off` |

## Switches

`docs/spec/components.md` has a switch's kinds, and `crates/look/src/switches` the code. A switch makes its segments itself, as borderless buttons of the size its kind has, so none has a look of its own.

| To | Call |
|---|---|
| Make one, of a kind, with no segments yet | `look::switch(kind)` |
| Add a segment that says words, or that's only an icon, which says what it is while the pointer rests on it, each on while it's chosen, with what choosing it does | `words`, `icon` |
| Say what the segment added last does while the pointer rests on it | `tooltip` |
