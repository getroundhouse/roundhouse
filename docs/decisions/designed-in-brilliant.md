# The product is designed in Brilliant before it's built

**Status:** Decided. Not built.
**First recorded:** 2026-11-25, in #2725.

How the product looks and what a person does with it are designed in Brilliant, a design tool agents and the owner both work in, and settled there before code. Its files are text, in `design/`, branched and merged with the code. Code is built from what's ready, in the gallery and then in the app, as `docs/standards/design.md` says.

- **It's where the whole can be seen and worked on.** Every use of a pattern can be side by side, and changing one component changes every copy of it. Options drawn one feature at a time are each judged alone, so nothing settles what recurs across features: something that needs the person, or went wrong, is shown in eight ways.
- **Agents design better in a design tool,** as the owner has found. In code, the smallest change that passes is what's rewarded, and an option can only recombine the components there are.
- **Trying something bold costs what trying something timid does,** since no component or state has to exist first.
- **The gallery shows that what was built is what was designed,** rather than being where it's designed.
- **The files are the repo's.** Two branches' designs that can't both stand are found as they merge, as code's are, and a test can read them.

## Chosen over

- **Options drawn by the real app,** as screens were decided before. What's chosen is exactly what ships, but each is seen by itself, and nothing that has no component can be drawn.
- **Figma.** It's the more proven, and has comments. Its files aren't in the repo, a test can't read them without its service, and reading them through its tools is limited by rate.
- **A design repo of its own, checked out once.** Everyone would see one copy, unfinished work included. A design and its code would be in two repos, and two designs that can't both stand would overwrite each other rather than conflict.
- **A dedicated designer.** Settling what's shared before agents work in parallel, with the owner choosing who designs what, keeps the whole together without one.

## What it costs

- **What's designed is drawn twice,** in Brilliant and then in code, and the two can part. Names that match, and checks, hold them together.
- **Nothing it draws goes in the app,** which is Rust on GPUI.
- **Brilliant is young.** `docs/reference/brilliant.md` has what's been found, such as fonts that stay wrong until it restarts.
- **Git merges a canvas by line,** so some changes that could both stand conflict, and the branch that comes second makes its own again.
- **A contributor needs Brilliant,** and Apple's fonts, to change a design.
- **The design is ahead of the app,** while it's made again from the ground up from what's there.
