# Where data is kept

Everything Roundhouse keeps is on your machine. There's no Roundhouse account, and nothing is sent to us.

## The hub's data

| Platform | Directory |
|---|---|
| Mac | `~/Library/Application Support/roundhouse` |
| Windows | `%LOCALAPPDATA%\roundhouse-data` |
| Linux | `$XDG_DATA_HOME/roundhouse`, or `~/.local/share/roundhouse` |

In it:

| | |
|---|---|
| `hub.db` | Every project, workspace and chat, and each chat's history |
| `backups/` | Copies of the database from before each migration |
| `attachments/` | The files prompts carried |
| `logins/` | The tokens of the logins you've added in [Settings](settings.md#accounts), a file each, which only you can open. Disconnecting one removes its file |
| `hub.log` | The hub's log |
| `worker-logs/` | Each workspace's worker's log |

## Worktrees

Each workspace's worktree is in `~/roundhouse/workspaces`. It's an ordinary git worktree of your repository, so its branch is in your repository too.

In a worktree, Roundhouse uses `.context/`, which git is told to ignore:

- Agents keep what they make for their own use there, in folders named for what they hold, such as `.context/screenshots/`.
- `.context/attachments/` holds the files prompts carried.

## Choosing other places

Two environment variables move them, for a hub started with them set:

| Variable | Moves |
|---|---|
| `ROUNDHOUSE_HOME` | The hub's data |
| `ROUNDHOUSE_WORKSPACES` | The worktrees |

## What leaves your machine

- **The app's check for updates,** which is its only request of its own.
- **What your agent sends,** to its model provider, through the Claude or Codex CLI under its login. Codex keeps its native sign-in and conversation files in its own data directory.
- **Your prompts' words and their first images, to write a title,** in a separate call through that chat’s CLI and login: Haiku for Claude, Luna at Low effort for Codex.
- **What `git` and `gh` send,** to your remotes and to GitHub, as you.

## Going back a version

An older version can't open a database a newer one has migrated. That's what `backups/` is for: the hub saves a copy before each migration, named for the schema version it holds. The repository's [README](../../../README.md#installing-a-build-of-your-own) says how to go back to one.
