# Where data is kept

Each hub keeps its workspaces and chats on its machine. A desktop connected to another hub reads and controls that machine's state. There's no Roundhouse account, and nothing is sent to us.

Saved hub names, addresses and the selected hub are in the desktop's `settings.json`. Paired-device tokens are in its operating system's credential store. A hub keeps `hub-cert.der` and `hub-key.der` in its data directory; its database holds only hashes of paired-device tokens.

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
| `logins/` | The tokens of the logins you've added in [Settings](settings.md#agents), a file each, which only you can open. Disconnecting one removes its file |
| `github-logins/` | The tokens of the GitHub accounts you've added in [Settings](settings.md#git), a file each, which only you can open. Disconnecting one removes its file |
| `hub.log` | The hub's log |
| `worker-logs/` | Each workspace's worker's log |
| `instances/` | A directory for each workspace's worker, with the workspace's setup log, which its archive script adds to. While an agent or the setup script runs, the directory also holds a record of it, so it can be stopped if the worker is killed, and while a Claude agent runs, the files its CLI reads, such as the one that tells it where Roundhouse's tools are |
| `daemons/` | A copy of the hub's program, `Roundhouse Hub`, for each version that has started a hub, which that version's hub and its workers run from. Once the app, or the command line, starts a hub, it removes each copy that nothing runs from anymore and that's more than a day old, except its own version's. A workspace still at work on the version before keeps that version's copy until it [moves to the new one](updates.md#what-an-update-does) |
| `settings.json` | The app's [settings](settings.md) |
| `window.json` | Where the app's window was left, so it opens there again |
| `updates/` | On Linux, the updates the app has downloaded |
| `dictation/` | The model that hears what you dictate, 2.4 GB, once you turn dictation on, in a folder for the version it is. Time Machine leaves it out, since it can be downloaded again. Turning dictation off keeps it, and Delete the model, under Dictation in [Settings](settings.md), frees the room |

## Worktrees

Each workspace's worktree is in `~/roundhouse/workspaces`. It's an ordinary git worktree of your repository, so its branch is in your repository too.

In your repository's git directory, Roundhouse keeps an empty file, `roundhouse-worktrees`, which it locks while it changes the repository's worktrees, so that two workspaces being made at once take turns. Leave it there.

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

- **The app's check for updates,** and, once you turn [dictation](../guides/dictate-a-prompt.md) on, its download of the model from Hugging Face. Those are its only requests of its own. What you dictate is heard on your Mac, and never leaves it.
- **What your agent sends,** to its model provider, through the Claude, Codex or Antigravity CLI under its login. Codex and Antigravity keep their native sign-ins and conversation files in their own data directories.
- **Your prompts' words and their first images, to write a title,** in a separate call through that chat’s CLI and login: Haiku for Claude, Luna at Low effort for Codex, and Gemini 3.8 Flash at Low effort for Antigravity.
- **What `git` and `gh` send,** to your remotes and to GitHub, as the default GitHub account in [Settings](settings.md#git), or each machine's own login.
- **The hub's calls to GitHub,** once you add a GitHub account: to ask whose a token is and when it ends, to sign in, and to renew a sign-in's token. And each time the hub starts, `gh` asks GitHub, once on each machine, whether it takes that machine's own login, which Git's Found says.

## Going back a version

An older version can't open a database a newer one has migrated. That's what `backups/` is for: the hub saves a copy before each migration, named for the schema version it holds. The repository's [README](../../../README.md#installing-a-build-of-your-own) says how to go back to one.
