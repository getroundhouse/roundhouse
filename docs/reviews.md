# Reviews

How agents review a change before it becomes a pull request. What the PR then says about its reviews is in `docs/pull-requests.md`, and what a person has to judge by hand is in `docs/manual-checks.md`.

Agents write this codebase, and each one copies what it finds. A workaround that gets in becomes the pattern for the next change, so a review asks whether a change is built the way it should be, not only whether it works. An agent that didn't build a change sees what its builder can't, because it isn't anchored on the builder's reasoning.

A review is split into lanes. Each lane is one kind of question, asked by an agent of its own.

## When

The agent that builds a change has it reviewed without being asked to, once the change is built, its tests pass and its PR's description is drafted, and before the full checks and the PR. How much review a change gets follows what it changes:

| Change | Lanes |
|---|---|
| Only Markdown | Docs |
| A small fix in one place, such as a flaky test | Correctness and tests |
| Anything else | Correctness, tests, architecture, cleanliness and docs |

A change that adds or moves state, changes the protocol or what a hub stores, or adds a crate also has its design reviewed, before any code is written.

## How

- **Each lane is a fresh agent,** and the lanes run in parallel.
- **A reviewer gets the task, the diff, the checkout and the PR's description as drafted,** which holds the design, and the reason for any choice a reviewer might question. The diff is from the commit the change is built on, or from the PR below it in a stack, and the brief names that commit and any new file the diff doesn't show. A reviewer doesn't get the builder's chat, so it judges the change by what the change and its description say. Its brief names its lane and points it to this doc, rather than retyping it.
- **A reviewer changes nothing in the builder's checkout.** One that breaks code to see whether a test notices does it in a checkout of its own, with the change in it.
- **A reviewer stays in its lane.** Something it sees outside it gets one line.
- **Findings go back to the builder,** and nothing is filed, as `docs/documentation.md` says of any local review.

## Findings

Each finding says:

- **How much it matters:**
  - *Must fix:* the change is wrong. A bug, a state it can leave inconsistent, a test that can't fail, or a claim in a doc or the PR that isn't true.
  - *Should fix:* it breaks a rule in a doc, or it costs more to fix later than now, such as state with the wrong owner, logic repeated, or a workaround.
  - *Consider:* a judgment call.
- **Where:** the file, and the item or lines.
- **What's wrong,** and why it matters, in a sentence or two.
- **The fix,** concrete enough to act on.

A review that finds nothing worth raising says so. That's a result, and a reviewer doesn't make findings up to look thorough. The bar for *should fix* stays where it is for the same reason: if every round finds something that should be fixed, reviewing never ends.

## Until it's done

1. The builder fixes every finding that must or should be fixed, or says in the PR's description why not, with an issue for anything left open. It takes one to consider when that's cheap and clearly better.
2. Fixes can be wrong too, so the lanes whose findings they fix review again, with fresh agents, on the new diff and description. A finding the description answers isn't raised again unless the answer is wrong.
3. The review is done when every finding that must or should be fixed is fixed or answered, and the last round of fixes was reviewed and raised nothing new that must or should be.
4. After three rounds that still raise some, the builder stops and asks the owner. Findings that keep coming usually mean the design is wrong, not its details.

## Workarounds

Each of these needs a reason: in a comment beside it, where `docs/documentation.md` puts why code is the way it is, so the next agent sees it, and in a paragraph of its own in part 3 of the PR, as `docs/pull-requests.md` says of any choice a reviewer might question. Without one, the change is restructured instead.

- A flag, mode or option added so that one caller works.
- A special case for one value, kind or caller.
- An error swallowed, or a fallback that hides a failure.
- A check for a state the code can't reach. If it can reach it, fix what lets it. A state that comes from outside the process can always arrive, such as from the network, storage, another program, or an older hub or client, and handling it is right.
- Logic copied, or nearly copied, rather than shared.
- A wrapper around code this repo owns, rather than a change to it.
- A sleep in the product's code, a retry of what failed, or a longer timeout, added so that a test passes. A fixed sleep in a test isn't let in even with a reason, as "Tests" in `docs/rust.md` says.

A workaround copied from code nearby isn't let in, even with a reason. As `AGENTS.md` says, the change doesn't copy it, and the one it would copy is fixed if it's part of the change, and otherwise given an issue.

Working around the kit has a rule of its own, and signs of its own, in `docs/gpui.md`: the app builds the piece itself instead.

## Lanes

### Design

Before any code is written, the reviewer reads the change's design: parts 2 and 3 of its PR, written first, where each approach the builder weighed and didn't take has a paragraph of its own, such as **Why X, not Y:**. Changing course costs least now. It asks:

