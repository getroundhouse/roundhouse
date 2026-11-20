# While a machine's own `gh` login is the default, the hub reads GitHub with `gh`'s token

**Status:** Replaced by [github-identity-held-by-hub.md](github-identity-held-by-hub.md).
**First recorded:** 2026-11-17, in #1616.

Someone who uses Roundhouse locally with only `gh` logged in has added no GitHub account. Each machine's own login is then the default, and the hub has no token of its own to read GitHub with. So the worker that finds a machine's `gh` login hands the hub the token `gh` uses, and the hub reads GitHub with it. The person sees their pull requests with no account added.

The hub keeps the token only in memory, and reads with it only for the workspaces on that provider's machines. Agents still use `gh` itself.

## Chosen over

- **Reading through the worker.** That would keep a second way of calling GitHub, and would read only while one of that machine's workers is running.
- **Reading nothing until an account is added.** Roundhouse used locally with only `gh` would show no pull requests.

## What it costs

A hub that restarts holds no token until a worker on one of the provider's machines connects and hands it over. So a read that needs the token starts a workspace's worker to get it.
