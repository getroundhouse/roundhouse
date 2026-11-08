# Product spec

What the product does for the person using it, how fast, how it looks, and why, one file per area. The owner decides it, and `docs/documentation.md` says how it changes.

A rule can end with *Not built*, when it's decided and not in the product yet, or *Unconfirmed*, when an agent chose it and the owner hasn't confirmed it.

| File | Covers |
|---|---|
| `workspaces.md` | Creating, setting up and archiving workspaces, and opening a pull request |
| `chats.md` | Chats, the agent's questions, the model picker, and their keys |
| `sidebar.md` | The sidebar |
| `updates.md` | What someone sees when the app or its hub updates |
| `speed.md` | How fast the desktop app has to be |
| `screens.md` | How each screen of the desktop app looks |

## How it looks

How each screen looks is a picture rather than words: `screens.md` shows each screen's approved image, kept in `screens/`. The owner decides the look by looking at it, so the record of each decision is the picture itself. The real app draws every screen from made-up data, and a test fails when a screen no longer matches its image, so the look can't change without someone seeing it.

A screen is confirmed the same way as a rule. One an agent adds, or changes before the owner has approved it, ends with *Unconfirmed*. Changing a confirmed screen's image needs the owner's approval, in the PR that changes it, where GitHub shows the old image beside the new one.

An image is exact about everything, including what nobody chose, such as how text is smoothed. So a GPUI upgrade or a font change can change confirmed images too, and those need approving the same way.
