# Test server

A simulated hub for testing a client on a phone. The real hub only listens on a Unix socket, so a phone can't reach it until the relay exists. This serves synthetic sessions over HTTP instead, with artificial latency, and streams deltas and terminal output. It's for step 3 of `mobile-poc/PLAN.md`.

```sh
bun mobile-poc/test-server/gen.ts                # Generates the sessions into out/, which is gitignored
PORT=7801 LATENCY_MS=100 bun mobile-poc/test-server/server.ts
```

It binds to `127.0.0.1`, so a phone on Wi-Fi needs `hostname` in `server.ts` changed first. `gen.ts` takes `--only s,m,l,xl`, and the sizes are explained in `docs/performance.md`. Its content is synthetic and deterministic, with no real transcript in it.

## Protocol

| Endpoint | Purpose |
|---|---|
| `GET /run` | Current run config: `{run_id, contender, scenario, session, spawned_at_ms, params}`. `scenario: null` means interactive mode |
| `GET /workspaces` | Sidebar data |
| `GET /sessions` | `[{id, title, total_items}]` |
| `GET /sessions/:id/items?start=&end=` | `{start, end, total, items}`, 500 items max, with simulated latency |
| `GET /diffs/:name` | Patch text |
| `WS /ws/stream` | Send `{"sessions":[...],"chunks_per_sec":60,"duration_ms":30000}`. Receive `{session, seq, text}` per chunk and `{session, done:true}` at the end |
| `WS /ws/terminal` | Send `{"bytes_per_sec":N,"duration_ms":N}`. Receive binary ANSI frames, then text `{done:true, bytes}` |
| `POST /mark` | `{"name":"..."}` is accepted, and did nothing without the timing harness that's since been removed |
| `POST /results` | Saves a result JSON under `out/results/` |

## Items

```jsonc
{ "i": 0, "id": "it_0", "ts": "2026-09-01T00:00:00.000Z", "role": "user|assistant|tool", "blocks": [ /* Block */ ] }
// Block is one of:
{ "type": "text", "text": "markdown" }
{ "type": "thinking", "text": "...", "expanded": false }
{ "type": "tool_use", "name": "Bash", "input": "string, sometimes JSON" }
{ "type": "tool_result", "name": "Bash", "is_error": false, "expanded": true, "text": "..." }
{ "type": "image", "width": 800, "height": 600 }
```

Stream chunks append to one new assistant `text` item at the tail of that session. Streams for IDs with no fixture only update in-memory state.
