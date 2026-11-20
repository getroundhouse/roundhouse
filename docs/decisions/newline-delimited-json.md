# The wire format is newline-delimited JSON

**Status:** Decided.
**First recorded:** 2026-11-06, in #78. The comparison with Protobuf was added on 2026-11-11, in #518.

Every message in the client, worker and provider protocols is one line of JSON on a byte stream. The stream is a Unix socket or a Windows named pipe for a local client, TLS for a client paired from another machine, and stdio for a worker. JSON lines work over any byte stream, a person can read them, and the agents' output is already JSON.

## Chosen over

- **A schema format, such as Protobuf.** Its rules for adding to a message are the same as the ones the protocols already follow with serde, such as making every new field optional. And neither format helps when a message changes meaning, which is what a break is.
