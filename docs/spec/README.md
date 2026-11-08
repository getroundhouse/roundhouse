# Product spec

What the product does for the person using it, how fast, how it looks, and why, one file per area. The owner decides it, and `docs/documentation.md` says how it changes.

A rule can end with *Not built*, when it's decided and not in the product yet, or *Unconfirmed*, when an agent chose it and the owner hasn't confirmed it.

| File | Covers |
|---|---|
| `workspaces.md` | Creating, setting up and archiving workspaces, and opening a pull request |
| `chats.md` | Chats, the agent's questions, the model picker, and their keys |
| `diff.md` | The diff, and comments on it |
| `sidebar.md` | The sidebar |
| `updates.md` | What someone sees when the app or its hub updates, or when there's no hub |
| `look.md` | The rules every screen follows: surfaces, type, space, buttons, colour and the status mark |
| `speed.md` | How fast the desktop app has to be |

## How it looks

Each area's file shows its screens beside the rules they go with, because what a feature does and how it looks are decided together, and read together. A screen is a picture rather than words: the owner decides the look by looking at it, so the record of each decision is the picture itself. The images are kept in `screens/`. The real app draws every screen from made-up data, and a test fails when a screen no longer matches its image, or its area's file doesn't show it, so the look can't change without someone seeing it.

A screen is confirmed the same way as a rule. One an agent adds, or changes before the owner has approved it, ends with *Unconfirmed*. Changing a confirmed screen's image needs the owner's approval, in the PR that changes it, where GitHub shows the old image beside the new one.

An image is exact about everything, including what nobody chose, such as how text is smoothed. So a GPUI upgrade or a font change can change confirmed images too, and those need approving the same way.

The rules every screen follows, such as spacing, type and colour, are in `look.md`, the way `speed.md` holds the bars every area meets.

## How a feature is designed

What a feature does and how it looks are settled in one issue, labelled `kind/question` and `needs/owner`, because neither can be judged without the other.

1. An agent sketches two to four options. Each is a few rules and the screens that show them, drawn by the real app on a branch that's never merged, and posted in the issue with `gh issue comment --attach`. A flow across several screens can start as a diagram.
2. The owner picks one in the issue, or asks for another round.
3. One PR builds it: the code, its tests, and the rules and screens in the area's file. It closes the issue, and the owner approves the rules, the images and the code there together.

When building waits, the rules go in marked *Not built*, and the chosen screens stay in the issue until it's built, because every image here is one the app draws.
