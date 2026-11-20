# Documentation

Where each kind of information goes. There are two places: markdown in this repo, for what stays true, and GitHub, for what changes. Docs say how things are, and GitHub tracks what's changing. A few kinds of information go in neither, and are kept private instead.

**The test:** would it still be true if nobody changed the code or the design? Then it goes in a doc. If it can be finished, closed or go out of date on its own, it goes on GitHub.

Two things belong in neither place. Why a piece of code is the way it is goes in a comment beside it, and what a change did and how it was checked goes in its PR, written as `docs/standards/pull-requests.md` says.

## Docs

`docs/README.md` lists every doc, by the folder it's in. A folder holds one kind of doc, and says who decides it.

A doc describes how things are. It leaves out what was removed or replaced, and how a change was made or checked, since git and the PR keep that. Each doc has one purpose, and something that fits none of them gets a doc of its own rather than a section in a nearby one. A purpose is narrow enough when someone doing the task the doc is for needs all of it. Each doc also has a word limit, and each table cell another, which a test in `tools/policy` holds them to, so adding to a full doc means cutting or moving something. A doc that was longer when the limits came keeps a ceiling of its own, which only comes down. The spec and the guide have no limit, which is *Not built*.

The guide is the one kind written for someone using the product rather than building it. It decides nothing: everything it says the app does is a rule in the spec, or where the spec doesn't reach, such as what installing needs and the command line, what the README and the code say. So agents keep it, and change a page in the PR that changes what it rests on.

`AGENTS.md` is read at the start of every session, so it stays lean: it holds what every task needs, and points to the docs for the rest.

## Where a sentence goes

The spec says what the product does, in product and design terms only. Everything about how it's built goes to one of four places. Ask these in order, and stop at the first yes:

| Question | It goes in | Decided by |
|---|---|---|
| Would a person using the product notice if it were different? | The spec | Owner |
| Does the same choice recur across features? | A principle, in `docs/principles/` | Owner |
| Is it a one-off choice among real alternatives, such as a framework? | A decision record, in `docs/decisions/` | Owner |
| Is it a fact about a system this project doesn't own, such as GitHub or an agent's CLI? | A reference doc | Agents |
| None of those | The code, with a comment where the reason isn't plain from it | Agents |

Three things the questions leave open:

- **A person notices in ordinary use.** The spec says the behavior, and a value only where someone using the app could tell it had changed, such as how long a terminal's shell gets before it's killed. A value they couldn't tell, such as what a program is told the terminal is, stays in the code. So does a fix for something that would otherwise break.
- **What's planned and not designed is an issue.** A rule marked *Not built* says what the thing will do. Where all that's known is that something is missing, such as links in a terminal, an issue holds it.
- **A fact about an outside system gets a reference doc when more than one place depends on it, or when it took an experiment to find.** Otherwise it's a comment at the one place that depends on it.

So no doc describes how a part works. The code says that, and an agent can explain it from the code when someone asks. A doc holds what the code can't say: what the system is built to obey, why a choice was made and what it was chosen over, what's decided and not built, and what an outside system does.

A decision record is written once, with its date, and only its status changes afterwards. When the choice changes, a new record replaces it and the old one says which.

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

## Kept private

The product's name and trademarks, its domains and accounts, and other legal or business matters go in the private repo, `getroundhouse/private`, and nowhere in this one: not in a doc, an issue, a PR or a commit message. This repo is public, and so are its history, issues and PRs. Research into other companies' names and marks shouldn't be, and neither should plans that would tip someone off before a name is held. What's already public is the exception: the marketing site's address, `https://getroundhouse.dev`, is on the repo's About box and linked from the README, so a doc, an issue or a PR can name it. A domain nobody has been shown stays private, and so does how any domain or account is held. The private repo's issues follow the same rules as this repo's. Work here mentions such a matter only as far as the code needs, such as the rename itself.

## Where they meet

1. **Nothing lives only in a chat or an agent's memory.** Whatever a conversation settles goes in a doc or an issue, as part of the same work.
2. **A decision moves when it's made.** It's discussed in an issue. When the issue closes, its outcome and a short reason go where the table above puts them, in the same PR. The issue keeps the options, the discussion, and who decided and when.
3. **The reason stays with the design.** A doc says why things are as they are, in a sentence or two. The evidence behind it stays in the issue, or in git.
4. **A bug's lesson goes where it'll be seen:** a comment at the fix, or a rule in a standard. The story stays in its issue and PR.
5. **Issues link to docs, and docs don't link to issues.** An issue closes, and a doc that pointed at it goes stale. A doc can link to a decision record, which doesn't change. When a doc or section that an issue links to moves or goes, the same change fixes the issue.
6. **Targets go in docs, and results on GitHub.** "A frame in under 8 ms" is a standard, and "6 ms on a given day" is a result.
7. **A doc can describe design that isn't built, but not progress toward it.** An agreed part that doesn't exist yet is marked once as not built, and where the code does something simpler for now, the doc says what. What's left, and in what order, is on GitHub.

In a doc, these usually mean a sentence belongs on GitHub: a date, a count of runs, "yet", "so far", "still", "next step", who decided something and when, or an issue or milestone number. A decision record is the exception: it carries its date and the PR that made it. A version is fine where it says what a fact is about, such as which CLI release behaved that way.