- Is the problem the cause, or a symptom of something deeper?
- Who owns the state, by the test in "State ownership" in `docs/architecture.md`?
- Was the approach that's most structurally right considered? If a smaller one was chosen, is the reason good? A smaller diff isn't a reason on its own.
- Does something in the repo already do this, or could it be extended to?
- Does it fit `docs/architecture.md`, including the parts that aren't built, such as workers on other machines and the iPhone app?
- Does it change what an owner doc says, without saying so?

It ends with a verdict: go ahead, go ahead with changes, or rethink, with the better approach described. A design that needs an owner doc to change follows "Who decides" in `docs/documentation.md`.

### Correctness

Whether the change does what it should, taking for granted that it builds and its tests pass. It looks for:

- Logic that's wrong: conditions, bounds, and assumptions about input. Input that's empty, repeated or very large.
- Timing: an answer that arrives late, out of order or twice, two requests at once, a second client, a reconnect, and a restart of the hub, a worker or the app. "Keeping clients in sync" in `docs/architecture.md` says how each kind of state recovers.
- Versions: a newer client or worker with an older hub, and the reverse, as "Changing the protocol" in `docs/architecture.md` says.
- Failures: whether each one is surfaced, and whether one partway through leaves state consistent.
- What's never let go of: processes, workers, subscriptions, records and tasks kept after nothing needs them.
- In the app, more than the mouse: the keyboard, assistive apps, a narrow window, full screen, and what shows while something loads.
- Comments, docs and the PR's description that say what the code doesn't do.

It leaves style and structure to other lanes, unless they cause a bug.

### Tests

Whether the tests would notice the change breaking. It looks for:

- A test that can't fail: an assertion that holds either way, a wait that passes with no delay, values that happen to match, such as places equal to `seq`s, or a check that something is still there when nothing would have removed it. It can confirm one by breaking the code in a checkout of its own and running the test.
- A fake that hides what the real program does, such as the fake agent where the Claude CLI behaves otherwise. `docs/claude-cli.md` and the CLI's captured output in `crates/agents/tests/` say how it behaves.
- A guarantee tested at the wrong one of the levels in `docs/testing.md`, or tested again at a higher one beyond one example of the wiring, which "Tests" in `docs/rust.md` rules out.
- Tests that break the rules for tests in `docs/rust.md`.
- A test made to pass rather than fixed: a sleep, retry or longer timeout under "Workarounds" without a reason, or what `AGENTS.md` says isn't a fix for a flaky test, such as loosening what it checks.
- An important case with no test.

### Architecture

Whether the change is built the way a strong engineer would build it, not only whether it works. It looks for:

- State with the wrong owner, by "State ownership" in `docs/architecture.md`, such as a client keeping or working out what the hub should record.
- A fix in the wrong place, which patches a symptom whose cause is elsewhere.
- A break in the dependency rules or the layers in `docs/architecture.md`, and new coupling between parts.
- A second way to do what a mechanism already does, where `docs/rust.md` says to change the mechanism instead.
- A crate, module or trait that doesn't earn its place, by "Crates and modules" in `docs/rust.md`, or something specific that should be shared. A type for a value that means something specific always earns its place, as "Types" there says.
- Anything under "Workarounds" without a reason, any workaround copied from code nearby, and any sign of working around the kit that `docs/gpui.md` lists.

When it would have built the change differently, it says how, and what that would take. It separates what has to change now from what's worth an issue.

### Cleanliness

Whether the change is as small and clear as it can be. Findings that remove code come first. It looks for:

- What the change adds that the repo already has: a helper, type, constant, piece of wording or test fixture. It searches for it and says where it is.
- Repetition within the diff.
- Dead code, unused parameters, debugging left in, and stale comments.
- Names that don't say what a thing is, or that use a word in a sense `docs/terms.md` doesn't give it.
- A doc that only repeats an item's name and type, which "Docs" in `docs/rust.md` rules out, and a comment that only repeats the code beside it.
- Functions and files that should be split by responsibility, even under the limits the lints and policy tests enforce.
- Code that breaks a rule in `docs/rust.md` or `docs/gpui.md` that the lints don't enforce.

It skips what `rustfmt` and Clippy catch.

### Docs

Whether every doc is still true after the change, and the PR says only what's true. It looks for:

- A doc the change makes untrue, anywhere in `docs/`, `AGENTS.md` and the READMEs, and docs that now contradict each other.
- A rule in the spec that says more or less than the code does, and *Not built* and *Unconfirmed* marks that are wrong. A choice an agent made in an owner doc is marked *Unconfirmed*.
- What `docs/documentation.md` keeps out of docs, such as a date, "yet" or an issue number.
- A new word, or a new sense of one, missing from `docs/terms.md`.
- The PR's description, when there is one, against `docs/pull-requests.md`: every claim checked, and anything unverified under **Not checked:**.
