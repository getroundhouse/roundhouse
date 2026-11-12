# Roundhouse

An open source app for running many coding agents in parallel, each in its own workspace. The workspace is the unit of delegation, and the branch and pull request are the unit of integration. It runs on your Mac first, and the same design extends to your own servers, the cloud and a phone.

Roundhouse is a working name, after the round building where a railway services its locomotives, each in its own stall around one turntable, and rail is the theme for naming. It's early, and not ready for other people.

## Why this exists

Running several agents at once works best when each one has its own workspace, and when the small day-to-day details are right. Conductor showed how good that can feel, and it's a big inspiration here. It's shutting down, and the alternatives tried don't fill the gap:

- None of the dozen options tried has quite the same mental model.
- None has all the small day-to-day features.
- Cloud and mobile are required, and existing options fall short there.

A web client isn't a target, since nothing depends on one, but nothing rules one out.

## Try it

It uses your own Claude login and settings, keeps its data in `~/Library/Application Support/roundhouse`, and puts worktrees in `~/roundhouse/workspaces`.

```sh
tools/install.sh
"$HOME/Library/Application Support/roundhouse-builds/current/roundhouse-desktop"
```

Each install keeps its own version, built from the current commit. `tools/install.sh --list` shows them, and `--use VERSION` goes back to one. An older version can't open a database that a newer one has migrated, so before migrating, the hub saves a copy in `backups/` in its data directory, named for the schema version it holds. To go back past a migration:

1. Quit the app, which otherwise starts a new hub as soon as this one stops, then run `roundhouse hub stop`.
2. Run `tools/install.sh --use VERSION` with the version from before the update, or the next hub migrates the copy again.
3. In the data directory, delete `hub.db-wal` and `hub.db-shm`, then copy the newest backup over `hub.db`, keeping the backup.

Anything recorded since the copy is lost.

To work on Roundhouse rather than use it, `cargo run -p desktop --bin roundhouse-desktop` runs your changes with their own hub and data, in `.context/dev`.

Use `Add project` at the top of the sidebar, then the `+` beside the project. Every agent has full access, and `Shift-Tab` in the composer, or Plan mode in the menu its `+` opens, turns plan mode on and off. The model is below the composer. To review the changes, click a file in the sidebar on the right, which `Cmd-Shift-D` hides and shows, and to comment, hover a changed line and click its add button. The top of that sidebar says where the workspace's pull request stands, and offers the next thing to do about it. The `roundhouse` command-line client, in the same directory, talks to the same hub.

| Key | Action |
|---|---|
| `Cmd-N` | New workspace |
| `Cmd-[`, `Cmd-]` | Move between workspaces |
| `Cmd-T` | New chat |
| `Cmd-W` | Close a chat, or the file tab |
| `Ctrl-Tab` | Move between chats |
| `Cmd-Shift-D` | Show or hide the diff |
| `Cmd-Shift-P` | Ask the agent for a PR |
| `Cmd-Enter` | Queue a prompt for when the agent finishes |
| `Cmd-Shift-A` | Archive the workspace |
| `Cmd-Shift-U` | Mark the workspace read, or unread again |
| `Cmd-.` | Stop the agent's turn |
| `Shift-Tab` | Turn plan mode on or off, in the composer and the new-workspace dialog |
| `Cmd-,` | Open Settings |

## Where things are

- **What it does:** the product spec, in `docs/spec/`.
- **Docs:** `docs/documentation.md` lists them, and says what goes in a doc and what goes on GitHub.
- **What's left:** [issues](https://github.com/getroundhouse/roundhouse/issues) for features, bugs, tasks, questions and risks, and [milestones](https://github.com/getroundhouse/roundhouse/milestones) for the build order.

## License

Apache-2.0, in `LICENSE`, so anyone can use, change and build on it. It's the license GPUI uses, and it has each contributor grant the patents their contribution uses.
