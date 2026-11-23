# Reviews

How agents review a change when the owner asks for a review. What the PR then says about its reviews is in `docs/standards/pull-requests.md`, and what a person has to judge by hand is in `docs/standards/manual-checks.md`.

Agents write this codebase, and each one copies what it finds. A workaround that gets in becomes the pattern for the next change, so a review asks whether a change is built the way it should be, not only whether it works. An agent that didn't build a change sees what its builder can't, because it isn't anchored on the builder's reasoning. So a review is a fresh look at what's complicated, not a check on what the builder can verify itself, such as whether a test fails when the code it covers breaks.

What a review weighs most is the structure: who owns each piece of state, the interfaces and abstractions between parts, and how a failure in one part reaches the others. These cost the most to change later, and a sound structure rules out whole classes of bugs. A bug in one place, in a structure that's right, costs about as much to fix later as now, so a review holds a change for one only when the change doesn't do what it's for, or the bug does harm that can't be undone.

A review is split into lanes. Each lane is one kind of question. A reviewer is a fresh agent that takes one lane or several, since each reviewer has to learn the change before it can judge it, and lanes that share a reviewer share that cost.

## When

An agent reviews a change, or has one reviewed, only when the owner asks, in the lanes the owner names. When the owner names none, one fresh reviewer takes docs and design for a change in words alone, and architecture, correctness and tests for any other.

## How

- **The reviewers run in parallel.**
- **A reviewer gets the task, the diff, the checkout, and the PR's description if one is drafted,** which holds the design, and the reason for any choice a reviewer might question. When none is, the brief says what the change is for and why it's built as it is, rather than a description being drafted for the review. The diff is from the commit the change is built on, or from the PR below it in a stack, and the brief names that commit and any new file the diff doesn't show. A reviewer doesn't get the builder's chat, so it judges the change by what the change and its brief say. Its brief names its lanes and points it to this doc, rather than retyping it.
- **A stack's reviewer gets the diff from the commit the stack is built on to its top,** naming any PR of it that has merged, the issue that holds the stack's plan, each PR's description, and the design review's verdict. It's a fresh agent rather than the design's reviewer, which approved the plan.
- **A reviewer reads the description to learn the change,** rather than checking it claim by claim. Where the description and the code disagree about what the change does, it says which is wrong: code that doesn't do what the description promises is a finding against the code, and a description that's wrong is a minor one.
- **A reviewer changes nothing in the builder's checkout, and makes no checkout of its own.** It judges a test from the code, and from what the builder says it broke on purpose. A test it doubts is a finding that asks the builder to break the code and see.
- **A reviewer stays in its lanes.** Something it sees outside them gets one line.
- **Findings go back to the builder,** and the reviewer files nothing, as `docs/standards/documentation.md` says of any local review.

## Findings

- *Must fix:*
  - **The structure is wrong,** by what the Architecture lane looks for in a change's structure. A bug is a sign of it when a better structure would have made it impossible to write, when its fix changes something other code relies on, such as a signature, a type, an owner, the protocol or a stored form, or when the same mistake would be easy to make elsewhere. Its fix is the structure, not a patch at the bug. A doubt about the structure is a must fix even when the reviewer isn't sure, so that it gets an answer.
  - **The change doesn't do what it's for:** the case it was built for doesn't work, or a test of that case can't fail.
  - **Harm that can't be undone,** of the kinds the Correctness lane lists.
  - **A rule other work is built to says what isn't true,** since agents copy a rule as they copy code.
- *Minor:* anything else worth raising, in a line each with where it is: a local slip, which is a bug in one place in a structure that's right, a sentence of a comment or the description that isn't true, or a small break of a standard.

Each must fix says:

- **Where:** the file, and the item or lines.
- **What's wrong,** and why it matters, in a sentence or two.
- **What makes it necessary:** the requirement or assumption behind it, and whether dropping that, and accepting what it costs, would be simpler than the fix.
- **The fix,** concrete enough to act on.

A review that finds nothing worth raising says so. That's a result, and a reviewer doesn't make findings up to look thorough. The bar for *must fix* stays where it is for the same reason: a fresh reviewer can always find one more thing that could be better.

After its findings, each reviewer's report has a part headed **For the owner:**, which holds what the owner would want to know that isn't a finding against this change: a problem in the code nearby that the change didn't cause, or a way to make this part of the product or its code better. A doubt about this change's structure is a finding, not one of these. A reviewer with nothing to put there says so.

## Until it's done

