# A client follows everything through one kind of subscription, at one of a few fixed levels

**Status:** Decided, and built.
**First recorded:** 2026-11-22, in #2407.

A client tells the hub what it follows: the hub, a workspace, a session, a terminal, or an issue or a pull request. For each, the hub sends where it stands and then every change. What a client follows is also the hub's signal to keep it current. `docs/principles/serving.md` has the rules.

## Chosen over

- **A request of its own for each kind of thing to follow,** as now: the feed of records, a session's log, a terminal, and what a client shows of GitHub. Each has its own rules for starting, being replaced and recovering. The feed being apart from the rest is how the protocol grew, and nothing was gained by it.
- **A subscription for what a client shows, beside the others.** It would say what the hub keeps current, while the others said what the hub sends. What a client follows says both.
- **Subscriptions a client narrows by a condition,** such as the workspaces of one repository. The hub would evaluate a query for each client on each change, and two clients following the same thing could be sent different things.

## What it costs

A client has one way to follow and no other to fall back on, so one built for it can't use a hub from before it. Moving clients to it is a break of the client protocol, as `docs/principles/protocol.md` says of one. A client that needs less than a level sends is still sent all of it, as every client that follows the hub is sent every record.
