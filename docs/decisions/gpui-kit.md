# The apps are built on gpui-kit, Longbridge's component kit for GPUI

**Status:** Decided.
**First recorded:** 2026-11-06, in #78.

The apps draw with gpui-kit, the component kit for GPUI that Longbridge makes, and uses in an app it ships, Longbridge Pro.

- **It has most of what the app needs:** messages, a message scroller, diffs, a sidebar, docks, a command palette, settings, native menus, variable-height virtual lists, a code editor, and Markdown.
- **It runs UI tests headless,** so the app's tests click and type without a window on screen.
- **It's under Apache-2.0.**

Building the kit from a copy in the repo is a choice of its own, which [gpui-kit-copy.md](gpui-kit-copy.md) records.

## Chosen over

No alternative was recorded.
