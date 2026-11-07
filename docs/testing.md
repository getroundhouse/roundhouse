# What's been tested

How the product has been checked, and what that found. How to write tests is in `docs/rust.md`, and the commands are in `AGENTS.md`. Known gaps are GitHub issues, labelled `gap`.

## The backend, through step 1.1

| Layer | What it covers | Result on 2026-11-06 |
|---|---|---|
| Unit and contract tests | The store, git against real repositories, status transitions, and real captured CLI output | Passing. The whole workspace on a Mac, including the end-to-end and UI ones |
| End-to-end tests, `apps/daemon/tests/e2e.rs` | Real hub and worker processes, git and socket, with `examples/fake_claude.rs` as the agent | Passing, in 3 of 3 runs on an idle Mac and 2 of 2 with 16 cores busy |
| Real CLI, `tools/smoke/real-claude.sh` | Step 1.1's done-when, approvals, cancelling, a hub killed mid-turn, a Finder-style launch, archive and restore, and since then changing the model and mode, plans, closing and reopening chats, and questions | 74 of 74 checks passing, for about $0.42. A few cents of that is Haiku, and the rest is the session with a plan and one reply on Sonnet |
| Linux, in Docker | The backend builds without GPUI, and the whole suite passes on aarch64 Linux with the fake agent | Passing, in repeated runs. The real CLI wasn't run on Linux, and neither was the desktop app, which is macOS only for now |
| Licenses, `tools/policy` | Every dependency's declared license is checked against an allowlist | Passing, with no exceptions |

The first test run passed cleanly, and these found the bugs it missed:

- **Breaking behaviors on purpose.** Each of 31 key behaviors and fixes was undone in turn, and a test now catches every one. This found several tests that couldn't fail. Writing a stress test to close one of those gaps exposed that dropping a `Client` never closed its connection.
- **Reading the concurrency code,** which found that status was written after its event was published. That could have left a client waiting forever. It only showed up with an artificial delay, and it's now impossible because both commit together.
- **An independent review** by a separate agent, which reported 19 problems:
  - 14 are fixed, each with a test that fails without the fix.
  - 3 are fixed with no test, because the windows are too narrow to hit: a prompt started during hub shutdown, a call registering just as its connection ends, and a prompt written just as a failed launch is retried.
  - 2 are GitHub issues, labelled `gap`: retrying a prompt after a hub crash, and sessions staying in memory.

Worst of what the review found, all now fixed and each covered by a test:

| Problem | What would have happened |
|---|---|
| `git status` obeyed `status.showUntrackedFiles=no` | A non-forced archive deleted untracked work without warning |
| Commits on a detached HEAD weren't checked | A non-forced archive lost them |
| A `watch::Sender::send` with no receivers loses the value | Stopping an agent that had just exited hung forever, and stopping a hub took about 15 s per worker |
| A killed worker left its agent running | The next prompt started a second agent on the same conversation and worktree |
| A late exit event from an old agent cleared the record of its replacement | The session could stick on Running |
| A prompt sent while an agent was exiting failed on a broken pipe | The user had to send it again |
| One global lock was held while a worker started | A worker that never answered blocked other workspaces and hub status |
| A failed worker start was left registered | The workspace stayed unusable until the hub restarted |
| An event could be read ahead of the deltas before it | A stalled client kept stale partial text on screen |

## The desktop app

- 26 UI tests walk through it by clicking and typing. Breaking 14 parts of the wiring on purpose was caught 13 times, and the other had no effect on behavior, because history loading finds the same data by another route.
- Chats, the pickers, plans and questions added 11 end-to-end tests, 10 UI tests and 4 contract tests on real output. Breaking 28 of the new behaviors on purpose was caught 27 times. The other only hides a duplicate row for cards, which was checked by eye.
- The real CLI ran all of it through `tools/smoke/real-claude.sh`: a live switch to full access and back, a refused model, plan mode and approving a plan, another model on the running agent, a change mid-turn, closing and reopening a chat with its conversation intact, and answering a question.
- Real Claude made a real edit that appeared in the real window, with its diff, a comment and a live approval. Screenshots were checked by eye.
- Real Claude took the `Create PR` prompt through a local bare repository: it committed, pushed, tried `gh`, and explained why that couldn't work without a GitHub remote. **No real pull request has been opened,** because that's outward-facing and nobody asked for one.

**A bug only the real app could show.** The first time the user clicked `New`, nothing happened. The app starts a hub through `cx.background_spawn`, on a background thread with its signals blocked, and blocked signals carry across fork and exec. Rust's `Command` resets SIGPIPE but leaves the mask alone (see `std/src/sys/process/unix/unix.rs` in 1.98). With `SIGCHLD` blocked, tokio never learned that `git` had exited, so creating a workspace hung for good, with a zombie `git` beside the worker. No test saw it, because each one started its hub from a test thread, and the headless executor's `background_spawn` isn't the real one. Blocking `SIGCHLD` by hand gave the same hang, and `trestled` now clears its signal mask first thing, whoever started it. A test starts a hub with every signal blocked. The fix was also tried in the real app: it started a hub, and clicking `New` made a workspace. `New` now shows that it's working, because it showed nothing during those minutes.

**Not checked, and the user is the judge of these:**

- Nobody has used it by hand. Trackpad feel, IME, VoiceOver, selection across messages, Cmd-C and the file picker for adding a repository are all untried.
- The app itself has never had its performance measured, only the transcript test's build of the same components. See `docs/performance.md`.