1. The builder keeps the diff its reviewers saw, and checks each finding against the code, since a reviewer can be wrong. It fixes each must fix, or answers it: why the finding is wrong, or which downside the change accepts instead, and why that's better. The answer goes in a paragraph of its own in part 3 of the PR's description if there is one, and otherwise in its report. A must fix whose fix would take the change in another direction is the owner's to decide, so the builder leaves it and asks the owner, with its question tool.
2. It fixes each minor finding whose fix is a few lines and plainly right. A bug it doesn't fix gets an issue labelled `kind/bug`, so the owner can weigh it later, and the rest it passes on are dropped.
3. The builder checks its fixes as it checks the rest of its work, such as by breaking one on purpose to see its test fail. A fresh agent checks them only when the owner asks.

A second review of the whole change, by fresh reviewers, is the owner's to ask for. Fresh reviewers find new things each time, so a review that waits for them to find nothing doesn't end.

## What the owner hears

The owner hears about a review once, when it's done, in one report from the builder, rather than in a note as each reviewer finishes. The builder waits for no answer but the one to the question "Until it's done" has it ask. A review leads to no full checks, commit or PR of its own: the builder goes on with what it was asked to do. The report says:

- **What was found and fixed,** must fix first, in a line or two each.
- **What was answered rather than fixed:** each must fix the builder judged wrong, and each downside the change accepts, with why.
- **What's the owner's to confirm:** a fix that changed an owner doc, with what the builder recommends.
- **What the reviewers put under For the owner,** passed on whole, with what the builder thinks of each.
- **What was filed,** as issues, including each bug passed on, since nothing lives only in a chat.
- **How much review it took:** how many reviewers ran, in which lanes, and whether the fixes were checked.

## Lanes

### Design

Before any code is written, the reviewer reads the change's design: parts 2 and 3 of its PR, written first, where each approach the builder weighed and didn't take has a paragraph of its own, such as **Why X, not Y:**. Changing course costs least now. It asks:

- Is the problem the cause, or a symptom of something deeper?
- Who owns the state, by the test in `docs/principles/state.md`?
- How does a failure spread? For each thing the design waits on that can be slow, fail or never answer, what limits the wait, and what else waits behind it?
- Which requirement makes the design hard, and would dropping it, and accepting what that costs, be simpler?
- Was the approach that's most structurally right considered? If a smaller one was chosen, is the reason good? A smaller diff isn't a reason on its own.
- Does something in the repo already do this, or could it be extended to?
- Does it fit `docs/principles/`, for the iPhone app as well as the desktop, and including the parts that aren't built, such as workers on other machines?
- Does it change what an owner doc says, without saying so?
- What does it commit, and in which PR: the forms it stores or sends, any migration, and any protocol break, as `docs/principles/versions.md` says?
- For a stack, are its splits and their order right, by `docs/standards/stacks.md`?

A rule changed in words alone is read the same way, the rule being its design, and the reviewer also asks where an agent following it would go wrong, on either side of its line.

It ends with a verdict: go ahead, go ahead with changes, or rethink, with the better approach described. A design that needs an owner doc to change follows `docs/standards/who-decides.md`.

### Correctness

Whether the change does what it's for, and whether it can do harm that can't be undone, taking for granted that it builds and its tests pass. It looks for:

- Logic that's wrong in the case the change was built for: conditions, bounds, and assumptions about input.
- Stored data lost or garbled, including by a migration run on whatever an older build could have stored, and on a large store, and by code that reads rows an older build wrote.
- A person's work lost, such as a prompt or an agent's output, an action taken on someone's behalf that can't be taken back, such as a push, a merge or a delete, and a credential exposed.
- A newer client or worker with an older hub, and the reverse, as `docs/principles/versions.md` says, since a build can't be changed once it's published.

It doesn't hunt for edge cases beyond these. One it comes across is a minor finding, unless it's a sign that the structure is wrong, as "Findings" says.

### Tests

Whether the tests would notice the change breaking. It looks for:

