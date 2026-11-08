# Documentation

Where each kind of information goes. There are two places: markdown in this repo, for what stays true, and GitHub, for what changes. Docs say how things are, and GitHub tracks what's changing.

**The test:** would it still be true if nobody changed the code or the design? Then it goes in a doc. If it can be finished, closed or go out of date on its own, it goes on GitHub.

## Docs

| Kind | What it holds |
|---|---|
| Product | What this is, who it's for, and why it exists |
| Design | How the system fits together, and why it's built that way |
| Behavior | What the product does for the person using it, and why |
| Standards | How code, tests and docs are written |
| Reference | Facts to look up: what words mean, how an outside system behaves, the bars to hit, what a person checks by hand |
| Instructions | How to build, test, run and install |

A doc describes how things are. It leaves out what was removed or replaced, and how a change was made or checked, since git and the PR keep that. Each doc has one purpose, and something that fits none of them gets a doc of its own rather than a section in a nearby one.

| Doc | Kind | Its one purpose |
|---|---|---|
| `README.md` | Product | What this is, why it exists, and how to try it |
| `AGENTS.md` | Instructions | How to build and test, and the rules for working here |
| `docs/documentation.md` | Standards | Where each kind of information goes |
| `docs/architecture.md` | Design | How the system fits together: parts, repo layout, dependency rules, state ownership, wire format, what happens when a process dies, managing a hub, and providers |
| `docs/ui.md` | Design | Why the desktop app is built on GPUI and gpui-kit, and its transcript on the kit's views |
| `docs/behavior.md` | Behavior | What the product does for the person using it, and why |
| `docs/rust.md` | Standards | How Rust is written |
| `docs/gpui.md` | Standards | How code that uses GPUI is written |
| `docs/testing.md` | Standards | Where each kind of test goes |
| `docs/terms.md` | Reference | What the project's words mean, and which to write |
| `docs/claude-cli.md` | Reference | How the Claude CLI behaves, and how the worker drives it |
| `docs/performance.md` | Reference | The desktop app's speed bars, how to measure against them, and what helps meet them |
| `docs/manual-checks.md` | Reference | What a person has to judge by hand |
| `docs/inspiration.md` | Reference | What Conductor does, as a source of ideas |
| `mobile-poc/README.md` | Instructions | How to build and run the mobile proof of concept |
| `mobile-poc/test-server/README.md` | Instructions | The simulated hub for testing clients on a phone |

## GitHub

| Kind | What it holds | Where |
|---|---|---|
| Work | Features and tasks | Issues. `small-features` collects the small day-to-day features |
| Bugs | Something that's wrong | Issues labelled `bug` |
| Gaps and risks | What's known to be missing, or could go wrong | Issues labelled `gap` or `risk` |
| Questions and decisions | Anything not settled yet, and the discussion that settles it | Issues labelled `decision` or `design` |
| Things to try | What has to be tried by hand or on a device | Issues labelled `try-it` |
| Plans | What comes next, in what order, and why | Milestones |
| Status and results | Progress, and what a run, measurement or review found | Issue comments and PRs |

Anything left open gets an issue, so it isn't dropped, even when a chat or a PR mentions it. A local agent review is the exception: it reports its findings to whoever asked for it and files nothing, and they decide what's worth tracking.

Two things belong in neither place. Why a piece of code is the way it is goes in a comment beside it, and what a change did and how it was checked goes in its PR.

## Where they meet

1. **A decision moves when it's made.** It's discussed in an issue. When the issue closes, its outcome and a short reason go into the doc for that area, in the same PR. The issue keeps the options, the discussion, and who decided and when.
2. **The reason stays with the design.** A doc says why things are as they are, in a sentence or two. The evidence behind it stays in the issue, or in git.
3. **Targets go in docs, and results on GitHub.** "A frame in under 8 ms" is a standard, and "6 ms on a given day" is a result.
4. **A doc can describe design that isn't built, but not progress toward it.** An agreed part that doesn't exist yet is marked once as not built, and where the code does something simpler for now, the doc says what. What's left, and in what order, is on GitHub.
5. **A bug's lesson goes where it'll be seen:** a comment at the fix, or a rule in a standard. The story stays in its issue and PR.
6. **Issues link to docs, and docs don't link to issues.** An issue closes, and a doc that pointed at it goes stale.
7. **Nothing lives only in a chat or an agent's memory.** Whatever a conversation settles goes in a doc or an issue, as part of the same work.

In a doc, these usually mean a sentence belongs on GitHub: a date, a count of runs, "yet", "so far", "still", "next step", who decided something and when, or an issue or milestone number. A version is fine where it says what a fact is about, such as which CLI release behaved that way.
