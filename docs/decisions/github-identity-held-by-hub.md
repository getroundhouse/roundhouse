# Agents' GitHub identity always comes from an account the hub holds

**Status:** Replaced by [github-on-provisioned-machines.md](github-on-provisioned-machines.md), before it was built.
**First recorded:** 2026-11-20, in #2061.

Who agents are on GitHub is always an account the hub holds, on every machine. There's no mode in which each machine's own login is used instead. An account is one connected through Roundhouse's sign-in, a GitHub App's installation, or the `gh` login on the hub's own machine.

The hub doesn't store the `gh` login's token. It reads it with `gh auth token` when it needs it, keeps it in memory for a few minutes, and checks that it still belongs to the account's user. So a person who logs `gh` out, or switches it to another user, stops Roundhouse using it. Only the hub's machine can provide one, since the hub reads GitHub with no worker running and hands the token to remote workers.

Every process a worker starts gets the worker's environment from the start, so a new default reaches everything already running. The held account is what `gh`, the hub's own reads of GitHub and pushes over HTTPS use, and commits on github.com are by its identity. A push over SSH goes as the machine sends it, with its own key, wherever that works.

This replaces [machine-gh-token-handed-to-hub.md](machine-gh-token-handed-to-hub.md), and the case in [git-credential-set-by-hub.md](git-credential-set-by-hub.md) where each machine's own login is the default. The rest of that record stands.

## Chosen over

- **Each machine's own login as a mode of its own, as before.** It works only where a person has set a machine up, and each machine is set up differently: `gh` keeping its token in the keychain or in `hosts.yml`, SSH rewrites in git's config, `GH_TOKEN`. A hub on a server, and a VM made for one workspace, have none. Switching between it and an account changed what a running agent's `gh` saw, differently on each machine.
- **Storing a copy of the `gh` login's token.** It would keep a second copy of a secret the person already keeps, and it would go on working after they logged `gh` out.
- **Sending every push through the held account, overriding the machine's SSH setup.** On GitHub, who pushes matters little when the SSH key and the account are the same person's. And `gh`'s own login usually lacks the `workflow` scope, so it can't push a change to `.github/workflows/` over HTTPS, though the same person's SSH key can.

## What it costs

- **Someone whose only GitHub setup is an SSH key has to sign in once** before agents can use `gh`, and before Roundhouse can read their pull requests. Agents' pushes over SSH work without it.
- **Where pushes go over SSH, the hub can't tell whether GitHub would take one,** since it checks with the held account's token.
- **The hub runs `gh`,** which only workers did. That's for its own machine, and for nothing else.
