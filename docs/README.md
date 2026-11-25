# Docs

Every doc in the repo, what each is for, and who decides it. `docs/standards/documentation.md` says what goes in a doc at all, and `docs/standards/who-decides.md` what it means to decide one.

A folder holds one kind of doc:

| Folder | What its docs hold | Decided by |
|---|---|---|
| `docs/spec/` | What the product does for the person using it, how fast, how it looks, and why | Owner |
| `docs/guide/` | How to use the product, for the person using it | Agents |
| `docs/standards/` | How work is done: how code, tests, docs and pull requests are written, reviewed and merged | Owner |
| `docs/principles/` | The rules the system is built to obey | Owner |
| `docs/decisions/` | One record for each large choice, with what it was chosen over | Owner |
| `docs/reference/` | Facts to look up: what words mean, how an outside system behaves, how to measure | Agents |

A README beside code says how to build, run or change that code, and agents keep it. `README.md` and `AGENTS.md`, at the root, are the owner's.

## Every doc

| Doc | Its one purpose |
|---|---|
| `README.md` | What this is, why it exists, and how to try it |
| `AGENTS.md` | What every session needs: the rules for working here, the everyday commands, and what to read before each kind of task |
| `docs/spec/` | What the product does for the person using it, how fast, how it looks, and why, one file per area |
| `docs/guide/` | How to use Roundhouse, a page for each task and each thing, published as the site's `/docs/` pages |
| `docs/standards/documentation.md` | Where each kind of information goes: a doc, GitHub, the code, or kept private |
| `docs/standards/who-decides.md` | Which decisions are the owner's, how an owner doc changes, and how to propose a change to one |
| `docs/standards/issues.md` | How an issue is written, labelled, scheduled and linked to others |
| `docs/standards/writing.md` | How to word what a person reads: docs, issues, pull requests, commit messages and comments |
| `docs/standards/rust.md` | How Rust is written, whatever the crate |
| `docs/standards/platforms.md` | How code is written so that it runs on macOS, Windows and Linux |
| `docs/standards/gpui.md` | How code that uses GPUI is written |
| `docs/standards/components.md` | How code makes and uses the components screens are built from, and how a component's API is laid out |
| `docs/standards/test-levels.md` | Where each kind of test goes, and the hub it runs on |
| `docs/standards/testing.md` | What a test runs in, and how a test is written |
| `docs/standards/workarounds.md` | The kinds of workaround, and what one needs before it's kept |
| `docs/standards/pull-requests.md` | How a pull request and its commit are written |
| `docs/standards/stacks.md` | Where to split a change into stacked pull requests, and in what order |
| `docs/standards/merging.md` | Which checks a pull request needs, how it merges through the queue, and what to do about a flaky test |
| `docs/standards/dependencies.md` | What a dependency has to meet before it's added: its license, and how its updates arrive |
| `docs/standards/reviews.md` | How agents review a change when the owner asks for a review |
| `docs/standards/manual-checks.md` | What a person has to judge by hand |
| `docs/principles/parts.md` | Which process does which job: hubs, workers, providers and clients |
| `docs/principles/layers.md` | Which crate can depend on which, how code is layered, and how an error crosses the layers |
| `docs/principles/state.md` | Who owns a piece of state, what an app keeps on its device, and how what the hub stores changes form |
| `docs/principles/serving.md` | The four kinds of thing a hub serves, how a client gets them, and what the hub keeps current |
| `docs/principles/events.md` | What an event is, how the hub records one, and how a client reads a session's log |
| `docs/principles/records.md` | What a record is, how one changes, and how a client keeps its copy |
| `docs/principles/observations.md` | What the hub's copy of state others own is, what it holds, and how fresh it says it is |
| `docs/principles/keeping-observations.md` | Where an observation comes from, where the hub keeps it, and how a client keeps its copy |
| `docs/principles/live-snapshots.md` | What something running shows now, which the hub doesn't store, and how it reaches a client |
| `docs/principles/sync.md` | What's common to every kind in a client's copy of a hub's state, and who allocates each ID |
| `docs/principles/protocol.md` | How a protocol's forms change with no new version, so that peers on different builds keep working together |
| `docs/principles/versions.md` | What a break of a protocol is, how two sides agree a version, and which versions work together |
| `docs/principles/failure.md` | What happens when a process dies, and how a slow one is kept from delaying the others |
| `docs/principles/observability.md` | How each part logs, what a log may hold, where logs go, and what metrics and traces will use |
| `docs/principles/outside-systems.md` | How the code deals with a system the project doesn't own, how the hub reads GitHub, and how credentials are kept |
| `docs/principles/agents.md` | How the hub and clients deal with each kind of agent through its adapter, and how an agent uses Roundhouse's tools |
| `docs/decisions/` | One record a file, listed in its `README.md` |
| `docs/reference/terms.md` | What the project's words mean, and which to write |
| `docs/reference/layout.md` | A map of the parts and what connects them, what each crate is, who owns each piece of state, and which kind each thing a hub holds is |
| `docs/reference/commands.md` | Every command for building, testing, running and packaging from a checkout |
| `docs/reference/claude-cli.md` | How the Claude CLI behaves, and how the worker drives it |
| `docs/reference/codex-cli.md` | How Codex's app-server behaves on the wire, and how to probe it |
| `docs/reference/antigravity-cli.md` | How Antigravity's ACP server behaves on the wire, and how the worker drives it |
| `docs/reference/git.md` | What git does that took an experiment to find, or that more than one place in the code depends on |
| `docs/reference/file-watching.md` | What the OS's file watchers do through `notify`, as found by experiment or read from its source |
| `docs/reference/github.md` | How GitHub and `gh` behave, where more than one place in the code depends on it, or it took an experiment to find |
| `docs/reference/launchd.md` | What launchd does to a hub run as a launch daemon, and to the processes it starts, as found by experiment |
| `docs/reference/media-remote.md` | What MediaRemote, the private framework that says what a Mac is playing, does, as found by experiment |
| `docs/reference/conformance.md` | How a version of an agent's CLI is checked against what Roundhouse needs of it, and how to play the scenarios that check it |
| `docs/reference/performance.md` | How to measure the desktop app's speed, and what helps it meet the bars |
| `docs/reference/inspiration.md` | What Conductor does, as a source of ideas |
| `apps/mobile-ios/README.md` | How the iPhone app is tested, built and run on a phone or in the Simulator, and how it's put together |
| `mobile-poc/README.md` | How to build and run the mobile proof of concept |
| `mobile-poc/test-server/README.md` | The simulated hub for testing clients on a phone |
| `third_party/README.md` | What the copies of other projects' code are, how each is changed, and how a change goes upstream and a release comes in |
| `site/README.md` | How to preview the marketing site, and how it follows the app's look |
| `tools/guide/README.md` | How the guide is organized and written, and how it's built and published |
| `infra/README.md` | How the AWS account where RunsOn can run CI's jobs is set up, planned and applied, and how a job uses its runners. *Unconfirmed* |
