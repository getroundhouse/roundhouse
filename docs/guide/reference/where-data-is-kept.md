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
| `hub.db` | Every project, workspace and chat, each chat's history, and which pull request is each workspace's |
| `backups/` | Copies of the database from before each migration |
| `github-cache.db` | What the hub last read from GitHub, such as each workspace's pull request and the issues and pull requests chats link to, how many lines each workspace has added and removed, as git last said, who each agent's account found on a machine is, its email, plan and login, what's left of each account's limits, and the models Codex offers in each project, as agents last said, so it has them as it starts. The hub empties each part of it whose form an update changes, and deleting it while the hub isn't running loses only what's said again: an account's email and plan show again once an agent next launches under its login, and its limits once an agent under it next says them |
| `attachments/` | The files prompts carried, and the images and videos replies point to, as they were when each reply came |
| `bundles/` | Unpushed commits saved when archiving workspaces whose provider destroys clones on archive, removed once restored |
| `logins/` | The tokens of the logins you've added in [Settings](settings.md#agents), a file each, which only you can open. Disconnecting one removes its file |
| `github-logins/` | The tokens of the GitHub accounts you've added in [Settings](settings.md#git), and the key of each app of your own, a file each, which only you can open. For `gh`'s login, only which GitHub user it's for, since the hub reads its token from `gh`. Disconnecting an account, or removing an app, removes its file |
| `hub.log` | The hub's log |
| `worker-logs/` | Each workspace's worker's log |
| `instances/` | A directory for each workspace's worker, with the workspace's setup log, which its archive script adds to. While an agent or the setup script runs, the directory also holds a record of it, so it can be stopped if the worker is killed, and while a Claude agent runs, the files its CLI reads, such as the one that tells it where Roundhouse's tools are |
| `daemons/` | A copy of the hub's program, `Roundhouse Hub`, for each version that has started a hub, which that version's hub and its workers run from. Once the app, or the command line, starts a hub, it removes each copy that nothing runs from anymore and that's more than a day old, except its own version's. A workspace still at work on the version before keeps that version's copy until it [moves to the new one](updates.md#what-an-update-does). On Windows the hub itself runs from `daemons\Roundhouse Hub.exe`, whichever version it is, so Windows Firewall asks once whether a paired phone may reach it, not after every update |
| `settings.json` | The app's [settings](settings.md) |
| `window.json` | Where the app's window was left, so it opens there again |
| `updates/` | On Linux, the updates the app has downloaded |
| `dictation/` | The model that hears what you dictate, 2.4 GB, once you turn dictation on, in a folder for the version it is. Time Machine leaves it out, since it can be downloaded again. Turning dictation off keeps it, and Delete the model, under Dictation in [Settings](settings.md), frees the room |
| `speaking/` | The voice conversation mode speaks with, about 65 MB, downloaded once speech is on, in a folder for the version it is. Time Machine leaves it out, since it can be fetched again |

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

- **The app's check for updates,** and, once you turn [dictation](../guides/dictate-a-prompt.md) on, its download of the model from Hugging Face, and once you turn speech on, to [talk with an agent](../guides/talk-with-the-agent.md), of the voice. Those are its only requests of its own. What you dictate or say is heard on your Mac, and what the agent says is spoken there, so no sound leaves it.
- **What your agent sends,** to its model provider, through the Claude, Codex or Antigravity CLI under its login. Codex and Antigravity keep their native sign-ins and conversation files in their own data directories.
- **Your prompts' words and their first images, to write a title,** in a separate call through that chat’s CLI and login: Haiku for Claude, Luna at Low effort for Codex, and Gemini 3.8 Flash at Low effort for Antigravity.
- **What `git` and `gh` send,** to your remotes and to GitHub, as the default GitHub account in [Settings](settings.md#git) on the machines Roundhouse sets up GitHub on, and as your own login on your own machine.
- **The app's manifest, as you make an app of your own,** which your browser posts to GitHub, from a page the app serves on your machine, on `127.0.0.1` alone, until GitHub sends you back with a code.
- **The hub's calls to GitHub,** once you add a GitHub account: to ask whose a token is and when it ends, to sign in, to renew a sign-in's token, and once a day to ask again who each account you added is, by the name and email its GitHub profile shows, which agents' commits in repositories on GitHub are by. For an app of your own, it makes the app from GitHub's code, looks for where it's installed, and makes its installation's tokens. While `gh`'s login on the hub's machine is an account you added, the hub runs `gh` there once a minute to read its token, and asks GitHub whose it is when it changes, as it does each time Connect account opens. And each time the hub starts, `gh` asks GitHub, once on each machine, whether it takes that machine's own login, which Git's Found says.

## Going back a version

An older version can't open a database a newer one has migrated. That's what `backups/` is for: the hub saves a copy before each migration, named for the schema version it holds. The repository's [README](../../../README.md#installing-a-build-of-your-own) says how to go back to one.
