# Documentation

Where each kind of information goes. There are two places: markdown in this repo, for what stays true, and GitHub, for what changes. Docs say how things are, and GitHub tracks what's changing.

**The test:** would it still be true if nobody changed the code or the design? Then it goes in a doc. If it can be finished, closed or go out of date on its own, it goes on GitHub.

Two things belong in neither place. Why a piece of code is the way it is goes in a comment beside it, and what a change did and how it was checked goes in its PR.

## Docs

| Kind | What it holds |
|---|---|
| Product | What this is, who it's for, and why it exists |
| Spec | What the product does for the person using it, how fast, and why |
| Architecture | How the system fits together, and why it's built that way |
| Standards | How code, tests and docs are written |
| Reference | Facts to look up: what words mean, how an outside system behaves, the bars to hit, what a person checks by hand |
| Instructions | How to build, test, run and install |

A doc describes how things are. It leaves out what was removed or replaced, and how a change was made or checked, since git and the PR keep that. Each doc has one purpose, and something that fits none of them gets a doc of its own rather than a section in a nearby one.

| Doc | Kind | Decided by | Its one purpose |
|---|---|---|---|
| `README.md` | Product | Owner | What this is, why it exists, and how to try it |
| `docs/spec/` | Spec | Owner | What the product does for the person using it, how fast, and why, one file per area |
| `docs/architecture.md` | Architecture | Owner | How the system fits together: parts, repo layout, dependency rules, state ownership, wire format, what happens when a process dies, managing a hub, and providers |
| `docs/ui.md` | Architecture | Owner | Why the desktop app is built on GPUI and gpui-kit, and its transcript on the kit's views |
| `docs/documentation.md` | Standards | Owner | Where each kind of information goes, and who decides it |
| `docs/rust.md` | Standards | Owner | How Rust is written, whatever the crate. A rule about one part of the system goes in that part's doc |
| `docs/gpui.md` | Standards | Owner | How code that uses GPUI is written |
| `docs/testing.md` | Standards | Owner | Where each kind of test goes |
| `docs/manual-checks.md` | Reference | Owner | What a person has to judge by hand |
| `docs/terms.md` | Reference | Agents | What the project's words mean, and which to write |
| `docs/claude-cli.md` | Reference | Agents | How the Claude CLI behaves, and how the worker drives it |
| `docs/performance.md` | Reference | Agents | How to measure the desktop app's speed, and what helps it meet the bars |
| `docs/inspiration.md` | Reference | Agents | What Conductor does, as a source of ideas |
| `AGENTS.md` | Instructions | Owner | How to build and test, and the rules for working here |
| `mobile-poc/README.md` | Instructions | Agents | How to build and run the mobile proof of concept |
| `mobile-poc/test-server/README.md` | Instructions | Agents | The simulated hub for testing clients on a phone |

`AGENTS.md` is read at the start of every session, so it stays lean: it holds what every task needs, and points to the docs for the rest.

## GitHub

| Information | What it holds | Where |
|---|---|---|
| Work | What to build, fix or do | Issues |
| Questions and decisions | Anything not settled yet, and the discussion that settles it | Issues |
| Risks | What could go wrong | Issues |
| Plans | What comes next, in what order, and why | Milestones, and issues that order other issues |
| Status and results | Progress, and what a run, measurement or review found | Issue comments and PRs |

Anything left open gets an issue, so it isn't dropped, even when a chat or a PR mentions it. A local agent review is the exception: it reports its findings to whoever asked for it and files nothing, and they decide what's worth tracking.

An issue's description says where things stand, and changes when they do. Its comments are the record, and each one says what it holds, not where it came from.

### Labels

Labels answer three questions, so any common search is one filter: what kind of issue it is, which part of the system it's in, and whether only the owner can move it.

**Kind: exactly one.** It says how the issue closes. An issue with no kind hasn't been triaged.

| Label | What it is | It closes when |
|---|---|---|
| `kind/bug` | Something built that behaves wrongly, including under failure | It's fixed, with a test |
| `kind/feature` | Something a person using the product would notice is missing | It's built |
| `kind/task` | Work no user sees: tooling, docs, setup, refactors, code brought up to a standard | It's done |
| `kind/question` | Something not settled, whether a design or a decision | It's answered, and the outcome is in a doc |
| `kind/risk` | Something that could go wrong, watched rather than worked on | It's mitigated or accepted |
| `kind/plan` | An issue that breaks work into parts, or orders other issues | What it holds is closed |

**Area: one or more.** Areas follow the parts in the layout in `docs/architecture.md`, such as `area/hub` and `area/transcript`, and small or unbuilt parts share one, such as `area/mobile` for the iOS app and the relay. Each area's description on GitHub says what it covers. Four cover what isn't a part: `area/install` for building, installing and updating the app, `area/dev` for developing Trestle itself, `area/docs`, and `area/project` for the name, license, scope and overall plans. Nothing else becomes an area.

