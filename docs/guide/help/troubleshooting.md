# Troubleshooting

What to check when something isn't working.

## An agent doesn't answer

Roundhouse runs your agent’s CLI as you. For Claude, run:

```sh
claude
```

For Codex, run `codex login` on the machine running the workspace; for Antigravity, run `agy` there and sign in. Roundhouse uses those native sign-ins; its Agents page does not supply a Codex or Antigravity credential. If any CLI asks you to sign in, do, then send the prompt again. A chat whose agent stopped short picks up its conversation at its next prompt.

## An agent reached its usage limit

The chat ends on a card saying `Usage limit reached`, and when the limit resets. Choose another model in the composer, then click `Retry`, or wait until it resets. The words under it, which are the CLI's, say where to manage your usage. If the agent had work going in the background, it goes on picking up by itself as each task ends, and each try fails the same way. The card says how many times, and what started each, rather than the chat showing a card for each.

## The pull request's bar only offers Create PR

Pull requests are read with GitHub's CLI. Check that:

- `gh` is installed, and either a GitHub account is the default in [Settings](../reference/settings.md#git), or `gh auth status` says you're logged in on the machine running the workspace.
- The repository has a GitHub remote.
- The pull request is the branch's own: one the workspace pushed to, or whose branch it checked out. One opened from a branch of the same name somewhere else isn't.

Everything else works without `gh`.

## An agent can't push

Open the Git page in [Settings](../reference/settings.md#git):

- **If it says `Agents can't push.`,** the default GitHub account can't be used, and it says why and what to do: renew its token, sign in again, or make another account the default. Agents don't fall back on the machine's own login.
- **If you've added no account,** agents push with the machine's own login. **Found** says who `gh` is logged in as there, and whether GitHub took its token, as of when the hub last started. Run `gh auth login` on that machine, or add an account.

See [Who agents are on GitHub](../concepts/who-agents-are-on-github.md).

## A workspace's setup failed

A failed setup warns, and doesn't block: the workspace is made and the agent still gets your prompt. The chat shows the script's log.

Fix the script in the repository's `.conductor/settings.toml`. Setup runs again for each new workspace, and when a workspace is restored. [Set up your repository](../get-started/set-up-your-repository.md) says how it works.

## Archiving is refused

Archiving refuses when the worktree has uncommitted changes, commits on no branch, or submodules, so work isn't lost by accident. Commit or discard them, or say to go on when it asks.

On Windows, it also refuses when it couldn't move the worktree aside, which happens while a program has a file in it open, or is working in one of its folders, such as a build you started there that's still running, or a terminal. It names any program running from the worktree, which is the likeliest to be what holds it. Close that program, and archive again.

If git has already forgotten the worktree, as when deleting it was stopped part way, archiving asks before it deletes what's left, since git can't say whether that holds work.

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

Open an [issue on GitHub](https://github.com/getroundhouse/roundhouse/issues), with the build shown at the bottom of the sidebar, or in About Roundhouse, and what you did.
