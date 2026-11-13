# What Roundhouse does

Everything the app does today, on one page, with where to read more. What's designed and not built is on the [roadmap](../roadmap.md).

## Hand work over

- **One task, one workspace.** Each workspace has a git worktree, a branch, ten ports and chats of its own, so agents never share files. [Workspaces, projects and chats](../concepts/workspaces.md) says how they fit together.
- **Say what to work on as you make the workspace.** The agent starts at once, and a prompt sent while the workspace is still setting up waits, then sends. See [Hand a task to an agent](../guides/hand-a-task-to-an-agent.md).
- **Setup makes each worktree runnable.** It copies the ignored files a new worktree needs, such as `.env`, and runs the repository's setup script. See [Set up your repository](set-up-your-repository.md).
- **Agents work with full access, or plan first.** An agent doesn't stop to ask before each step. In plan mode it works out a plan, and asks before it changes anything. See [Full access and plan mode](../concepts/full-access-and-plan-mode.md).
- **A prompt can carry files.** Paste a screenshot, or drop up to ten files on the composer.
- **Each chat names its model and how hard it thinks.** See [Models and plan mode](../reference/chats/models-and-plan-mode.md).
- **Titles are written for you.** A small model titles each workspace and chat from its first prompt, so it costs your agent no context.

## Stay in the loop without watching

- **Every workspace has a mark.** A light sweeps it while the agent works, it turns red when the agent stopped short, and its colour is the pull request's. See [The status mark](../reference/the-status-mark.md).
- **A title turns bright when it needs a look.** That's a reply you haven't read, or an agent waiting on you. Nothing else moves. See [Know when an agent needs you](../guides/know-when-an-agent-needs-you.md).
- **Notifications and a sound say so too,** when you're in another chat or another app.
- **Agents outlive the app.** A hub, in a process of its own, runs them. Quit the app and they keep working, and queued prompts still send. See [The hub](../concepts/the-hub.md).
- **Your Mac stays awake while there's work,** and can sleep once agents are only waiting on you.
- **Home lists every workspace,** to find one, and to see where they all stand.

## Steer the work

- **Steer.** `Enter` while the agent works sends a prompt it reads at its next chance.
- **Queue.** `Cmd-Enter` holds a prompt for when the agent finishes. The hub keeps the queue, so it sends with the app closed.
- **Stop means stop.** Stopping the agent holds the queue until you say go. See [Steer or queue while it works](../guides/steer-or-queue.md).
- **A question takes the composer's place.** You answer where you'd type, with a number key. See [Answer its questions](../guides/answer-its-questions.md).

## Review and merge

- **The changes sit beside the chat.** Click a file for its diff, in one column, side by side, or as the whole file.
- **Comment on a changed line, and send the comments to the agent.** See [Review its changes](../guides/review-its-changes.md).
- **A changed image is compared,** side by side, by flipping, or as what changed.
- **Every file in the workspace can be read,** ignored ones too.
- **A bar says the first thing in the way of merging,** and offers the one thing to do about it, such as Push, Fix checks, Resolve or Merge. See [Take a pull request to merge](../guides/take-a-pull-request-to-merge.md).
- **Every row in the sidebar shows where its pull request stands.**

## Keep long chats in hand

- **A ring shows how full the context is,** and bars beside it show what's left of your plan's limits. See [Watch context and limits](../guides/watch-context-and-limits.md).
- **Compact when you choose,** and say what to keep.
- **A chat says when its prompt cache is about to expire,** since the next prompt after that costs the most.
- **An outline lists a chat's prompts,** and `Cmd-F` finds text in all of it, however long. See [Finding, tabs and closing](../reference/chats/finding-tabs-and-closing.md).
- **A subagent's work has a tab of its own,** so it doesn't bury the chat it's part of. See [What a chat shows](../reference/chats/what-a-chat-shows.md).

## Around the edges

- **Updates never stop a running agent.** See [Updates](../reference/updates.md).
- **Nothing typed is lost.** What's in a composer, files and all, is kept until it's sent, and a prompt that can't send comes back to it.
- **A command line talks to the same hub.** See [The command line](../reference/command-line.md).
- **A repository set up for Conductor works unchanged.** See [Coming from Conductor](coming-from-conductor.md).
- **Your agent, your login.** It runs the Claude CLI you already have. There's no Roundhouse account, and nothing is sent to us. See [Where data is kept](../reference/where-data-is-kept.md).
