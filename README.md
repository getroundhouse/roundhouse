# Trestle

An open source app for running many coding agents in parallel, each in its own workspace. The workspace is the unit of delegation, and the branch and pull request are the unit of integration. It runs on your Mac first, and the same design extends to your own servers, the cloud and a phone.

It's early, and not ready for other people yet. Milestone 1, a local desktop app, is in progress.

## Why this exists

Running several agents at once works best when each one has its own workspace, and when the small day-to-day details are right. Conductor showed how good that can feel, and it's a big inspiration here. It's shutting down, and the alternatives tried don't fill the gap:

- None of the dozen options tried has quite the same mental model.
- None has all the small day-to-day features.
- Cloud and mobile are required, and existing options fall short there.

## Try it

It uses your own Claude login and settings, keeps its data in `~/Library/Application Support/trestle`, and puts worktrees in `~/trestle/workspaces`.

```sh
cargo build --workspace
target/debug/trestle-desktop
```

Use `Add repository` at the bottom left, then `New` beside the repository. The model and approval mode are below the composer. To comment, hover a line in the diff and click its add button. The `trestle` command-line client talks to the same hub.

| Key | Action |
|---|---|
| `Cmd-N` | New workspace |
| `Cmd-[`, `Cmd-]` | Move between workspaces |
| `Cmd-T` | New chat |
| `Cmd-W` | Close a chat |
| `Ctrl-Tab` | Move between chats |
| `Cmd-Shift-D` | Open the diff |
| `Cmd-Shift-P` | Ask the agent for a PR |
| `Cmd-.` | Stop the agent's turn |

## Where things are

- **Docs:** `AGENTS.md` lists them, each with its one purpose.
- **What's left:** [issues](https://github.com/zlangbert/trestle/issues) for questions, risks, gaps and tasks, and [milestones](https://github.com/zlangbert/trestle/milestones) for the build order.
