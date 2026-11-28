# A worker the hub can't reach is disconnected until it dials back or is shown to be gone, and silence never ends it

**Status:** Decided. Partly built: a hub waits for a worker it can't reach, whether an earlier hub left it or its connection dropped, asks its provider whether it's gone, and gives a prompt sent meanwhile to the agent once the worker is back.
**First recorded:** 2026-11-27, in #2942.

A worker's connection is how it talks to its hub, not proof that it's alive. When a hub restarts, a hub crashes or the network drops, the workspace is *disconnected* until its worker dials back, and then it carries on. The hub ends a worker only on proof that it's gone, or when the person says so, and never because it has gone quiet. A disconnected workspace counts as at work, so nothing replaces its worker. These still end a worker at once: a goodbye, meaning the worker closing its connection with a reason, the person stopping or archiving the workspace, and a connected worker that doesn't answer a call within its limit.

A provider can say that a worker is gone, with an optional request, `WorkerState`, whose answer is running, gone or unknown. Only gone, which a provider says when it's sure that no worker runs there, lets the hub end what's left and start afresh by itself. An adapter that can't tell answers `Unsupported`, which the hub takes as unknown.

A worker that has gone ten minutes without a hub stops its agents and terminals, then stays up. It holds what no hub has confirmed and keeps dialing, until an outer limit, such as a day, when it exits.

A disconnected workspace offers three actions: wait, which is the default, end its worker and start fresh, or archive, which says that unpushed work there can't be checked. An agent that looks into an instance comes later.

## Chosen over

- **When the hub ends a worker it can't reach.**
  - *After a limit.* The hub would wait longer than the worker's own ten minutes, then end it. A worker cut off for longer, such as a laptop asleep overnight, would lose its work, the two limits would have to stay in step, and ending the worker may not reach a machine the hub can't reach.
  - *As soon as its connection drops.* Any timeout that fires on a connection that was only quiet would end the agents, so remote workers would keep long timeouts, and a hub that was killed would end the workers on other machines, which only learn that their connection has gone after iroh's 30 s idle timeout.
- **How a provider says that a worker is gone.**
  - *The local provider alone.* The hub would check the local claim itself, with no protocol change. The hub would treat one provider specially, and microsandbox would get nothing.
  - *No provider says it.* Every disconnected workspace would wait for the person. A local worker that crashes would need a click before its workspace works again.
- **What a worker does once it has gone ten minutes without a hub.**
  - *Stop its agents and exit.* What it hadn't had confirmed would be lost, including how its agents ended.
  - *Never stop its agents.* It would reverse the rule in `docs/principles/failure.md` that nothing runs indefinitely with no hub to record it.
- **Which actions a disconnected workspace offers.**
  - *An agent to look into the instance from the start.* It's provider-specific, needs its own design of tools and login, and would hold up the rest.
