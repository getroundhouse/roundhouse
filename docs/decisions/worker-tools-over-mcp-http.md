# The worker serves Roundhouse's tools to its agents over MCP, as HTTP on 127.0.0.1

**Status:** Decided.
**First recorded:** 2026-11-12, in #714.

Agents call tools of Roundhouse's own, such as one that says where the workspace's pull request stands. Each workspace's worker serves them as an MCP server, over Streamable HTTP on 127.0.0.1. An agent's CLI reconnects to an HTTP server by itself, and the server has no process of its own that can die.

## Chosen over

- **A server on the CLI's own stream.** It ends when the agent's process ends.
- **A stdio server that the CLI starts.** When that server dies, whatever was waiting on it is stranded.
