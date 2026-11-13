# The hub

The app doesn't run your agents. A hub does, in a process of its own, and the app is one way of looking at it.

```
 Desktop app      Command line
      └──────┬───────┘
             │
            Hub       Owns workspaces, chats and their history
             │
          Workers     One per workspace: git, the worktree, scripts and diffs
             │
          Agents      Your own Claude CLI
```

The hub starts by itself when the app, or the command line, needs it. It runs on your machine.

## What that means for you

- **Agents outlive the app.** Quit the app, and agents keep working. Open it again, and each chat is where its agent has got to.
- **The queue sends without you.** A prompt queued for when the agent finishes is the hub's, so it sends with the app closed.
- **Nothing typed is lost.** The hub keeps what's in each composer, files and all, and an answer you were typing to a question. It's there when the app opens again.
- **Every client shows the same thing.** What's read, what's queued and where each pull request stands are the hub's record, so the app and [the command line](../reference/command-line.md) agree.
- **An update doesn't stop anything.** The app restarts the hub on the new version, and running agents carry on. [Updates](../reference/updates.md) says how.

## Workers

Each workspace has a worker, a process of its own that does the work there: git, the worktree, setup scripts and diffs. The worker runs the agent, which is the Claude CLI you already have, with your login.

## Where it runs

Today the hub runs on your machine, and so do its workers. It's designed to run on a server or in the cloud too, with a phone as another client, and that isn't built. The [roadmap](../roadmap.md) says where each stands.
