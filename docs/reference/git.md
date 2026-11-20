# Git

Facts about git that took an experiment to find, or that more than one place in the code depends on. `worker::git` is the one module that runs git, as `docs/principles/outside-systems.md` says.

## Worktrees

- **Two git commands at once in one repository can fail while one of them is `git worktree add`.** `git worktree add` writes the new worktree's entry in `.git/worktrees/` one file at a time. A command that reads every worktree's entry fails if it finds an entry whose `commondir` file exists and isn't written yet, with `fatal: failed to read .git/worktrees/NAME/commondir`. `git worktree add` is such a command, because it looks for the branch in every other worktree, and `git fetch` is another. Seen on git 2.54. So a worker holds a lock around each of these commands, which `crates/worker/src/git/worktrees/` has.
