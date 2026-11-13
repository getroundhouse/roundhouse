# Testing

Where each kind of test goes. How to write a test is in `docs/rust.md`, the commands are in `AGENTS.md`, and what a person has to judge by hand is in `docs/manual-checks.md`. Known gaps are GitHub issues.

| Level | Tests | Where |
|---|---|---|
| A module | Its own rules, without a process or a socket. One whose job is to run a program tests its rules without the program, and that it starts, watches and stops one with a stand-in for it | Unit tests |
| The agent adapter | Normalizing the CLI's captured output | `crates/agents/tests/` |
| The whole backend | What spans processes: restarts, crashes, a real worker, with a fake agent | `apps/daemon/tests/` |
| The window | Clicking and typing through to what's shown | `apps/desktop/tests/` |
| How each screen looks, and the app icon | That each still matches its approved image | `apps/desktop/tests/ui/gallery/` |
| The repo's rules | Dependencies, licenses, file length and lint inheritance, against the real dependency graph | `tools/policy/` |
| The guide's tool | That it publishes each page as a directory, and refuses a link, a heading or a picture that isn't there, against made-up guides. CI's `guide` job builds the real one, since a change to only its Markdown runs no test | `tools/guide/tests/` |
| Publishing updates | That a release signs only a feed it computed, finishes after stopping partway, and reads back as apps would, against a directory standing in for the bucket | `tools/package/test_publish.py` |
| The real CLI | That the fake agent is still honest. By hand, since it costs money | `tools/smoke/` |

The window is tested through its real UI, by clicking and typing in the kit's headless windows, against a real hub process and real git, with only the agent faked. That keeps test code out of the product and windows off the screen, and it catches what a screenshot can't, such as a click that silently does nothing.

The gallery is the exception to the real hub. It draws each screen from a fake hub that answers from fixed data, because a real hub's IDs, times and timing differ from run to run, and an image has to come out the same every time. A screen fails if it keeps changing, if it no longer matches its image in `docs/spec/screens/`, or if its area's file in the spec doesn't show it. The chat's scrollbar shows for two seconds after the chat scrolls, by the wall clock, which the gallery can't hold still, so a screen whose picture takes in the scrollbar waits for it to fade, and no picture shows it. *Unconfirmed.* Accepting a change to a confirmed screen needs the owner's approval, as `docs/spec/README.md` says.

Each screen names its frame, the part of the window its picture shows, from the kinds in `apps/desktop/tests/ui/gallery/frame.rs`. A frame shows as little as shows what the screen is about, so a change to the rest of the window leaves its picture alone. The whole window is for a screen about how the window is laid out, or one with a dialog open. The gallery fails a screen whose frame cuts off an open menu or what a hover step waited for, that has a dialog open without the dialog's frame or the dialog's frame without one, or whose frame names an element it can't find. It can't find the cards and tooltips the kit opens by themselves, so a hover step waits for the whole of what it opens, or the frame names it. A screen whose frame is wrong is kept in `.context/gallery/` to look at, but it isn't compared or accepted. Apart from a line in the chat, found by its label, and the open menu, a frame names the parts of the window the app marks as a `look::Region`. `.region()` gives an element its ID and lets a test find it, in one call, so a part can't be marked without being findable, and a frame can't name one the app doesn't mark. A screen whose frame shows only what's in the chat's column, or opens from it, is drawn in a window wide enough for the column at its widest beside both sidebars at their widest, so no sidebar's width moves its picture, and every other screen at the size the app opens at. A frame on the chat's rows leaves out the scrollbar and the outline in the strip along the pane's edge, unless it names the outline. *Unconfirmed.*
