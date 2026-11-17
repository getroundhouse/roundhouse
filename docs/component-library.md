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
- **Its states are methods named for what they mean,** such as `on`, `off` and `waiting`, rather than the kit's `selected`, `disabled` and `loading`, each taking whether it's in that state, so the state can come straight from what the area holds.
- **What every one of a kind does is built in,** rather than asked for at each place it's made, such as the hand over a button while a press does something, and the arrow while it's off or waiting.
- **What it shows beyond what it's made with goes in named slots,** such as a button's icon beside its words, and what comes after them.
- **How it sits among what's around it is an option named for what it does there,** such as standing out past its own room to line up with text, or growing to share a row. The room around it is its container's.
- **What it does is a handler,** such as what a click or a press does, and what it opens, such as a menu, which the component builds with the kit's.

## Checks

- **Each of a component's tables in `docs/spec/components.md` is held to its names in `look`** by a test, through `written`, as the foundations' tables are.
- **Each component has a sheet in the gallery of every kind in every state,** drawn with the same calls the app makes. A state the pointer brings, such as under the pointer or pressed, is drawn by pointing at each one or pressing it, for real, one at a time, and the sheet takes its pixels from that picture, with `headless::draw_pointing`. The design system's page shows each kind beside its name, cut from the sheet.
- **A test in `look` holds what's built in,** such as which buttons have the hand.
- **`tools/policy`'s `design_system.rs` refuses each of the kit's parts that a component holds outside `look`,** with a test for each, such as `only_the_component_library_uses_the_kits_button`, but in the places its list names, each with why: what a later component will draw, such as the sidebar's rows, and the few buttons with a look of their own.

## Adding a component, or a kind

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

A key in one of the kit's menus or tooltips is the kit's, from the action it's bound to, so it isn't made with `look::key`.

## Fields

`docs/spec/components.md` has a field's kinds and states, and `crates/look/src/fields` the code. A field holds the kit's `InputState`, which the place that keeps what's typed makes, with what the field says while it's empty, and the kit's input draws what's typed inside a frame of `look`'s own, since the kit sets its own room at an input's end, past what a style from outside reaches.

| To | Call |
|---|---|
| Make one, of what an `InputState` holds | `look::field(id, input, kind, size)` |
| Put an icon before what's typed, such as a search's, or something at its end, such as a key | `icon`, `after` |
| Say what an assistive app calls it, where nothing beside it says | `accessibility_label` |
| Mark it off | `off` |
| Share a row, rather than fill the width it's in | `grows` |

## Toggles

`docs/spec/components.md` has a toggle's states, and `crates/look/src/toggles` the code. A toggle draws whether it's on from what the place that holds the setting gives it, and says when it's flipped, so the place sets what it holds and draws it again.

| To | Call |
|---|---|
| Make one, on or off | `look::toggle(id, on)` |
| Do something as it's flipped | `on_flip` |
| Say what an assistive app calls it, such as the setting's name | `accessibility_label` |
| Grey it out, so it can't be flipped | `off` |
