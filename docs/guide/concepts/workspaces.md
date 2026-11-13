# Workspaces, projects and chats

A project holds workspaces, and a workspace holds chats. The workspace is how you hand work over, and its branch and pull request are how the work comes back.

## A project is a repository

A project is a git repository on your disk that you've added to Roundhouse. The sidebar lists your projects, each with its workspaces under it.

## A workspace is one task

A workspace is everything one piece of work needs, kept apart from every other:

- **A git worktree,** a checkout of its own, in `~/roundhouse/workspaces`.
- **A branch,** made as the workspace is.
- **Ten ports,** so its servers don't collide with another workspace's.
- **Chats,** each with an agent at work in the worktree.

Because each workspace has its own files, agents in different workspaces never get in each other's way. One can run the tests while another rewrites the module they test.

A worktree is a directory, not a sandbox. An agent can read and change anything you can.

## A name and a title

A workspace has both, and they're different things:

- **Its name,** such as `euston`, is its directory and the folder its branches go in, so it starts on the branch `euston/start`. It never changes, since renaming a worktree under a running agent would break it.
- **Its title** is what the sidebar shows, such as "Fix login redirect". It's written from your first prompt, and you can change it.

## A chat is a conversation with an agent

A chat is one conversation with one agent. A workspace starts with one, and `Cmd-T` adds another, which shares the worktree. Use a second chat for a side question, or to have another agent review the first one's work.

Closing a chat stops its agent and keeps its history, so it can be reopened.

## When to make a new workspace

Make a new workspace when the work should end as its own pull request. Make a new chat when it's part of the same change.

## From a prompt to a merge

A workspace's life follows the work:

1. You make it, and say what to work on.
2. Its agent works, and you review what changes.
3. Its branch becomes a pull request, which merges.
4. You archive it, which removes the worktree and keeps the branch and the chats' history.

[Hand a task to an agent](../guides/hand-a-task-to-an-agent.md) starts at the first step, and [Workspaces](../reference/workspaces.md) has every rule.
