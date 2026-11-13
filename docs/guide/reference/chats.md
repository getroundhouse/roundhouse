# Chats

A chat is one conversation with one agent, in a workspace's worktree. This page says what a chat is made of, and the pages under it have every rule.

![A finished turn in a chat](../../spec/screens/conversation.png)

## What a chat has

- **An agent,** the Claude CLI, started when the chat's first prompt is sent. Closing the chat stops it.
- **A model and an effort,** which the chat names itself, so your own Claude settings don't decide them.
- **An approval mode:** full access, or plan mode.
- **A history,** which the hub keeps, so it's there after the agent has gone, in every client.
- **A queue** of prompts for when the agent finishes.
- **A title,** written from its first prompt. Until then it's Untitled.

A workspace can have several chats, each with its own agent, all in the same worktree.

## A turn

A turn runs from a prompt until the agent finishes responding. While it lasts, the chat ends with the working line: the mark with its light sweeping, and how long the turn has taken so far.

A finished turn ends with how long it took and when it ended, such as `1m, 48s · 2:15 PM`, and a button that copies the agent's final message. A stopped turn says `Stopped`, and one that failed says why, in red.

A turn can end with work still going in the background, such as a test run the agent said it would come back to. When that finishes, the agent picks up again by itself, in a new turn.

## The composer

Where you type. Its bottom row has:

| Part | What it does |
|---|---|
| `+` | Attaches files, and turns plan mode on and off |
| The model picker | Chooses the model and its effort |
| The limits' bars | What's left of your account's limits |
| The context's ring | How full the agent's context is |
| Send | Sends what's typed. While the agent is on a turn, it offers to stop it |

What's typed and attached is kept by the hub until it's sent. It's there when the app opens again, and a prompt that can't be sent comes back to it. Closing the chat lets go of it.

## The parts of a chat

- [Prompts and the queue](chats/prompts-and-the-queue.md): sending, attaching files, steering, queueing and stopping.
- [Questions and approvals](chats/questions-and-approvals.md): what an agent asks, and how to answer.
- [Models and plan mode](chats/models-and-plan-mode.md): which model, how hard it thinks, and whether it asks first.
- [Context, compacting and the cache](chats/context-compacting-and-the-cache.md): what fills up, and what it costs.
- [What a chat shows](chats/what-a-chat-shows.md): tool calls, thinking, subagents and work in the background.
- [Finding, tabs and closing](chats/finding-tabs-and-closing.md): the outline, Find, titles, tabs, and reopening a closed chat.
