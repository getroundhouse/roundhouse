# Using Roundhouse

Roundhouse runs many coding agents at once, each in a workspace of its own, and takes each one's work from the first prompt to a merged pull request. This guide says how to use it.

New here? Start with the [quickstart](get-started/quickstart.md).

![A workspace in Roundhouse: its chat in the middle, the other workspaces on the left, and its files and pull request on the right](../spec/screens/conversation.png)

## Get started

- [Quickstart](get-started/quickstart.md): from a download to a merged pull request
- [Install](get-started/install.md): what you need, and the previews for Windows and Linux
- [What Roundhouse does](get-started/what-it-does.md): everything the app does, on one page
- [Set up your repository](get-started/set-up-your-repository.md): a setup script, and the files a new worktree needs
- [Coming from Conductor](get-started/coming-from-conductor.md): what works unchanged

## Concepts

- [Workspaces, projects and chats](concepts/workspaces.md): what each one is, and what it owns
- [The hub](concepts/the-hub.md): why agents keep working with the app closed
- [Full access and plan mode](concepts/full-access-and-plan-mode.md): what an agent does without asking
- [The window](concepts/the-window.md): what each part of the app is called

## Guides

- [Hand a task to an agent](guides/hand-a-task-to-an-agent.md): make a workspace, and say what to work on
- [Know when an agent needs you](guides/know-when-an-agent-needs-you.md): the mark, a bright title, and notifications
- [Steer or queue while it works](guides/steer-or-queue.md): send a prompt now, or for when the agent finishes
- [Answer its questions](guides/answer-its-questions.md): questions, and approvals in plan mode
- [Review its changes](guides/review-its-changes.md): read the diff, and send comments to the agent
- [Take a pull request to merge](guides/take-a-pull-request-to-merge.md): the one thing in the way, and what to do about it
- [Watch context and limits](guides/watch-context-and-limits.md): how full a chat is, and what's left of your plan
- [Archive and restore](guides/archive-and-restore.md): put a finished workspace away, and bring one back

## Reference

- [Chats](reference/chats.md): everything a chat does
  - [Prompts and the queue](reference/chats/prompts-and-the-queue.md)
  - [Questions and approvals](reference/chats/questions-and-approvals.md)
  - [Models and plan mode](reference/chats/models-and-plan-mode.md)
  - [Context, compacting and the cache](reference/chats/context-compacting-and-the-cache.md)
  - [What a chat shows](reference/chats/what-a-chat-shows.md)
  - [Finding, tabs and closing](reference/chats/finding-tabs-and-closing.md)
- [Workspaces](reference/workspaces.md): making one, its setup, its title, and archiving
- [The diff](reference/the-diff.md): changes, files, and comments
- [Pull request states](reference/pull-request-states.md): what the bar says, and what it offers
- [The status mark](reference/the-status-mark.md): what each part of it means
- [Keys](reference/keys.md): the app's keyboard shortcuts
- [Settings](reference/settings.md): what each setting does
- [Repository settings](reference/repository-settings.md): scripts, files to copy, and variables
- [The command line](reference/command-line.md): the `roundhouse` command
- [Updates](reference/updates.md): how the app updates without stopping agents
- [Where data is kept](reference/where-data-is-kept.md): on your disk, and nowhere else

## Help

- [Troubleshooting](help/troubleshooting.md): when something isn't working
- [FAQ](help/faq.md): short answers

## Roadmap

- [Roadmap](roadmap.md): what's built, and what's designed and not built
