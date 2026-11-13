# Product spec

What the product does for the person using it, how fast, how it looks, and why, one file per area. The owner decides it, and `docs/documentation.md` says how it changes.

A rule can end with *Not built*, when it's decided and not in the product yet, or *Unconfirmed*, when an agent chose it and the owner hasn't confirmed it.

| File | Covers |
|---|---|
| `workspaces.md` | Creating, setting up and archiving workspaces, their titles, and their pull requests |
| `chats.md` | Chats, their titles, the agent's questions, the model picker, plan mode, the context and the account, and their keys |
| `diff.md` | The diff, and comments on it |
| `files.md` | All files, which lists every file in a workspace, and a file opened from it |
| `sidebar.md` | The sidebar |
| `home.md` | Home, which lists every workspace |
| `updates.md` | What someone sees when the app or its hub updates, or when there's no hub |
| `menus.md` | The menus, on a Mac and off it, and the keys off the Mac |
| `settings.md` | Settings, which shows in place of the workspace, and what each setting does |
| `look.md` | The design system's foundations, the rules every screen follows: colour, type, space and shape, buttons, the pointer and the status mark |
| `speed.md` | How fast the desktop app has to be |

## How it looks

Each area's file shows its screens beside the rules they go with, because what a feature does and how it looks are decided together, and read together. A screen is a picture rather than words: the owner decides the look by looking at it, so the record of each decision is the picture itself. The images are kept in `screens/`. The real app draws every screen from made-up data, and a test fails when a screen no longer matches its image, or its area's file doesn't show it, so the look can't change without someone seeing it.

Each image shows only what its screen is about, such as a sidebar, the composer, or what's in the chat, so a change shows in the images of what it changed rather than in every image of the window. A few show the whole window: those about how it's laid out, and those with a dialog, which dims all of it. The screens about what's in the chat, such as its rows and the composer, are drawn in a window wide enough that the chat's column is at its widest whatever the sidebars' widths, so widening a sidebar doesn't change them. The rest, which are as wide as the window lets them be, such as the tabs, are drawn at the size the app opens at, and a sidebar's width changes those. *Unconfirmed.*

A screen is confirmed the same way as a rule. One an agent adds, or changes before the owner has approved it, ends with *Unconfirmed*. Changing a confirmed screen's image needs the owner's approval, in the PR that changes it, where GitHub shows the old image beside the new one.

A screen that shows a rule goes under it, in the rule's list item, with a caption of its own, so each rule is read beside the pictures of what it says, and a rule that describes several states has a screen for each. A screen about no one rule, such as the window before there's a project, goes on its own, after the rules it goes with. *Unconfirmed.*

`cargo run -p spec-page` draws the spec as a page for a browser, rule by rule, each beside the screens that show it, so an area is read and confirmed at one sitting, and with only what isn't confirmed if asked. A rule shows its words in bold, and the rest of it a click away, and says when it has no picture. It shows nothing the spec doesn't. It takes a screen's caption to be the paragraph right above it that starts in bold, or above the run of images it's in, and a screen with no caption shows as having none. A screen in a rule's list item, under the rule, shows beside that rule, and one on its own shows beside its caption. *Unconfirmed.*

An image is exact about everything, including what nobody chose, such as how text is smoothed. So a GPUI upgrade or a font change can change confirmed images too, and those need approving the same way.

The rules every screen follows, such as spacing, type and colour, are in `look.md`, the way `speed.md` holds the bars every area meets.

## How a feature is designed

What a feature does and how it looks are settled in one issue, labelled `kind/question` and `needs/owner`, because neither can be judged without the other.

1. An agent sketches two to four options. Each is a few rules and the screens that show them, drawn by the real app on a branch that's never merged, and posted in the issue with `gh issue comment --attach`. A flow across several screens can start as a diagram.
2. The owner picks one in the issue, or asks for another round.
3. One PR builds it: the code, its tests, and the rules and screens in the area's file. It closes the issue, and the owner approves the rules, the images and the code there together.

When building waits, the rules go in marked *Not built*, and the chosen screens stay in the issue until it's built, because every image here is one the app draws.
