# FAQ

Short answers.

## Which agents does it run?

Claude, through the Claude CLI you already have. Other agents are designed for and not built. The [roadmap](../roadmap.md) says where each stands.

## Do I need an account?

No. There's no Roundhouse account. You need the Claude CLI logged in, and `gh` logged in for pull requests.

## What does it cost?

Roundhouse is open source, under Apache-2.0. Your agents run on your own Claude login, as they would in a terminal. Roundhouse adds only the small calls that write titles.

## What is sent to you?

Nothing. The app's only request of its own is its check for updates. [Where data is kept](../reference/where-data-is-kept.md) has the rest.

## Is a workspace a sandbox?

No. A workspace's worktree keeps its files apart from other workspaces', and that's all. An agent runs commands and changes files as you. [Full access and plan mode](../concepts/full-access-and-plan-mode.md) says how to have one ask first.

## What happens to my agents when I quit the app?

They keep working. A hub, in a process of its own, runs them, and queued prompts still send. See [The hub](../concepts/the-hub.md).

## Does my computer have to stay awake?

Yes, since agents run on it. Roundhouse keeps a Mac awake while there's work, and lets it sleep once agents are only waiting on you.

## Can I use it without GitHub?

Yes. Workspaces, chats and the diff work with any git repository. The pull request's bar needs a GitHub remote and `gh`.

## Does my repository need any setup?

No. A setup script helps once a worktree needs installing or building before an agent can run anything. See [Set up your repository](../get-started/set-up-your-repository.md).

## Can several agents work on the same branch?

Several chats can share one workspace, and so one worktree and branch. Agents in different workspaces are each on a branch of their own.

## I use Conductor. What carries over?

Your repository's settings, scripts and variables work unchanged. See [Coming from Conductor](../get-started/coming-from-conductor.md).

## Where does the name come from?

A roundhouse is the round building where a railway services its locomotives, each in its own stall around one turntable. The mark beside every workspace is one seen from above. Roundhouse is a working name.

## Can I change it, or build my own?

Yes. Every line is [on GitHub](https://github.com/getroundhouse/roundhouse), and its [README](../../../README.md#working-on-roundhouse) says how to build it.
