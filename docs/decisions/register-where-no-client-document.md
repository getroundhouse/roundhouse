# The hub registers Roundhouse with a service that doesn't take its client document

**Status:** Decided, and built.
**First recorded:** 2026-11-25, in #2757.

A service that an MCP server is signed in to has to know who Roundhouse is. Roundhouse's client document says so where the service takes it, as MCP has it be. Where the service doesn't, and has apps register by RFC 7591, the hub registers Roundhouse there as the person signs in. It registers only as a client with no secret, since Roundhouse's code is anyone's to read.

The hub keeps what it was registered as with the sign-in, and uses it each time the person signs in to that server again. It registers again only where the service has forgotten it, for another service, or for a redirect it wasn't registered with.

Of twenty public MCP servers whose services' metadata was read on 2026-11-25, ten take the document. Five take only apps that register, and two of those only with a secret.

## Chosen over

- **Only the client document.** MCP deprecates registering, but three of the twenty could then not be signed in to, though they let anyone register.
- **Registering at each sign-in.** A service may have no way to delete a registration, so each would leave one behind.
- **A client the person registers and gives Roundhouse, with its secret.** It's the only way in to the rest, and asks the person to set an app up at the service first. It can be added later.

## What it costs

- **What a service registered stays there** once the server is removed from Roundhouse, where the service has no way to delete it.
- **A service that has forgotten the registration shows an error on its own page,** and sends nothing back that says so. So the person signs in once more, or twice more if the hub has restarted between.
- **A sign-in that isn't finished can leave a registration that nothing uses.**
- **It goes once services take the document,** as the `registering` module of `crates/tool-sign-in` says.
