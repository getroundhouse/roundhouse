# Testing

Where each kind of test goes. How to write a test is in `docs/rust.md`, the commands are in `AGENTS.md`, and what a person has to judge by hand is in `docs/manual-checks.md`. Known gaps are GitHub issues.

| Level | Tests | Where |
|---|---|---|
| A module | Its own rules, without a process or a socket | Unit tests |
| The agent adapter | Normalizing the CLI's captured output | `crates/agents/tests/` |
| The whole backend | What spans processes: restarts, crashes, a real worker, with a fake agent | `apps/daemon/tests/` |
| The window | Clicking and typing through to what's shown | `apps/desktop/tests/` |
| How each screen looks, and the app icon | That each still matches its approved image | `apps/desktop/tests/ui/gallery/` |
| The repo's rules | Dependencies, licenses, file length and lint inheritance, against the real dependency graph | `tools/policy/` |
| The real CLI | That the fake agent is still honest. By hand, since it costs money | `tools/smoke/` |

The window is tested through its real UI, by clicking and typing in the kit's headless windows, against a real hub process and real git, with only the agent faked. That keeps test code out of the product and windows off the screen, and it catches what a screenshot can't, such as a click that silently does nothing.

The gallery is the exception to the real hub. It draws each screen from a fake hub that answers from fixed data, because a real hub's IDs, times and timing differ from run to run, and an image has to come out the same every time. A screen fails if it keeps changing, if it no longer matches its image in `docs/spec/screens/`, or if its area's file in the spec doesn't show it. Accepting a change to a confirmed screen needs the owner's approval, as `docs/spec/README.md` says.
