# Design

How the product is designed before it's built: where the design is, what has to be settled in it and in what order, how the owner decides, and how code is built from it and checked against it. `docs/reference/brilliant.md` has how Brilliant, the tool it's drawn in, behaves.

**The test:** before code changes how something looks or what a person does with it, could you point to the part of the design that shows it, marked ready? If you couldn't, design it first, or ask.

## Design, then gallery, then app

A change to how the product looks, or to what a person sees and does, goes through three stages. Each starts once the one before is settled.

1. **Design.** It's drawn in Brilliant, in `design/`, and what it does is decided with how it looks, because neither can be judged without the other. The owner marks it ready.
2. **Gallery.** The component is made in `look`, or the screen in its app, and drawn in the gallery from made-up data. It shows what a drawing can't, such as a long title, an empty list, or each width. A question it raises about how something looks goes back to the design. The owner reviews the gallery's images beside the design.
3. **App.** It's wired to what the hub holds. Wiring adds nothing to how it looks. A gap it shows goes back to the design.

**Nothing about how the product looks goes into code unless a ready part of the design shows it, however small the change.** Code that chooses for itself is how many small choices, each sensible alone, add up to a product that isn't one design. A component's own canvas is the reference for it, so using a ready component or pattern as it's drawn needs no drawing of its own.

Three kinds of change need no design: one a person can't see, such as caching, one that puts back what the spec or the design already shows, and one the app settles, as "What the app settles" says.

**When the design lacks something, it's a decision, and isn't worked around.** A screen that needs a component nobody drew, a value the foundations don't have, or a state nobody thought of puts that on the board to decide, as "Deciding" says, and building what depends on it waits. This holds where `docs/standards/who-decides.md` would otherwise let an agent choose and mark it *Unconfirmed*.

## What's designed, in order

Each level is made only from the ones above it, and is settled before anything relies on it.

| Level | What it holds | In code |
|---|---|---|
| Foundations | Colour, type, space, shape and motion, by name | `look`'s foundations |
| Components | Buttons, fields, cards and the rest, each in every kind, size and state | `look`'s components |
| Patterns | How components are put together for one purpose that recurs, such as asking for a decision, or saying something went wrong, and when each form of it applies | Where they're drawn, and `look` where one part can hold it |
| Screens | What each client shows, made of those, with made-up data | `apps/` and `transcript` |

- **A pattern is designed from every use of it at once.** The uses are gathered on one canvas first, from the gallery, so that they're redrawn as one set.
- **Two agents don't change the same settled level at once.** Agents design different parts at the same time once what they share is settled, and the owner says who designs what.
- **Every client is in one design,** with a section for each client's screens, as "Each client" in `docs/spec/look.md` says what they share.

## Where it lives

The design is a Brilliant project in `design/`. It's branched, reviewed and merged with the rest of the repo, so that changes that can't both stand are found as they merge.

| Folder | What it holds |
|---|---|
| `Foundations/` | A canvas for each of the foundations' tables in `docs/spec/look.md`, such as `Colours` |
| `Components/` | A canvas for each of `look`'s components, named as its type is, such as `Chip`, with no folders in it |
| `Patterns/` | A canvas for each pattern, named for what it's for |
| `Screens/<client>/<area>/` | A canvas for each screen, in a folder for its client, such as `Desktop`, and in one named as the file of `docs/spec/` that has the screen, such as `Chats`. Each of its states is a frame, named as its image in the gallery |
| `Board/` | The design's own parts, which `look` doesn't build, such as the status badge |
| `Styles/` | The design system |