- A test that can't fail: an assertion that holds either way, a wait that passes with no delay, values that happen to match, such as places equal to `seq`s, or a check that something is still there when nothing would have removed it.
- A fake that hides what the real program does, such as the fake agent where the Claude CLI behaves otherwise. `docs/reference/claude-cli.md` and the CLI's captured output in `crates/agents/tests/` say how it behaves.
- A guarantee tested at the wrong one of the levels in `docs/standards/test-levels.md`, or tested again at a higher one beyond one example of the wiring, which "Writing tests" in `docs/standards/testing.md` rules out.
- A new test that's a case of one already there, with the same setup and one input or assertion changed, or that tests again, for one more caller, what's tested where it's defined, both of which "Writing tests" in `docs/standards/testing.md` rules out. It names the test that should have taken the case.
- A test made to pass rather than fixed: a sleep, retry or longer timeout that `docs/standards/workarounds.md` lists, without a reason, or what `docs/standards/merging.md` says isn't a fix for a flaky test, such as loosening what it checks.
- No test of the case the change was built for, or of a failure the structure is meant to contain, such as one part that never answers while the rest carry on.

### Architecture

Whether the change is built the way a strong engineer would build it, not only whether it works, and is as small and clear as it can be. Findings that remove code come first. In the change's structure, it looks for:

- State with the wrong owner, or more than one writer, by `docs/principles/state.md`, such as a client keeping or working out what the hub should record.
- A failure that spreads: a wait with no limit on something that can be slow or never answer, such as a worker, GitHub, the store or a lock, or something parts meant to be independent share, such as a loop, a task or a budget, so one stuck part holds up the rest.
- Something with no owner to end it: a process, worker, subscription, task or record kept after nothing needs it.
- An interface that's easy to call wrong, or an abstraction that leaks what it should hide.
- A fix in the wrong place, which patches a symptom whose cause is elsewhere.
- A break in the dependency rules or the layers in `docs/principles/layers.md`, and new coupling between parts.
- A second way to do what the repo already does: a mechanism, where `docs/standards/rust.md` says to change the mechanism instead, or a helper, type, constant, piece of wording or test fixture. It searches for it and says where it is.
- A crate, module, file, trait or function that doesn't earn its place, or does more than one job, by "Crates and modules" in `docs/standards/rust.md`. A type for a value that means something specific always earns its place, as "Types" there says.
- A requirement that makes the change harder than it's worth, where accepting a downside would be simpler.
- A rule other work is built to that the change adds or rewrites, judged as part of its design, and, when no reviewer has the docs lane, one the change makes untrue.
- Anything `docs/standards/workarounds.md` lists, without a reason, any workaround copied from code nearby, and any sign of working around the kit that `docs/standards/gpui.md` lists.

In how the change is written, it looks for:

- Repetition within the diff.
- Dead code, unused parameters, debugging left in, and stale comments.
- Names that don't say what a thing is, or that use a word in a sense `docs/reference/terms.md` doesn't give it.
- A doc that only repeats an item's name and type, which "Docs" in `docs/standards/rust.md` rules out, and a comment that only repeats the code beside it.
- Code that breaks a rule in `docs/standards/rust.md` or `docs/standards/gpui.md` that the lints don't enforce. It skips what `rustfmt` and Clippy catch.

When it would have built the change differently, it says how, and what that would take.

### Docs

Whether every doc is still true after the change. It looks for:

- A doc the change makes untrue, anywhere in `docs/`, `AGENTS.md` and the READMEs, and docs that now contradict each other.
- A rule in the spec that says more or less than the code does, and *Not built* and *Unconfirmed* marks that are wrong. A choice an agent made in an owner doc is marked *Unconfirmed*.
- What `docs/standards/documentation.md` keeps out of docs, such as a date, "yet" or an issue number.
- A new word, or a new sense of one, missing from `docs/reference/terms.md`.
- Text that breaks `docs/standards/writing.md`, in a doc, a comment or the PR's description: a thing described where it has a name, an everyday word standing in for a technical one, an opening that starts from the code rather than from what happens, or more than the reader will act on. It quotes the passage and gives the rewrite, or says what to cut.

### Stack

Once a stack is built, the reviewer reads it as one change, against its design. It leaves what one PR's diff shows to that PR's reviewers. It looks for:

- Where the built change departs from its design, and whether it was right to.
- A shape that suits what fills it but not what reads it, or the reverse: a field nothing reads, a reader working out what the writer knew, a value converted twice, or one thing named differently in two layers.
- State with the wrong owner, by `docs/principles/state.md`, that only shows with both layers in view.
- Code that exists only for the middle of the stack, beyond what `docs/standards/stacks.md` lets in, and any of it left at the top.
- Two forms of one shape at the top, where "Each message type has one serialized form" in `docs/principles/protocol.md` asks for one, and readers of an old form the top no longer needs.
- A pairing of versions in `docs/principles/versions.md` that the stack breaks, and a migration that makes a different form from the live path given the same input.
- No test that follows the data across the layers.
- A doc that isn't true at the top of the stack.
