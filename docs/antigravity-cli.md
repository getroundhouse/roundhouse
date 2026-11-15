# Antigravity CLI

How `agy acp` behaves on the wire, for an adapter that drives it. Antigravity speaks the Agent Client Protocol (ACP 1.0) over stdio JSON-RPC 2.0, calling a conversation a **session** (`sessionId`) on the wire and streaming every turn item through `session/update` notifications while `session/prompt` stays open.

The observations here are from Antigravity CLI (`agy`) 1.5.9 on an Apple silicon Mac. Scripted trials used a local loopback stand-in for the Gemini `streamGenerateContent` SSE endpoint. Real-model probes used `gemini-3.8-flash` and `gemini-4-carbon` under the machine's own Google OAuth sign-in.

## Driving it

Run `agy acp --disable-telemetry`. Both directions are newline-delimited JSON-RPC 2.0: every request, response and notification carries `"jsonrpc": "2.0"`. Omitting `"jsonrpc": "2.0"` returns `-32600`. Server requests, such as `session/request_permission` and `elicitation/create`, carry their own `"id"` alongside `"method"` and `"params"`; responses carry `"id"` and either `"result"` or `"error"`.

Initialize the connection before creating or resuming a conversation:

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1,"clientInfo":{"name":"roundhouse","version":"0.1.0"},"clientCapabilities":{"fs":{"readTextFile":false,"writeTextFile":false},"terminal":false,"elicitation":{"form":true}}}}
```

Advertising `fs: false` and `terminal: false` tells `agy` to execute its own filesystem and terminal tools rather than delegating them to the client, while `elicitation.form: true` enables `agy`'s built-in `ask_question` tool to send structured `elicitation/create` requests over ACP rather than falling back to plain text.

Start a fresh conversation with `session/new`, or continue an existing one with `session/resume`, and then apply the mode, model and effort:

```json
{"jsonrpc":"2.0","id":2,"method":"session/new","params":{"cwd":"/scratch/work","mcpServers":[],"_meta":{"antigravity":{"systemPrompt":{"append":"..."}}}}}
{"jsonrpc":"2.0","id":3,"method":"session/set_mode","params":{"sessionId":"SESSION","modeId":"accept-edits"}}
{"jsonrpc":"2.0","id":4,"method":"session/set_config_option","params":{"sessionId":"SESSION","configId":"model","value":"gemini-4-carbon"}}
{"jsonrpc":"2.0","id":5,"method":"session/set_config_option","params":{"sessionId":"SESSION","configId":"thought_level","value":"medium"}}
```

`session/new` and `session/resume` return `sessionId`, `modes`, `models` and `configOptions` (including `model`, `thought_level` and `mode`). `_meta.antigravity.systemPrompt` accepts `{"append": "..."}` or `{"replace": "..."}`; `append` places Roundhouse's instructions inside `<AGENT_PROMPT>` in the model request while keeping them off the command line. Calling `session/set_mode` triggers an asynchronous `config_option_update` carrying whatever model `agy` had active when `set_mode` ran (initially `gemini-3.1-pro`, whose `thought_level` choices omit `medium` and `xhigh`), so the adapter sets the mode first, updates `config_options` from `set_config_option`'s own reply, and ignores `config_option_update` notifications whose `model` `currentValue` doesn't match the session's configured model.

## A turn's output

`session/prompt` takes `sessionId` and a `prompt` array of content blocks (`text`, `image` with base64 `data` and `mimeType`, or `resource_link`). Unlike app-server protocols that acknowledge turn intake immediately, `session/prompt` stays pending for the entire turn and resolves with `{"stopReason": "end_turn" | "cancelled" | "max_tokens" | "max_turn_requests" | "refusal", "usage": {...}}` when the turn ends.

| What | Observed behavior | What an adapter needs to account for |
|---|---|---|
| Prompt intake | `session/update` with `user_message_chunk` arrives as `agy` accepts the prompt into the turn, before model output | Record `PromptTaken` on the first turn notification or server request for the in-flight prompt, or before `TurnEnded` if a short turn finishes without chunks (and `PromptDropped` if cancelled before intake) |
| Thinking and text | Reasoning arrives as `agent_thought_chunk` updates, and assistant prose as `agent_message_chunk` updates, each carrying a fragment in `content.text` | Stream fragments as deltas and flush the accumulated block before tool calls, server requests or the end of the turn |
| Tool calls | Built-in and MCP calls arrive as `tool_call` (`pending` or `in_progress`) followed by `tool_call_update` (`completed` or `failed`), carrying `toolCallId`, `title`, `kind`, `rawInput`, `rawOutput` and Antigravity's top-level `name` and `args` fields | Deduplicate call starts by `toolCallId`, read `name` and `args` when present with `kind`/`rawInput` as fallback, and emit one `CallResult` when status reaches `completed` or `failed` |
| Commands, reads and edits | `run_command` sets `kind: "execute"`, puts the command in `CommandLine`, and wraps output with optional `Created At:`/`Completed At:` timestamps and `Command exited with code N.` before `Command output:` (either as structured `{"exit_code", "output"}` or as a string). `read_file` puts numbered `Showing lines A-B of T` text in `content`. `edit_file` and `write_file` carry `Path` with `Target`/`Replacement` or `Content`. `edit_file` replaces every place `Target` is, and `write_file` with `Append` adds to the end of the file, as their tools tell the model. `run_command`'s `WaitMsBeforeAsync` is how long `agy` waits before it moves the command to the background, not a time limit, and `IsDaemon` marks one meant to run until stopped. `send_message` and `manage_subagents` name a subagent by its conversation's ID | Strip the command envelope and recover its exit code, parse `read_file`'s numbered lines into `CallOutput::Lines`, and record `edit_file` as `Call::Edit`, each `Replace` with `Replaces::Every`, and a `write_file` that doesn't append as `Call::Write`, both with `CallOutput::Changed` (`change: None`, without a synthesized unified diff). Record a command without `timeout_ms`. Record `send_message` and `manage_subagents` as `Call::Other`, since nothing maps a subagent's conversation ID to the call that started it, and an appending `write_file` too, since an append doesn't write the file whole, as a `Call::Write` says |
| Context and turn tokens | Each model step emits `usage_update` with `used` and `size` token counts (such as `1048576` on Gemini models), and `session/prompt`'s result carries `usage` (`inputTokens`, `outputTokens`, `thoughtTokens`, `cachedReadTokens`, `cachedWriteTokens` and `totalTokens`), without prompt-cache TTL, billing cost or account rate-limit windows. Given Gemini's `promptTokenCount` 100, `cachedContentTokenCount` 40, `candidatesTokenCount` 10 and `thoughtsTokenCount` 7, `agy` reported `inputTokens` 60, `cachedReadTokens` 40, `outputTokens` 17, `thoughtTokens` 7 and `totalTokens` 117: input leaves out what was read from the cache, output counts the thinking, and the total is input, output and cache. Each `session/prompt` reports its own requests' | Report `ContextUsage` from `usage_update` and per-turn `TokenUsage` on `TurnEnded` from `inputTokens`, `outputTokens`, `cachedReadTokens` and `cachedWriteTokens`, leaving out `thoughtTokens`, which `outputTokens` holds. Add the `usage` of a held plan's prompt to that of the prompt its approval or feedback sends, since both are one Roundhouse turn. Leave prompt-cache TTL, cost and account-limit indicators absent |
| Session metadata | On the first turn, `agy` can make an internal request to a small flash-lite model to name its conversation, emitting `session_info_update` | Ignore `session_info_update` for Roundhouse titles, which come from Roundhouse's own `title_for` call |

## Steering, cancellation and async prompt errors

| What | Observed behavior | What an adapter needs to account for |
|---|---|---|
| Steering mid-turn | Sending a second `session/prompt` while one is in flight steers the active turn. The superseded `session/prompt` resolves with `{"stopReason": "end_turn"}` and no `usage` before the steering `session/prompt` finishes, and the steering one's `usage` counts every request of the turn, the superseded prompt's included | Track in-flight prompt RPCs in an ordered queue by JSON-RPC request ID. When an earlier RPC resolves with `Ok` while a newer one remains in flight, keep the turn open until the last RPC finishes, and take the turn's tokens from the last |
| Cancelling | `session/cancel` is a notification (`{"sessionId": "..."}`) with no request ID. In-flight commands stop and the active `session/prompt` resolves with `{"stopReason": "cancelled"}` | Cancel open native server requests before sending `session/cancel`, drop any untaken prompts (including a superseded RPC that resolved while cancelling), and record `TurnOutcome::Cancelled` when the final prompt RPC settles |
| Async prompt errors | Because `session/prompt` isn't awaited synchronously at `send_prompt` time, a model-provider failure or invalid steer arrives asynchronously as a JSON-RPC `error` reply on that prompt's request ID, while the `agy acp` process stays alive | Settle any untaken `Seq` on that RPC with `PromptDropped`. If another prompt RPC remains in flight (a failed steer), emit an error log notice and leave the turn running; if it was the last RPC, end the turn with `TurnOutcome::Error` without killing the process |

## Approvals, plans and questions

`agy acp` exposes three modes on `session/set_mode`: `default`, `accept-edits` and `plan`. Passing `--dangerously-skip-permissions` on the command line bypasses permissions for the life of the process and overrides `session/set_mode("plan")`, so the adapter omits that flag and selects `accept-edits` for `FullAccess` and `plan` for `Plan` mode. `session/set_mode` applies immediately, while model and effort changes via `session/set_config_option` apply to the next turn.

- **Full access (`accept-edits`):** File edits inside the workspace run without asking, while shell commands (`run_command`) and accesses outside the workspace send `session/request_permission`. In `FullAccess`, the adapter replies immediately with the `allow_once` option without opening a Roundhouse approval.
- **Plan mode (`plan`):** Mutating tools and commands send `session/request_permission` with `options` (`allow_once`, `allow_always`, `reject_once`). The adapter opens a typed `Asks::Call` approval naming the call in `tool_use_id`, as Claude's and Codex's adapters do. Allowing replies with `allow_once` or `allow_always`; denying records the `toolCallId` so its subsequent failed `tool_call_update` carries `Denied`, replies with `reject_once`, and sends `session/cancel` when `AfterDenial::Stop` is chosen.
- **Approving a plan:** A planning turn that finishes normally ends with `stopReason: "end_turn"` and assistant prose, with no native plan-approval request. When an agent chat in `Plan` mode finishes with non-empty final prose after its last tool call and has no open approval, the adapter holds `TurnEnded` and opens a typed `Asks::Plan` request carrying only that final reply. Approving switches the conversation to `accept-edits` and sends the continuation prompt; `KeepPlanning` stays in `plan` mode and sends the follow-up feedback.
- **Questions (`elicitation/create`):** `agy`'s `ask_question` tool sends `elicitation/create` with `mode: "form"` and a JSON Schema under `requestedSchema.properties`. Single-choice questions use `oneOf` with 1-based `"const"` strings (`"1"`, `"2"`, ...) and `"title"` labels; multi-select questions use `type: "array"` with `items.anyOf`; free-text `"Other"` inputs appear as sibling `<key>_other` string properties. Answering maps Roundhouse's 0-based `picked` indices back to each option's `"const"` string, writes typed `"Other"` text to `<key>_other`, and replies with `{"action": "decline"}` when every answer is empty.

## Worker tools over MCP

`session/new` and `session/resume` accept HTTP MCP servers in `mcpServers`:

```json
{"name":"roundhouse","type":"http","url":"http://127.0.0.1:PORT/mcp","headers":[{"name":"Authorization","value":"Bearer TOKEN"}]}
```

`agy` 1.5.9 speaks MCP `2026-07-28` without advertising `io.modelcontextprotocol/tasks` in `params._meta["io.modelcontextprotocol/clientCapabilities"].extensions`. The worker's stateless HTTP MCP endpoint checks that capability on `server/discover` and `tools/list` as well as protocol version, exposing `get_pull_requests` while omitting `wait_for_pull_requests` when the client cannot run MCP tasks.

When `agy` exposes an MCP tool to its model, it names the tool `mcp_<server>_<tool>` in `name`, sets `title` to `<server>: <tool>`, and injects a `toolSummary` argument into the schema. The adapter uses `title` when it matches `mcp_<server>_<tool>` so server names containing underscores or hyphens stay intact (falling back to the first `_` after `mcp_`), and strips `toolSummary` from translated `Call::Mcp` inputs.

## Resume, accounts, titles and local state

- **Resume:** `agy` stores conversation state under `~/.gemini/antigravity-cli/` (`conversations/<sessionId>.db`, `annotations/<sessionId>.pbtxt` and `brain/<sessionId>/`). `session/resume` of a conversation whose `.db` file is gone, unreadable or corrupt, or that never existed, fails with `-32603` and `Failed to read session <id>; retry, or load it with session/load` in each case, on 1.5.9. So the adapter falls back to `session/new`, with a warning, only when that file is gone from the home it was given, and otherwise lets the launch fail rather than lose a conversation that may still be there. When a resumed conversation sends its next Gemini request, `agy` 1.5.9 appends a trailing `role: "user"` `<SYSTEM_MESSAGE>` notice (reporting that subagents and background tasks stopped across server restart) after the prompt's `<USER_REQUEST>` entry.
- **Catalog and accounts (`agy models`):** For the local path, the compiled catalog offers Gemini 4 Carbon, Hydrogen and Argon, and Gemini 3.8 Flash (Low, Medium and High on Carbon, Argon and 3.8 Flash, plus Extra high on Hydrogen), starting on Gemini 4 Carbon at Medium effort. Starting an ACP conversation creates state on disk, so ambient account detection runs `agy models` instead. `agy models` prints available model IDs without user identity or email, distinguishing Google OAuth (which includes non-Gemini and orchestrator models) from a Gemini API key, and exits non-zero when nobody is signed in. Because `agy models` reports no account identifier, `id` and `email` remain unset.
- **Automatic titles:** `title_for` launches `agy acp --disable-telemetry --include-default-customizations=false` in an empty temporary directory on `gemini-3.8-flash` at `low` effort in `plan` mode (with plan-holding off and `mcpServers: []`), aborts at once if a server request arrives, sends `session/close` with a short timeout when a title is returned, and removes `conversations/<id>.db`, `annotations/<id>.pbtxt` and `brain/<id>/` when `BinaryLookup.home` is set. One deadline covers starting `agy` and asking it, and the files are removed only once `agy` has started, so a start that fails or runs out of time leaves whatever it wrote.
- **App data directory and memory:** Passing `--app-data-dir <dir>` to `agy acp` in 1.5.9 causes `session/new` to fail with `-32603`, and `--settings` is a top-level `agy` flag that the `agy acp` subcommand rejects (`unknown flag: --settings`). Real chats therefore run against the worker machine's default `~/.gemini/antigravity-cli`, while scripted conformance trials isolate `agy` by setting `HOME` to a scratch directory.

## The scripted API

`agy` honours `GOOGLE_GEMINI_BASE_URL` and `GEMINI_API_KEY` when talking to Gemini models:

```sh
HOME=/scratch/home \
GOOGLE_GEMINI_BASE_URL=http://127.0.0.1:PORT \
GEMINI_API_KEY=scripted \
HTTPS_PROXY=http://127.0.0.1:9 \
HTTP_PROXY=http://127.0.0.1:9 \
NO_PROXY=127.0.0.1,localhost \
agy acp --disable-telemetry
```

The stand-in answers `POST /v1beta/models/<model>:streamGenerateContent` with `text/event-stream` SSE frames containing `candidates[0].content.parts` (`{"text": "...", "thought": true}` for thinking, `{"functionCall": {"name": "...", "args": {...}}}` for tool calls, and `{"text": "..."}` for replies) plus `usageMetadata`. Because `agy` appends a trailing `<SYSTEM_MESSAGE>` user turn on resumed conversations and can send background `flash-lite` title requests, the stand-in answers `flash-lite` requests separately and matches scenario scripts against the last `role: "user"` content entry carrying `<USER_REQUEST>`.
