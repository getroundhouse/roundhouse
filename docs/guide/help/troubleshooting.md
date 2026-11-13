# Troubleshooting

What to check when something isn't working.

## An agent doesn't answer

Roundhouse runs the Claude CLI as you. In a terminal, run:

```sh
claude
```

If it asks you to log in, do, then send the prompt again. A chat whose agent stopped short picks up its conversation at its next prompt.

## The pull request's bar only offers Create PR

Pull requests are read with GitHub's CLI. Check that:

- `gh` is installed, and `gh auth status` says you're logged in.
- The repository has a GitHub remote.

Everything else works without `gh`.

## A workspace's setup failed

A failed setup warns, and doesn't block: the workspace is made and the agent still gets your prompt. The chat shows the script's log.

Fix the script in the repository's `.conductor/settings.toml`. Setup runs again for each new workspace, and when a workspace is restored. [Set up your repository](../get-started/set-up-your-repository.md) says how it works.

## Archiving is refused

Archiving refuses when the worktree has uncommitted changes, or commits on no branch, so work isn't lost by accident. Commit or discard them, or say to go on when it asks.

## The app says it can't reach a hub

The hub is the process that runs your agents, and the app starts it when it needs one. If it can't:

1. Quit the app, and open it again.
2. Look at `hub.log`, in the hub's data directory. [Where data is kept](../reference/where-data-is-kept.md) says where that is.

## An update won't install

The bottom of the sidebar says why, with the detail when you point at it. [Updates](../reference/updates.md#problems) lists what it can say.

On a Mac, an app run from where it was downloaded can't replace itself. Move it to Applications.

## No notifications

- On a Mac, check that Roundhouse is allowed to show notifications, in System Settings.
- Check `Notify when an agent needs you`, in [Settings](../reference/settings.md).
- The chat you're looking at never notifies, and nothing is told of that happened while the app was closed.

## Windows warns before running the installer

The Windows preview isn't signed, so Windows asks first.

## A prompt costs more than expected

A prompt sent after a long chat's prompt cache has expired writes the whole conversation again. [Watch context and limits](../guides/watch-context-and-limits.md) says how to see it coming.

## Report a bug

Open an [issue on GitHub](https://github.com/getroundhouse/roundhouse/issues), with the build shown at the bottom of the sidebar, and what you did.
