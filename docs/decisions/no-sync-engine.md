# Clients stay in sync through the hub's own protocol, not a sync engine

**Status:** Decided.
**First recorded:** 2026-11-08, in #189.

A client keeps its copy of a hub's state correct through the hub's own protocol, which needs only a revision number and a few rules. No sync engine sits between them. If clients ever write while offline, that would be the reason to look at this again.

## Chosen over

- **A sync engine, such as PowerSync, Electric or Turso's sync.** These exist to merge writes from clients that work offline. A hub is the only writer of its state, so it has nothing to merge. They also need Postgres, or they copy a whole database to each client. And none of them can serve a window of a log, or the tail of live output that isn't stored.
