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

Roundhouse reads pull requests from GitHub as the default account in [Settings](../reference/settings.md#git), or without one, as `gh` on the machine running the workspace. Check that:

- A GitHub account is the default, or `gh auth status` says you're logged in on the machine running the workspace.
- The repository is on github.com.
- The branch is pushed with its upstream set, to a branch of the same name, as `git push -u origin HEAD` does. One pushed without `-u`, or still tracking the branch it was made from, such as `origin/main`, counts as not pushed.
- The pull request is from the branch it's pushed to. One opened from a branch of the same name somewhere else isn't.

Everything else works without `gh`.

## The pull request's bar says it can't see the pull request

GitHub shows the default account no repository where the branch's pull requests are, so Roundhouse can't tell whether there's one. The workspace's name has a warning's sign after it. Rest the pointer on the name, or on the bar's words, for what GitHub said and what to do:

- **If you signed in with GitHub,** and the repository's organization limits which apps can reach it, it has to approve Roundhouse. **Approve on GitHub** opens Roundhouse's page on GitHub, where you grant the organization access, or ask its owners to.
- **If the default is a token,** give the token the repository, or use one that has it.
- **Otherwise,** ask the repository's owner to give the account access, or connect an account that can see it in [Settings](../reference/settings.md#git), which **Open Settings** opens.

The bar shows the pull request once Roundhouse reads it again, as it does each time the workspace is shown.

## An agent can't read or wait on a pull request

The agent's call says why. Roundhouse reads the pull requests agents ask for and wait on from GitHub itself, so check that:

- The repository's `origin` is on github.com. Roundhouse doesn't read pull requests on GitHub Enterprise Server or on a `ghe.com` host.
- Either a GitHub account is the default in [Settings](../reference/settings.md#git), or `gh auth status` says you're logged in on the machine running the workspace.

A wait that can't be read for want of a login goes on waiting, and Roundhouse reads it again once `gh` is logged in or an account is the default.

A call that says the pull requests kept changing as they were read, as while a branch is pushed or the queue merges, works once they settle: ask again.

## An agent can't push

Open the Git page in [Settings](../reference/settings.md#git):

- **If the workspace says `Agents can't push to` its repository,** above the composer, GitHub refused the default, has no such repository it can see, or doesn't take its token, and the line says GitHub's words and what to do: make another account the default, add one that can push there, or ask the repository's owner for access, or to approve Roundhouse. **Open Settings** at its end opens the Git page. For an app of your own, it names the app's bot, and says to install the app on the repository's owner with access to the repository, which **Install on GitHub** opens GitHub's page for, or, for a repository of an owner the app isn't installed on, to make another the default.
- **If Git says `Roundhouse can't use its GitHub account.`,** the default GitHub account can't be used, so Roundhouse can't read pull requests and agents on the providers it sets up can't push, and it says why and what to do: renew its token, sign in again, or make another account the default. Agents don't fall back on the machine's own login.
- **If the workspace is on your own machine,** agents push with your own `git` and `gh`, SSH key and all, whoever the default is, so it says nothing of the default. Check that you can push there from your own terminal.
- **If you've added no account,** agents on the machines Roundhouse sets up GitHub on can't push, and Roundhouse can't read pull requests. Connect one in Git. On your own machine, **Providers** in Settings says who `gh` is to agents, the owner of a `GH_TOKEN` your profile exports or else who `gh` is logged in as, and whose SSH key GitHub takes.

See [Who agents are on GitHub](../concepts/who-agents-are-on-github.md).

## A workspace's setup failed

A failed setup warns, and doesn't block: the workspace is made and the agent still gets your prompt. The chat shows the script's log.

Fix the script in the repository's `.conductor/settings.toml`. Setup runs again for each new workspace, and when a workspace is restored. [Set up your repository](../get-started/set-up-your-repository.md) says how it works.

## Archiving is refused

Archiving refuses when the worktree has uncommitted changes, commits on no branch, or submodules, so work isn't lost by accident. Commit or discard them, or say to go on when it asks. It refuses too when git can't read what's in the worktree, as when its folder was deleted or moved by hand, and says what couldn't be read. Say to go on if there's nothing in it to keep.

On Windows, it also refuses when it couldn't move the worktree aside, which happens while a program has a file in it open, or is working in one of its folders, such as a build you started there that's still running, or a terminal. It names any program running from the worktree, which is the likeliest to be what holds it. Close that program, and archive again.

If git has already forgotten the worktree, as when deleting it was stopped part way, archiving asks before it deletes what's left, since git can't say whether that holds work.

## The app says it can't reach a hub

The hub is the process that runs your agents, and the app starts it when it needs one. If it can't:

1. Quit the app, and open it again.
2. Look at `hub.log` and `app.log`, in the data directory. [Where data is kept](../reference/where-data-is-kept.md) says where that is.

## An update won't install

The bottom of the sidebar says why, with the detail when you point at it. [Updates](../reference/updates.md#problems) lists what it can say.

On a Mac, an app run from where it was downloaded can't replace itself. Move it to Applications.

## No notifications

- On a Mac, check that Notifications is allowed under Permissions, in [Settings](../reference/settings.md#permissions).
- Check `Notify when an agent needs you`, in [Settings](../reference/settings.md).
- The chat you're looking at never notifies, and nothing is told of that happened while the app was closed.

## Windows warns before running the installer

The Windows preview isn't signed, so Windows asks first.

## A prompt costs more than expected

A prompt sent after a long chat's prompt cache has expired writes the whole conversation again. [Watch context and limits](../guides/watch-context-and-limits.md) says how to see it coming.

## Report a bug

Open an [issue on GitHub](https://github.com/getroundhouse/roundhouse/issues), with the build shown at the bottom of the sidebar, or in About Roundhouse, and what you did.
