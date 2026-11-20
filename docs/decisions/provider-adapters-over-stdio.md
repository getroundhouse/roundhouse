# A provider's adapter is a separate program that the hub runs over stdio

**Status:** Decided.
**First recorded:** 2026-11-11, in #604.

Every provider has an adapter: a program that makes and ends the provider's instances. The hub runs each adapter as one long-lived child process, talks to it over stdin and stdout in the provider protocol, and starts it again if it stops. So an adapter can be written in any language, and adding one doesn't mean rebuilding the app. The protocol is defined as messages, independent of the transport, because a gRPC API may come later.

The local provider is the exception for now. It runs inside the hub, which calls it through the `Provider` trait.

## Chosen over

No alternative was recorded.
