# A hub owns the state and workers do the work, with the same code for local and remote

**Status:** Decided.
**First recorded:** 2026-11-06, in #78.

A hub stores workspaces, sessions and their event logs, and serves the API that every client uses. A worker runs one workspace's agents, terminals and git, and reports events to its hub. A hub on your own machine and a hub on a server are the same code and the same API. A worker is a separate process even on the hub's machine, so a remote worker changes only the transport. A hub on a server isn't built yet.

## Chosen over

- **The same full daemon everywhere.** Each sandbox would own its log, and a control plane would hold a copy. That's simpler to start. But every cloud log would exist twice, and a sandbox that's asleep would need a second way to read its log.
- **Separate local and cloud paths.** Each feature would be built twice.
- **Everything through a central sync engine.** It would break offline use, and local data would no longer stay on the device.