- **A canvas holds one component, pattern or screen,** so that branches seldom change the same file.
- **A canvas is placed and named before others use it.** A copy of a component names its master's canvas by path, so moving one rewrites every canvas with a copy of it, which conflicts with each branch that changed one of those.
- **A film is in a `Motion/` folder beside the canvas it plays, and names that canvas by its path,** so that it's found with the part that moves. Brilliant plays a film only from a folder of that name, and writes one there only when it names its canvas by its path.
- **A film's states are frames on the canvas of the part that moves,** beside the part's own.
- **The design system has a value for each reach,** as `look` has, in a mode of its own, `reach`, whose values are `desk` and `handheld`. A token that's different on a handheld has both, and one that's the same has one. A handheld's screen sets its frame to the handheld with `ds(, reach(handheld))`, and what's in it takes the handheld's values, copies of components too. Its canvas isn't switched as a whole, since Brilliant doesn't lay out again what's already on it.
- **What's drawn and what's built have the same names.** A token is named as `look` names it, a component and its kinds, sizes and states as `look`'s types are, and a screen's frame as its image in the gallery. So each can be found from the other, and checked against it.
- **A canvas draws with roles,** the `color.` tokens, and never with the palettes they're made from, since Brilliant makes steps of a palette that nobody chose. A test refuses one that doesn't.
- **Text is in SF Pro Text, with the letter spacing that makes it as wide as the system font,** since Brilliant can't draw the system font by name.
- **Content is made up,** as a fixture's is, since the repo is public.
- **Thumbnails, caches and exports aren't committed.** Brilliant makes them again.
- **No font file is committed.** Each machine installs its own.
- **The project isn't published from Brilliant,** which would keep it synced with a copy in Brilliant's cloud, so that files could change outside git.

## Deciding

Each canvas has a status badge at its top, a copy of one in `Board/Status` named `Status`, and the `Index` canvas lists every canvas with its status. Only the canvas's own badge has that name, so a badge shown in an option of `To decide` is named for the option. `cargo run -p design` writes the index from the badges, so that it's written again after a merge rather than merged, and a test refuses an index that isn't as the badges have it. The status isn't in a canvas's name, since other canvases find a component by its path.

| Status | Meaning |
|---|---|
| Exploring | Being worked on, and not to be built |
| Ready for review | The agent thinks it's settled, and the owner hasn't said so |
| Ready | The owner settled it, and it can be built |

Only the owner marks a canvas ready, or agrees to changing one that is.

- **An open question is drawn, with its options side by side,** made of the real components, in a `To decide` section at the top of the canvas of the part it's about. Each has an ID, what it's between, what each costs, and the one the agent recommends with its reasons.
- **The owner decides there or in the chat.** The decision is applied to the design, and the question moves to the `Decided` section under it, with the choice and the reason.
- **The issue tracks the work.** It names the questions, and its closing comment says what was decided, so that it's found from GitHub.
- **When the design contradicts itself, the spec or the code, don't pick a side.** Put it on the board.
- **What the product does goes in the spec in words,** marked *Not built* until it is, as `docs/spec/README.md` says. The design shows it, and the spec says it.

## Building from it

Read the part's own canvas, and what's open on its board, before writing code. Examples elsewhere only illustrate, and can be behind.

- **Keep the design out of the code.** No comment names a canvas, a decision's ID, or how the code differs from a drawing. A difference is settled in the design.
- **A PR that builds from the design shows the two together,** the drawing beside the gallery's image.
- **A design merges before the code that's built from it,** in a PR of its own, unless the owner asks for one.

## What the app settles

A drawing can't show everything. These are settled in the app, and where one changes what was drawn, the drawing is changed to match:

- how something feels: whatever follows the pointer or a finger, scrolls, streams, or can be interrupted
- what real content does, such as a long reply, or a thousand files
- the keyboard, focus and what an assistive app says
- how text is drawn at each pixel density

Motion is shared between them. What moves, in what order, for how long and on what curve is designed, as a film between two states where it's more than one property, and its numbers are what the code uses.

## Checking one against the other

1. **Values.** A test holds every token in `design/` but the font's to `look`'s value of the same name, at each reach, as the spec's tables are held.
2. **Pictures.** The PR that builds a part shows the gallery's picture beside the design's. The agent fixes what differs, and says what still does and why. The owner looks at both before it merges. They're never compared pixel by pixel, since each draws the edges of letters and the widths of text its own way.

## Merging designs

- **Git merges canvases, as it merges code.** Where one conflicts, the branch that comes second takes `main`'s canvas and makes its change again in Brilliant.
- **A canvas that two branches changed is laid out again and looked at** before it's committed, since a merge leaves sizes as they were on one side.
- **The index is written again with `cargo run -p design`,** rather than its conflict being settled by hand.
- **A test refuses a path to a canvas that isn't there,** in a copy of a component or in a film. A canvas added on a branch from before another moved still names the old path, and a copy whose canvas isn't there draws nothing, and nothing says so.
- **A test refuses a canvas that uses a token the design system doesn't have.** Brilliant draws the value the token last had and says nothing, so two branches that can't both stand would otherwise merge.
- **A conflict in what was designed is the owner's to settle,** as any open question is, and never whichever side came last.
