# Pull requests

How a pull request is written: its title, its description, its labels, and the commit it becomes on `main`. How a change is split into several is in `docs/standards/stacks.md`. What belongs in a PR rather than a doc is in `docs/standards/documentation.md`. Which checks to run and how PRs merge are in `docs/standards/merging.md`.

A PR is read twice. The owner reads it to decide whether it's right, and someone later finds its commit through `git log` or `git blame`. So it says what changed and why, then how it was checked. Every PR puts the same things in the same places, so neither reader has to learn each PR's own layout.

## Title

- **Start with a verb, and say what's true afterwards,** in the words of `docs/reference/terms.md`, such as "Let a hub refuse a request it doesn't know". If someone using the app would notice the change, say it the way they'd see it: "Show a downloaded update as a card at the bottom of the sidebar". Otherwise, say what the code or the repo does now.
- **A fix says what works now, not "Fix".** "Stop setup even when Stop comes before its script has started" says what was broken and that it works now. "Fix setup race" says neither.
- **No prefix, no issue number, and no full stop at the end.** No `feat:` and no area, since the diff shows those. GitHub adds the PR's number when it merges. Use sentence case, and write the interface's labels as they appear, such as "Check for Updates…".
- **At most 70 characters.** It becomes the first line of the commit on `main`, and GitHub cuts that line off at about that length. A title that won't fit may be two changes, and `docs/standards/stacks.md` gives independent changes a PR each.

## Description

The description says what the PR does as it stands now. When the PR changes, rewrite the description rather than adding to it, so a reader doesn't need the PR's pushes, its reviews, or the chat that led to it.

The parts come in this order. The last four start with their label in bold, written exactly as here, so the owner finds each in the same place every time. Leave out a part with nothing in it, except **Checked:**.

1. **Its place in the stack, if it's in one.** The first line says which PR of the stack this is, and what the stack as a whole does, such as "The second of two stacked PRs that take native dialogs out of updating, on top of the one for Check for Updates…".
2. **What was wrong or missing, and what's true now.** A paragraph or two. Where someone using the app would notice, start with what they'd see, and name the code after. A PR that finishes an issue says `Closes #N`, or `Fixes #N` for a bug, so merging closes the issue. A PR that does only part of one says `Part of #N`.
3. **How it works, and why.** One bold-led paragraph or bullet for each part of the change, saying what that part does and why. A choice a reviewer might question gets a paragraph of its own, such as **Why the test changes, not the hub:**. A PR that adds state, or moves it, has a paragraph that starts **Where the state lives:**, saying which part owns it and why, by the test in `docs/principles/state.md`, so the owner sees that choice without finding it in the diff. Name a file where that helps the review.
4. **A picture of what changes on screen.** Link a screen from the gallery at the PR's commit, so the link keeps working after the branch is deleted: `https://github.com/getroundhouse/roundhouse/blob/<commit>/docs/spec/screens/<screen>.png?raw=true`. Attach a screenshot of the app for anything the gallery can't show. Either way, the alt text says what the picture shows.
5. **Needs the owner:** which owner docs, or files that enforce a standard, change, and what each change is, so the owner can approve without hunting through the diff. List what's marked *Unconfirmed*, and say which changes the owner already agreed to in conversation. This part is there exactly when the PR has the `needs/owner` label.
6. **Checked:** what was run, and what it showed:
   - the full checks from `docs/standards/merging.md`, or that the PR changes only Markdown or the marketing site and needs none;
   - for a change to what the product does, which pages of `docs/guide/` changed with it, or that none says what changed, since no test fails when a guide page goes stale;
   - each test that changed, and what it now covers, and for each new test, what it covers and why no test there already could take it as a case, as "Writing tests" in `docs/standards/testing.md` asks;
   - what was broken on purpose, and which test failed;
   - anything checked by hand, with the machine, the OS, what was done and what was seen;
   - any review the owner asked for, as `docs/standards/reviews.md` has it: which lanes it took, whether the fixes were checked, what was found that changed the PR, any must fix answered rather than fixed, and the issues filed for bugs passed on. The fixes themselves go in part 3, with the rest of the change, rather than as a list of review findings. A stack reviewed whole says what that review found in its top PR.
7. **Not checked:** what wasn't verified, and why, so nothing unverified reads as checked. Leave this out only when everything was checked.
8. **Left open:** anything a reader might expect the PR to do that it doesn't, each with its issue, since `docs/standards/documentation.md` gives everything left open an issue.

Word it as `docs/standards/writing.md` says, and claim only what you checked. Its length follows the size of the change and what the reader has to judge, so a change of a few lines gets a few short paragraphs. Whatever needs the owner's judgment, such as a refusal that now takes longer, goes in part 2 and not further down. Each item under **Checked:** is what was run and what it showed, in a line or two. Code, files, commands and test names go in backticks. Leave out numbers nobody will act on, such as how many tests ran, and plans beyond the issues in **Left open:**. Nothing that `docs/standards/documentation.md` keeps private goes in a PR or a commit, since they'd go public with the repo.

## Labels

A PR gets the `needs/owner` label when it changes an owner doc or a file that enforces a standard, as `docs/standards/issues.md` says. It gets no other label and no milestone. Kinds, areas and milestones sort issues, and the issue a PR closes already has its own. The only search over PRs is the owner's approvals, which needs only `needs/owner`.

## The commit

GitHub squashes each PR into one commit on `main`, and builds that commit's message from the branch.

- **Its title is the PR's title.**
- **Its message says what changed and why,** in a few short paragraphs of plain text, with no Markdown, wrapped at 72 columns. It condenses parts 2 and 3 of the description, since it's what `git log` and `git blame` show. How it was checked, the pictures, and what needs the owner stay in the PR.
- **It ends with `Closes #N` or `Fixes #N`** when the PR does.
