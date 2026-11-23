# The hub's copies of what others own are a kind of their own, sent as records are

**Status:** Decided, not built.
**First recorded:** 2026-11-22, in #2407.

The hub's copy of state that someone else owns, such as git, GitHub or an agent, is an observation: a fourth kind of thing a hub serves, beside events, records and live snapshots. An observation reaches clients as a record does, with a revision, and says whether it's current or as of when it was read. The hub keeps it in a cache file that it empties rather than migrates. `docs/principles/observations.md` has the rules.

Each copy had settled its own rules before this. One was a field of a record in the database, another was in GitHub's cache and reached a client on a subscription of its own, and a workspace's diff wasn't kept, so each client that asked had it read whole from the worker. Few said when they were read.

## Chosen over

- **Fields on the records the hub owns.** This needs the least built. But each change to a copy would send every client the whole record it's on, and what the hub only observes would stay in the database, which the hub migrates, and copies before each migration.
- **Sent with nothing to tell a newer copy by, to each client that names the workspaces it shows,** as GitHub's cache reaches a client now. A client would keep these copies by a rule of their own, apart from the one it keeps records by, and a sidebar would have to name every workspace it lists before it could draw anything of them.
- **Keeping nothing, and reading from the owner on each request,** as a workspace's diff is read now. Each client's read starts a worker or a query, and there's nothing to draw of a workspace no client has open, such as its lines added and removed in a sidebar.

## What it costs

A new kind in the protocol, in `client-core` and in the hub. An observation and a record are in different files, so the two can't change in one transaction. What the hub can't ask its owner for again is lost when the cache is emptied, until the owner next says it.
