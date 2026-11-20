# The hub sets the GitHub credential for every process a worker starts

**Status:** Decided.
**First recorded:** 2026-11-16, in #1517.

Agents push and open pull requests with whatever credential git and `gh` find. So the hub decides what they find for github.com: the credential of the hub's default GitHub account. The hub sends it to each worker with `GitCredentialSet`, when the worker's link opens and whenever the credential changes. The worker gives it to every process it starts.

## Chosen over

- **Each machine's own git and `gh` login.** Who agents push as would depend on where they run. A machine's own login is still used when the person makes it the default.
- **A field of each agent launch.** A worker needs the credential with no launch at all, for its clones and fetches, its reads of pull requests and its scripts. And a token that expires has to be renewed while an agent is running.

## What it costs

Some things go around the credential:

- The person's own `url.<ssh>.insteadOf` or `pushInsteadOf` for GitHub, and a host alias such as `git@github-work:`. These push over SSH with the machine's key.
- Tools built on libgit2, Git LFS, and tools that read `GITHUB_TOKEN`.
- A Codex configured to filter the environment its commands inherit.

And while a credential is set, `gh` reads none of the person's own config, such as their aliases.
