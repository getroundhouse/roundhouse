# Every worker dials its hub, with a ticket the hub gives it, and no provider sits between them

**Status:** Decided, and built.
**First recorded:** 2026-11-22, in #2487.

A worker opens its own connection to its hub, wherever it runs. When an instance starts, the hub gives its provider a ticket, which holds the hub's iroh key and a secret that admits one worker, once. The provider puts the worker and its ticket on the machine, and the worker dials the hub with the ticket, over QUIC through iroh. The hub never reaches a worker through its provider. So a provider only makes, starts, stops and destroys machines, and a worker's stream doesn't depend on its adapter. A worker the local provider runs on the hub's own machine dials the hub the same way, at a local address.

**Decided by the owner, 2026-11-27: a local worker looks its hub up again if the hub comes back on another port.** The hub's loopback endpoint keeps its port across restarts, but other hubs on the machine draw from the same ports, so one can take it meanwhile, and a local worker has no relay to fall back on. So each time the hub binds that endpoint, it writes where it is to a file in its data directory, apart from its key's. The local provider gives its worker the file's path, and a worker whose dial fails reads it and dials the hub there. The file says where the hub is, not who, so a worker dials there only if it names the key its ticket does.

iroh starts a connection through a relay, moves it to a direct path once one forms, and keeps it on the relay only when none does. A provider whose machines sit behind a port mapping has the worker advertise the mapped address, which iroh calls an external address, so the hub opens the direct path to it. microsandbox's VMs on a Mac need this, since UDP sent from a VM to its host never forms a direct path, while UDP forwarded into a VM does. *Not built:* which relays a hub uses. Until a hub can name its own, it uses iroh's default, n0's public relays.

What it costs: a worker needs a route to its hub or to a relay, where a stream the hub opens needs none. A worker that can't reach its hub directly connects through the relay first, so with n0's relays, even a microsandbox VM on the hub's own machine can't connect without the internet.

## Chosen over

- **The hub runs a command its provider returns, and uses the command's stdin and stdout as the worker's stream.** This is what's built. Each provider has to find its own way to every machine it makes, and its own program then carries every byte. An adapter for microsandbox would relay the stream through `msb exec`, and a machine behind NAT, such as one from a cloud API, would need a tunnel the provider builds.
- **The hub dials each worker, at an address its provider gives.** The hub would need a route to every machine, which one behind NAT doesn't have.
- **TCP with TLS, or WebSocket, to an address the hub listens on.** A worker behind NAT, or on another network, could then reach the hub only through a relay or tunnel of our own. iroh brings the relay, NAT traversal and moving a connection between paths, and it's MIT or Apache-2.0.
