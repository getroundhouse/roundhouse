# Workspaces

## Creating and setting up

**Before there's a repository.** *Unconfirmed.*

![No repositories](screens/first-run.png)

- **A workspace's name is generated from its first prompt,** and the sidebar shows it, so telling workspaces apart doesn't mean opening each one. *Not built.*
- **Setup runs in the background once the worktree exists.** It copies gitignored files matching `.worktreeinclude`, then `file_include_globs`, then `.env*`, and runs `scripts.setup`. A prompt sent meanwhile is recorded at once and delivered when setup finishes. A failure warns, and never blocks. Restoring a workspace runs it again. The matching is git's own, so patterns mean what they do in `.gitignore`. *Unconfirmed.*

**A new workspace whose setup script failed, with its log open.** *Unconfirmed.*

![A failed setup](screens/setup-failed.png)

## Settings from Conductor

A repository set up for Conductor works here unchanged, because Trestle reads its settings and sets its variables under Conductor's names. There are no names of Trestle's own for these.

- **Settings:** `.conductor/settings.local.toml` in the repository, then `.conductor/settings.toml` in the worktree, then `~/.conductor/settings.toml`. The first to set a value wins.
- **Scripts:** `[scripts]` with `setup` and `archive`. Run scripts, `run_mode` and managed settings aren't read.
- **Files to copy:** `.worktreeinclude`, then `file_include_globs`, then the default `.env*` pattern.
- **Variables** for scripts and agents: `CONDUCTOR_WORKSPACE_NAME`, `CONDUCTOR_WORKSPACE_ID`, `CONDUCTOR_WORKSPACE_PATH`, `CONDUCTOR_ROOT_PATH`, `CONDUCTOR_DEFAULT_BRANCH`, `CONDUCTOR_PORT`, the first of ten ports for the workspace, and `CONDUCTOR_IS_LOCAL`.

## Archiving

- **Archiving removes the worktree and keeps the branch,** following it if the agent renamed it. It refuses if there are uncommitted changes, or commits on no branch, unless forced. That's checked before anything changes, and again once the agents have stopped. Ignored files such as `.env` and `node_modules` go with the worktree. Committed work can't be lost, and uncommitted work can't be lost by accident. *Unconfirmed.*

## Pull requests

- **`Create PR` asks the agent to commit, push and run `gh pr create`,** and the app shows the result from `gh pr view`. The agent knows why the changes were made, so it writes the better description. *Unconfirmed.*
