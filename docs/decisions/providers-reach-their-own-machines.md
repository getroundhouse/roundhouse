# The hub runs every provider's adapter itself, and each adapter reaches its machines its own way

**Status:** Decided.
**First recorded:** 2026-11-22, in #2487.

The hub talks to a provider only through the provider protocol, to an adapter it runs as its own child process, as [provider-adapters-over-stdio.md](provider-adapters-over-stdio.md) says. No adapter connects to the hub from another machine, and the hub never reaches a provider's machines itself. Where the machines are, and how the adapter reaches them, is the adapter's business. For example, a Vercel Sandbox adapter calls Vercel's API. A microsandbox adapter runs VMs on the hub's machine through microsandbox's library. To run VMs on another Mac, an adapter could call an API served on that Mac, or run `msb` there over SSH, and the hub works the same either way. Several entries in `providers.json` can run one adapter with different settings, such as one entry for each Mac.

Workers on those machines don't go through the adapter, since every worker dials its hub, as [workers-dial-the-hub.md](workers-dial-the-hub.md) says. So an adapter on the hub's machine never has to carry a stream to the machines it makes.

## Chosen over

- **An adapter on another machine that dials the hub, as a worker does.** It would add a second way into the hub that has to be authenticated, versioned and reconnected. An adapter on the hub's machine can do the same through an API or SSH.
- **A provider protocol that describes machines, which the hub then reaches itself.** The hub would need to know every kind of machine and how to reach it, which is what adapters keep out of the hub.
