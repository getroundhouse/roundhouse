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
| `github-cache.db` | What the hub last read from GitHub, such as each workspace's pull request, the issues and pull requests chats link to, whether the default GitHub account can push to each repository, each GitHub account's username and the name and email its commits are by, and what GitHub calls each app of your own and where it's installed, how many lines each workspace has added and removed, and its branches, as git last said, who each agent's account found on a machine is, its email, plan and login, what's left of each account's limits, the models Codex offers in each project, how full each chat's context is, what its prompt cache holds and which MCP servers its running agent has, as agents last said, and what each open wait last read of its pull requests, so it has them as it starts. The hub empties each part of it whose form an update changes, and deleting it while the hub isn't running loses only what's said again: a GitHub account's username is asked of GitHub again within a minute, a workspace's branches are said again once its worker next runs, an account's email and plan show again once an agent next launches under its login, its limits once an agent under it next says them, a chat's context ring and its cache's clock once its agent next works, its MCP servers as its agent's next turn starts, and why a wait's last read failed once it's read again |
| `attachments/` | The files prompts carried, and the images and videos replies point to, as they were when each reply came |
| `bundles/` | Unpushed commits saved when archiving workspaces whose provider destroys clones on archive, removed once restored |
| `logins/` | The tokens of the logins you've added in [Settings](settings.md#agents), a file each, which only you can open. Disconnecting one removes its file |
| `github-logins/` | The tokens of the GitHub accounts you've added in [Settings](settings.md#git), and the key of each app of your own, a file each, which only you can open. For `gh`'s login, only which GitHub user it's for, since the hub reads its token from `gh`. Disconnecting an account, or removing an app, removes its file |
| `tool-servers/` | The keys of the MCP servers you've added in [Settings](settings.md#mcp-servers), and the sign-ins to them, which are the tokens that keep you signed in, where they're renewed, what the service said it allows, what else a server wants to be allowed, and what its service registered Roundhouse as, where it did, a file each, which only you can open. Removing a server removes its file, and so does saving another key or sign-in for it, or none. Each machine agents are on has the keys, and the tokens that end soon, in memory, to send them, and in no file |
| `hub.log` | The hub's log, a line of JSON for each thing it says. Once it's over 10 MB, the hub next starts a new one, and keeps the last as `hub.log.1` |
| `app.log` | The app's log, kept the same way |
| `worker-logs/` | Each workspace's worker's log, kept the same way |
| `worker-endpoint.json` | The key the workers of a provider other than the local one know the hub by, and the port they dial it at, which only you can open. The hub makes it the first time it gives such a worker a ticket |
| `instances/` | A directory for each workspace's worker, with the workspace's setup log, which its archive script adds to. While an agent or the setup script runs, the directory also holds a record of it, so it can be stopped if the worker is killed, and while a Claude agent runs, the files its CLI reads, such as the one that tells it where Roundhouse's tools are |
| `daemons/` | A copy of the hub's program, `Roundhouse Hub`, for each version that has started a hub, which that version's hub and its workers run from. Once the app, or the command line, starts a hub, it removes each copy that nothing runs from anymore and that's more than a day old, except its own version's. A workspace still at work on the version before keeps that version's copy until it [moves to the new one](updates.md#what-an-update-does). On Windows the hub itself runs from `daemons\Roundhouse Hub.exe`, whichever version it is, so Windows Firewall asks once whether a paired phone may reach it, not after every update |
| `settings.json` | The app's [settings](settings.md) |
| `window.json` | Where the app's window was left, so it opens there again |
| `preferences.json` | What the app remembers from how you use it: which workspace it was left on, for each hub it has shown, so it opens on it again |
| `updates/` | On Linux, the updates the app has downloaded |
| `dictation/` | The model that hears what you dictate, 2.4 GB, once you turn dictation on, in a folder for the version it is. Time Machine leaves it out, since it can be downloaded again. Turning dictation off keeps it, and Delete the model, under Dictation in [Settings](settings.md), frees the room |
| `speaking/` | The voice conversation mode speaks with, about 65 MB, downloaded once speech is on, in a folder for the version it is. Time Machine leaves it out, since it can be fetched again |
| `recordings/` | What you dictate, once you turn on Keep recordings in [Settings](settings.md): a folder for each recording, which only you can open, with its audio, about 4 MB a minute, and a file of what was written, the words it was heard with, and what those were picked from: what you'd typed, the question or proposal you were answering, and the names of your files, branches, workspace, repository and pull request. Beside them is what you sent from where you dictated, which what you only typed isn't in. There's no limit on how much room they take, and backups copy them. Turning it off keeps them, and **Delete recordings** deletes them |

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

- **The app's check for updates,** and, once you turn [dictation](../guides/dictate-a-prompt.md) on, its download of the model from Hugging Face, and once you turn speech on, to [talk with an agent](../guides/talk-with-the-agent.md), of the voice. Those are its only requests of its own. What you dictate or say is heard on your Mac, and what the agent says is spoken there, so no sound leaves it. Recordings you chose to keep stay on it too.
- **What your agent sends,** to its model provider, through the Claude, Codex or Antigravity CLI under its login. Codex and Antigravity keep their native sign-ins and conversation files in their own data directories.
- **Your prompts' words and their first images, to write a title,** in a separate call through that chat’s CLI and login: Haiku for Claude, Luna at Low effort for Codex, and Gemini 3.8 Flash at Low effort for Antigravity.
- **What `git` and `gh` send,** to your remotes and to GitHub, as the default GitHub account in [Settings](settings.md#git) on the machines Roundhouse sets up GitHub on, and as your own login on your own machine.
- **What agents ask of the MCP servers you've added in [Settings](settings.md#mcp-servers),** with each server's key, or its sign-in's token, from the machine the agent is on, to that server.
- **The hub's calls for an MCP server's tools,** to the server, as you add one with a key or with none, or change its URL or its key, with the key if it has one.
- **The hub's calls to sign in to an MCP server, and to renew that,** to the server and to the service it's signed in to, as you sign in, and then each time a token is about to end or a server refuses one. Your browser opens the service's page, which sends you back to the app, on `127.0.0.1` on the machine you're at. The service reads who Roundhouse is from [a document on Roundhouse's site](https://getroundhouse.dev/oauth/client.json), which has nothing of yours in it. A service that doesn't take that is sent what's in it, to register Roundhouse, with the address on `127.0.0.1` you're sent back to, and may keep that once you've removed the server.
- **The app's manifest, as you make an app of your own,** which your browser posts to GitHub, from a page the app serves on your machine, on `127.0.0.1` alone, until GitHub sends you back with a code.
- **The hub's calls to GitHub,** once you add a GitHub account: to ask whose a token is and when it ends, to sign in, to renew a sign-in's token, and once a day to ask again who each account you added is, by the name and email its GitHub profile shows, which agents' commits in repositories on GitHub are by. For an app of your own, it makes the app from GitHub's code, looks for where it's installed, and makes its installation's tokens. While `gh`'s login on the hub's machine is an account you added, the hub runs `gh` there once a minute to read its token, and asks GitHub whose it is when it changes, as it does each time Connect account opens. And each time the hub starts, `gh` asks GitHub, once on each machine, whether it takes that machine's own login, which Git's Found says.

## Going back a version

An older version can't open a database a newer one has migrated. That's what `backups/` is for: the hub saves a copy before each migration, named for the schema version it holds. The repository's [README](../../../README.md#installing-a-build-of-your-own) says how to go back to one.