**`needs/owner`:** only the owner can move it, because it needs their decision, their account or device, or their judgment by hand. Agents leave these alone. A PR that changes an owner doc, or a file that enforces a standard, gets it too, and waits for the owner's approval.

Everything else uses GitHub's own fields rather than a label:

- **When:** a milestone. An issue with no milestone is in the backlog, and there are no priority labels.
- **What a plan holds:** a plan that breaks one piece of work into parts uses sub-issues, which nest and show progress. A plan that orders or selects issues with their own reasons to exist uses a checklist, since an issue can be in several.
- **Blocked:** one issue waiting on another, without being part of it, is a "blocked by" link.
- **Duplicate, or won't do:** the reason it was closed.

## Where they meet

1. **Nothing lives only in a chat or an agent's memory.** Whatever a conversation settles goes in a doc or an issue, as part of the same work.
2. **A decision moves when it's made.** It's discussed in an issue. When the issue closes, its outcome and a short reason go into the doc for that area, in the same PR. The issue keeps the options, the discussion, and who decided and when.
3. **The reason stays with the design.** A doc says why things are as they are, in a sentence or two. The evidence behind it stays in the issue, or in git.
4. **A bug's lesson goes where it'll be seen:** a comment at the fix, or a rule in a standard. The story stays in its issue and PR.
5. **Issues link to docs, and docs don't link to issues.** An issue closes, and a doc that pointed at it goes stale. When a doc or section that an issue links to moves or goes, the same change fixes the issue.
6. **Targets go in docs, and results on GitHub.** "A frame in under 8 ms" is a standard, and "6 ms on a given day" is a result.
7. **A doc can describe design that isn't built, but not progress toward it.** An agreed part that doesn't exist yet is marked once as not built, and where the code does something simpler for now, the doc says what. What's left, and in what order, is on GitHub.

In a doc, these usually mean a sentence belongs on GitHub: a date, a count of runs, "yet", "so far", "still", "next step", who decided something and when, or an issue or milestone number. A version is fine where it says what a fact is about, such as which CLI release behaved that way.

## Who decides

The owner decides what the product is: what it does, how it looks, how fast it is and how it should feel. The owner also decides its high-level architecture: the parts, how they fit together, and the big technical choices, such as the UI framework. And the owner decides the standards everything is built to, including the files that enforce them: the lints in `Cargo.toml` and `clippy.toml`, `rustfmt.toml`, `rust-toolchain.toml`, and `tools/policy`. Agents decide everything else, and keep the reference docs, which record facts they find.

An owner doc, or a file that enforces a standard, changes only with the owner's approval, given in the PR or in conversation. The spec, the architecture and the standards take one kind of change without it. When work needs a choice they don't make, an agent makes the smallest reasonable one and adds it marked *Unconfirmed*, in the same PR. Confirming a rule, and changing or removing a confirmed one, are the owner's.

Agents push and merge as the owner's GitHub account, so GitHub can't tell them apart. This holds by convention, and nothing enforces it.

Two marks show where an owner doc and the product stand apart. *Not built* ends a rule that's decided and not in the product yet. *Unconfirmed* ends one an agent chose that the owner hasn't confirmed.

What the owner has decided isn't fixed. An agent that sees a better way, in the product, the architecture or a standard, says so rather than quietly working around it or going along with it. A small change goes in a PR that changes the doc, with the reasoning, and waits for the owner. A larger one is an issue labelled `kind/question` and `needs/owner`, saying what's decided, what would be better, the evidence, what switching costs, and a recommendation. Until the owner agrees, the work follows what's decided. Every rule gives its reason, so it can be challenged, and a proposal the owner turned down comes back only with something new.

Four searches cover everything waiting on the owner:

| Queue | What's in it | Search |
|---|---|---|
| Decisions | Questions only the owner can answer | [Issues with `needs/owner` and `kind/question`](https://github.com/zlangbert/trestle/issues?q=is%3Aissue+is%3Aopen+label%3Aneeds%2Fowner+label%3Akind%2Fquestion) |
| Tasks | What only the owner can do, such as trying something by hand | [Issues with `needs/owner` that aren't questions](https://github.com/zlangbert/trestle/issues?q=is%3Aissue+is%3Aopen+label%3Aneeds%2Fowner+-label%3Akind%2Fquestion) |
| Unconfirmed | Agents' choices in owner docs, waiting to be confirmed | [*Unconfirmed* in `docs/`](https://github.com/search?q=repo%3Azlangbert%2Ftrestle+path%3Adocs+Unconfirmed&type=code). It also finds this doc and the spec's README, which define the mark |
| Approvals | PRs that change an owner doc or a standard | [PRs with `needs/owner`](https://github.com/zlangbert/trestle/pulls?q=is%3Apr+is%3Aopen+label%3Aneeds%2Fowner) |

From a terminal, `gh issue list -l needs/owner` shows the first two, `git grep Unconfirmed docs` the third, and `gh pr list -l needs/owner` the last.
