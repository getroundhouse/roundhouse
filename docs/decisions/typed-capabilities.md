# What an agent can do is one typed set of capabilities, which each adapter fits its agent to

**Status:** Decided.
**First recorded:** 2026-11-14, in #1242.

`protocol::capability` defines each thing an agent can do, such as rewinding a conversation, once for every kind of agent. The set is the experience Roundhouse decides is best, and each adapter fits its agent to it where it can. Each run reports which capabilities it claims, because that depends on the CLI's version and flags, the account and the model provider. The hub records a run's claims and refuses a request the run doesn't claim, and a client offers only what's claimed.

## Chosen over

- **Holding every agent to what all agents share.** With the typed set, something only one agent can do is still a capability, which only that agent's kind claims.

## What it costs

- A capability that no adapter claims is only a contract. The hub and the clients do nothing for it, so the first change that has an adapter claim it also builds what the hub and the clients need.
- The client protocol's version stayed the same through the change. So a client from before it can't read a session's record from a newer hub, and shows no chats until it updates.
