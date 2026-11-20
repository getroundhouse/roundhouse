# The hub holds each agent account's login and hands it to every launch

**Status:** Decided.
**First recorded:** 2026-11-13, in #758.

An agent's CLI needs a login wherever it runs. So the hub keeps a login for each account it's given one for, and hands every launch of a chat's agent the login of the account the chat is on. An instance that comes and goes then needs no login of its own.

A login is a token that the agent's own CLI makes to last, such as the one `claude setup-token` prints. The person makes it themselves, with the CLI's own sign-in, and gives it to the hub. It lasts a year without refreshing, so workers that share one never have to refresh it.

## Chosen over

- **The login on each worker's machine.** An instance that comes and goes has none to keep. A chat still runs under its machine's own login while its kind of agent has no default account.
- **A login held by a provider adapter, or kept in a base image.** The hub hands the login to the worker with each launch, so neither holds one.
- **The hub signing the person in.** The person signs in with the CLI itself, and the hub never signs anyone in.

## What it costs

A token doesn't say whose it is. So the person names the account as they connect it, and the hub's record of the login says whose it is, where a worker says it for a login on its own machine.
