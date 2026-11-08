# Testing

Where each kind of test goes. How to write a test is in `docs/rust.md`, the commands are in `AGENTS.md`, and what a person has to judge by hand is in `docs/manual-checks.md`. Known gaps are GitHub issues.

| Level | Tests | Where |
|---|---|---|
| A module | Its own rules, without a process or a socket | Unit tests |
| The agent adapter | Normalizing the CLI's captured output | `crates/agents/tests/` |
| The whole backend | What spans processes: restarts, crashes, a real worker, with a fake agent | `apps/daemon/tests/` |
| The window | Clicking and typing through to what's shown | `apps/desktop/tests/` |
| The repo's rules | Dependencies, licenses, file length and lint inheritance, against the real dependency graph | `tools/policy/` |
| The real CLI | That the fake agent is still honest. By hand, since it costs money | `tools/smoke/` |

The window is tested through its real UI, by clicking and typing in the kit's headless windows, against a real hub process and real git, with only the agent faked. That keeps test code out of the product and windows off the screen, and it catches what a screenshot can't, such as a click that silently does nothing.
