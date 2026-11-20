# Roundhouse sets up GitHub only on machines it provides, and the hub has an identity of its own

**Status:** Decided, not built.
**First recorded:** 2026-11-20, in #2086.

The hub has its own GitHub identity: an account connected through Roundhouse's sign-in, a token, an app of your own, or the `gh` login on the hub's machine, whose token the hub reads from `gh` when it needs it and never stores. The hub reads GitHub as it, such as for pull requests.

Each provider says whether Roundhouse sets up GitHub on its machines.

- **Where it does,** as on a VM made for one workspace, the worker hands every process the hub's identity, so agents push, open pull requests and commit as it. GitHub's SSH addresses are rewritten to HTTPS, so they push as it too. The line above the composer says when GitHub won't take its pushes.
- **Where it doesn't,** the worker hands nothing, so agents use the machine's own `git` and `gh`, as a terminal there does, and commit as its git config says. The local provider is one, always. Another provider can say so too, such as one whose base image has its own credentials.

Settings › Providers shows, for this Mac, who `gh` is logged in as and whose SSH key it is, beside who the hub is. Where these differ, the person sees it and decides.

This replaces [github-identity-held-by-hub.md](github-identity-held-by-hub.md), and narrows [git-credential-set-by-hub.md](git-credential-set-by-hub.md) to the providers where Roundhouse sets up GitHub.

## Chosen over

- **One identity on every machine, the hub's,** as the record this replaces chose. On a person's own machine, their SSH key and `gh` already decide who pushes, and overriding them gave agents a different identity from the person's terminal. It also needed a record of how each repository pushes, and a different answer for each provider.
- **Each machine's own login as a hub-wide default,** as before that. Switching between it and an account changed what running agents saw, differently on each machine, and a hub on a server had nothing to use.
- **A setting on this Mac for how pushes go,** through `gh`'s login or as git is set up. Someone whose own git can push has no reason to choose the first. Someone whose git can't push is better told what's broken.

## What it costs

- **On this Mac, the hub can't say whether agents' pushes will be taken,** since it isn't who pushes there. A push that fails says why itself.
- **The hub's identity and the person's own can disagree,** such as an app's bot for the hub and the person's own `gh`. Settings shows both, and leaves it to the person.
- **A provider that doesn't set up GitHub on a machine with nothing on it** gives agents no way to push or use `gh`.
