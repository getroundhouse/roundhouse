# Only the hub speaks the previous protocol version

**Status:** Decided.
**First recorded:** 2026-11-11, in #518.

The hub speaks the current and the previous version of each protocol. Clients, workers and provider adapters speak only their own. The hub is the only one that meets another version in both directions: a client talks only to hubs, a worker only to the hub that started it, and an adapter only to the hub that runs it. So the code for an old version is in one place, and only until the next break.

## Chosen over

- **Old versions in clients and workers too.** Old code would be in more places.
- **More than one old version in the hub.** That's more old code, for cases that are rare or that updating fixes.

## What it costs

- A worker or a client two breaks behind its hub is cut off, and the hub stops such a worker with its agents. The app's update crosses one break at a time, so it never cuts one off. Installing a build by hand can.
- Rolling a hub back past a break stops the agents of every worker on the newer version.
- Each break in the worker protocol needs a published release before the next break, and the update feed keeps that release permanently.
- The database's older forms go only at a break too, when the oldest release a build updates from moves. A build installed by hand across that release finds its database refused, as "The database" in `docs/principles/state.md` says.
- Clients that can't all update with their hub, such as an iPhone app waiting on App Store review, would need a hub that keeps speaking each version for a while after the next one replaces it.
