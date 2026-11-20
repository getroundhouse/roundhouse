# A session's history is a log that clients read by range, not a CRDT

**Status:** Decided.
**First recorded:** 2026-11-08, in #164.

A session's history is a log of events with one writer, the hub, so each event gets a sequence number and nothing is ever merged. Long sessions are on remote machines, so a client reads the log by range and never holds a whole transcript.

## Chosen over

- **A CRDT for session history.** CRDTs suit many people typing in one buffer, which isn't this product.
