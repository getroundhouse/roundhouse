# Codex CLI

How `codex app-server` behaves on the wire, for an adapter that drives it. Codex calls a conversation a **thread**, and a request to its model can be one of several within a **turn**.

The observations here are from Codex 0.169.0 on an Apple silicon Mac running macOS 27.2. Scripted trials used a local stand-in for the Responses API. Real-model trials used `gpt-6-luna`, at `low` effort, under the machine's own ChatGPT sign-in. A row says when only the scripted model was used. Generated schemas describe fields; they don't establish notification order or what a running process does.

## Driving it

Run `codex app-server --stdio`. Both directions are newline-delimited JSON. A request has `id`, `method` and `params`; a response has the same `id` and either `result` or `error`. Notifications have no `id`. The `jsonrpc` member is omitted. Server requests, such as asking for approval, have their own IDs, which can overlap the client's: a server request with ID `0` isn't a reply to a client request with ID `0`.

Start with these two messages, waiting for the first reply before sending the second:

```json
{"id":1,"method":"initialize","params":{"clientInfo":{"name":"roundhouse_probe","version":"0.1.0"},"capabilities":{"experimentalApi":true}}}
{"method":"initialized","params":{}}
```

`initialize` reports the CLI version in `userAgent`, the Codex home and the platform. With experimental APIs enabled, plan collaboration mode and user-input requests are available. These trials used that setting throughout; the same operations without it weren't tried.

A thread and a turn are separate requests. `/scratch/work` below stands for an empty scratch git repository outside every checkout:

```json
{"id":2,"method":"thread/start","params":{"cwd":"/scratch/work","model":"gpt-6-luna","approvalPolicy":"never","sandbox":"danger-full-access","experimentalRawEvents":false,"config":{"features.memories":false,"features.goals":false,"model_reasoning_effort":"low"}}}
{"id":3,"method":"turn/start","params":{"threadId":"THREAD","model":"gpt-6-luna","effort":"low","input":[{"type":"text","text":"Run exactly `echo hello-from-$((6*7))`, then say what it printed."}]}}
```

The start reply gives the thread ID and effective model, effort, sandbox and approval policy. `turn/start` gives the turn ID and `status: inProgress`; accepting the request isn't the end of the turn. The IDs in these examples stand for those replies, not IDs the client invents.

The CLI can generate its own schema, including experimental fields:

```sh
codex app-server generate-json-schema --experimental --out /tmp/codex-schema
```

Generate it from the CLI being qualified: fields and defaults can change between versions. In this version, `thread/start.sandbox` is a string such as `danger-full-access`, while `turn/start.sandboxPolicy` is an object such as `{"type":"dangerFullAccess"}`.

## A turn's output

| What | Observed behavior | What an adapter needs to account for |
|---|---|---|
| Starting | A fresh thread brought `thread/started`. A turn brought `thread/status/changed` to active, `turn/started`, then `item/started` and `item/completed` for the user's message. The turn reply preceded those turn notifications in the trials | Match replies by request ID, and events by their thread, turn and item IDs. Other notifications can arrive between them |
| Answering | An assistant answer brought `item/started` for an `agentMessage`, `item/agentMessage/delta` fragments, then `item/completed` with the full text. The real model split its short reply across several deltas | Accumulate deltas for the live display; the completed item's text is the full answer, not another fragment |
| Commands | The scripted and real `echo` commands brought a `commandExecution` item with `command`, `cwd`, `commandActions`, `processId` and status. Its completed item held `aggregatedOutput: "hello-from-42\n"`, `exitCode: 0` and `status: completed` | The command's result is distinct from what the assistant later says it printed |
| Reading a file | A scripted `cat README.md` was a `commandExecution`; its `commandActions` described a read, naming the file and its path | The CLI already classifies some shell actions. Preserve that classification rather than assuming every command is an opaque shell use. Other forms of reads and compound commands weren't qualified |
| Finishing | `turn/completed` held `status: completed` for success, `interrupted` after an interrupt and `failed` after a model error. The thread could become idle before this notification | A thread's idle status isn't a turn outcome |
| What a completed turn holds | A successful command turn's `turn/completed.turn.items` contained the final assistant message, with `itemsView: summary`, and omitted the command and user message. A plan turn's items were empty, with `itemsView: notLoaded` | Build the transcript from item notifications. A turn summary isn't a replay of its items |
| Model failures | A scripted HTTP 500, with provider retries disabled, produced `error` with `willRetry: false` and `codexErrorInfo: internalServerError`, then `turn/completed` with `status: failed`. The app-server stayed alive | A failed turn isn't necessarily a dead process. Retry behavior with the CLI's defaults wasn't qualified |
| Usage | Both paths emitted `thread/tokenUsage/updated` and `account/rateLimits/updated` around model responses. Real trials also brought account and MCP-startup notifications | These aren't assistant text or turn boundaries. The interpretation of limits and context windows wasn't qualified |

### Code Mode and commands

The standalone distribution uses its sibling `codex-code-mode-host` executable for Code Mode. A sandbox that permitted only the `codex` executable made the CLI report that the host couldn't be found. Permitting reads of that installed release's directory let commands work, while the rest of the person's home stayed inaccessible. A worker needs the distribution's supporting executables, not just a copied `codex` binary.

The scripted model invoked `functions.exec` as a Responses API `custom_tool_call`, with JavaScript calling `tools.exec_command`. App-server exposed the nested shell operation as `commandExecution`. The model's tool definitions were in an `additional_tools` input item, not a top-level `tools` array. The scripted API must accept the request the CLI actually sends.

An `exec_command` can yield a session while its command keeps running. Its `processId` on the app-server item is a tool-session identifier: the command's own `$$` was recorded separately to check whether the OS process survived an interrupt.

**A command can run before its item is reported.** In a scripted trial, a command wrote a marker and waited, and the client interrupted as soon as the marker existed. The process was stopped, but no `commandExecution` item had arrived. Waiting for a command's marker therefore doesn't guarantee the transcript already contains its call. When the tool yielded first, the start item arrived; the interruption behavior below was different too.

### Reasoning summaries

Sol and Astra's model metadata in Codex 0.169.0 defaults `default_reasoning_summary` to `none`. With that effective setting, completed reasoning items can have empty `summary` and `content` arrays; an encrypted reasoning item in the native history supplies no display text. The adapter requests `model_reasoning_summary: "auto"` in the thread configuration and `summary: "auto"` on each new root turn. This requests whatever summary the model makes available; it does not guarantee every reasoning item contains one. Title-generation threads keep summaries off.

### Subagent threads

Codex 0.169.0 multiplexes subagent notifications over the parent's app-server connection. `threadId` identifies the conversation, and `turnId`, or `turn.id` on a turn notification, identifies an invocation. Child turn starts and completions must not change the root turn, consume its queued native user-message echoes, or settle its calls and requests.

A `subAgentActivity` item of kind `started` identifies the spawn call, child thread, task path, resolved model and effort. Its start and completion notifications describe the same spawn operation; neither completion means the child finished. These activity items do not provide the delegated prompt. The observed child work followed the activity and included commands, reasoning and replies on its own thread. `collabAgentToolCall` items separately describe operations such as waiting for agents.

The adapter associates each child turn with a subagent call and gives its output that call as `parent_tool_use_id`. A child turn's completion supplies its outcome and its last assistant message supplies its report. Activity kinds `interacted`, `interrupted` and `completed` do not identify a child turn, so they cannot close a newer invocation on the same thread. A new child turn gets another call, referring to the preceding invocation with `resumes`; earlier turns keep their associations for late notifications. An unassociated foreign thread is retained as untranslated data without affecting the root. Root cancellation settles root work only; it does not establish that a child stopped. Live children remain in the background-task list until their native turns end or the process exits.

## Steering and interruption

| What | Observed behavior | What an adapter needs to account for |
|---|---|---|
| Steering | While a scripted command waited, `turn/steer` with the active `expectedTurnId` accepted more input and returned that turn ID. The added user message reached the model after the command. A wrong ID returned JSON-RPC error `-32600`, and that rejected text never reached the model | An acknowledgement means the input was accepted, not that a new turn started. Keep its association with the prompt Roundhouse recorded |
| Starting while active | A second `turn/start` during that same wait accepted its input into the existing turn and returned the same turn ID. Only one `turn/started` was emitted | Sending another start isn't how to hold a prompt for a later turn; the hub's queue has to keep it until then |
| Interrupting | `turn/interrupt`, with `threadId` and `turnId`, replied with `{}`. The turn then completed with `status: interrupted`, and the same process accepted another turn. Seen with the scripted and real models | The reply acknowledges the request. Use the turn outcome to record that the turn ended |
| A yielded command | With both models, interrupting after `exec_command` yielded a running shell session ended the turn but left its `sleep` process alive. With the scripted model it survived through a subsequent successful turn. No completed item arrived for the command before the app-server was stopped. Closing the app-server stopped the child | Ending a turn doesn't prove its commands stopped. An adapter cannot claim that guarantee on the strength of `turn/interrupt` alone |
| Cleaning terminals | After the scripted interrupt, `thread/backgroundTerminals/list` named the surviving command by its `processId`. `thread/backgroundTerminals/clean` stopped it; a second list was empty, its OS process was gone, and the same app-server completed the next prompt. The command's completed item arrived after the cleanup reply | Wait for terminal cleanup as well as the interrupted turn before reporting cancellation. The schema also offers `thread/backgroundTerminals/terminate` for one terminal; these observations don't qualify stopping subagents |
| An open approval | Interrupting a scripted turn while a command approval was unanswered prevented the command from running. `turn/completed` reported `interrupted`; `serverRequest/resolved` arrived after it. The command had a start item and no completed item | The adapter needs to settle open requests and calls when a turn ends, without waiting indefinitely for individual completion notifications |

For the yielded-command checks, the shell wrote its OS PID and a start marker before waiting. Its existence was checked after the interrupted outcome, while app-server stayed open, and again after app-server closed. These were foreground shell commands exposed as tool sessions, not detached daemons. Other ways of starting child processes weren't checked.

## Resume and shutdown

| What | Observed behavior | What an adapter needs to account for |
|---|---|---|
| Resuming | After stdin was closed and the first app-server exited, a new process using the same Codex home accepted `thread/resume` with the original thread ID. The scripted model's next request included the earlier conversation. Luna remembered a word supplied before the restart | Persist the CLI's thread ID, and keep its Codex home on the worker's machine. A new app-server process doesn't mean a new conversation |
| Resumed history | `thread/resume` with `excludeTurns: false` returned the previous turn and its full items inside `result.thread.turns`. It also emitted a deprecation notice because the thread used paginated history. It didn't replay the previous messages as `item/started` or `item/completed` notifications | Don't append the returned history as if the agent had just said it again. `excludeTurns: true` successfully resumed the real thread without that full-history response |
| Pagination | The resumed thread reported `historyMode: paginated`. The generated schema offers `thread/turns/list`, `thread/items/list` and an initial turns page on resume | Prefer explicit history fetching when it is needed. Pagination itself wasn't exercised |
| Closing stdin | Closing stdin while idle exited with code 0. Closing it during a scripted running command also exited with code 0, stopped the child, and sent no `turn/completed` | EOF is process shutdown, not a successful turn. Drain output and distinguish requested shutdown from a spontaneous exit |
| Sending SIGTERM | Sending SIGTERM to the scripted app-server's process group during a running command exited with code 0, stopped the child, and sent no `turn/completed` | An exit code of zero doesn't establish that an in-flight turn succeeded. Signalling only the app-server PID, rather than its group, wasn't checked |

Use ephemeral threads for real command and plan probes. A resume probe needs a durable thread; `thread/delete` can remove that probe's thread afterwards. No real-model wire capture belongs in the repo: initialization, accounts, tool configuration and usage can identify the machine or person.

## Approvals and planning

### Command approvals and questions

The command-approval trials used a scripted model, `approvalPolicy: on-request`, `approvalsReviewer: user` and `sandbox: read-only`. The script requested an escalated command that would only create a marker in its scratch repository.

- Before running it, app-server sent `item/started` for a `commandExecution`, then the server request `item/commandExecution/requestApproval`. Its parameters named the thread, turn and item, the command and working directory, the reason and `availableDecisions`.
- Replying with `{"id":0,"result":{"decision":"accept"}}` ran the command and made its marker. The response uses the server request's ID, not the item ID.
- Replying with `decline` didn't make the marker, completed the command item with `status: declined`, and let the model answer. This value was accepted even though the request's advertised choices were `accept`, a rule amendment and `cancel`; a UI should use the choices the request actually offers.
- Replying with `cancel` didn't make the marker either, completed the command item with `status: declined`, and ended the turn with `status: interrupted`, without another model answer.
- A normal reply brought `serverRequest/resolved` as well as a change in the thread's waiting status. An interrupt can put that resolution after the turn's completion, as described above.

In plan mode, a scripted direct call to `functions.request_user_input` brought `item/tool/requestUserInput`, with `isBlocking: true`, question IDs and options. The reply shape was:

```json
{"id":0,"result":{"answers":{"choice":{"answers":["Blue"]}}}}
```

That answer reached the model, which continued in the same turn. This tool was a direct Responses API `function_call` in namespace `functions`; it wasn't callable as `tools.request_user_input` inside Code Mode. Asynchronous questions, free-text answers and secret inputs weren't qualified.

### A plan isn't an approval request

Set plan mode on `turn/start` with:

```json
{"collaborationMode":{"mode":"plan","settings":{"model":"gpt-6-luna","reasoning_effort":"low","developer_instructions":null}}}
```

The collaboration mode carries its own model and effort. The schema says they take precedence over the turn's separate fields, so set them explicitly too. `null` developer instructions selects the mode's built-in instructions.

Both the scripted model's `<proposed_plan>…</proposed_plan>` answer and Luna's real plan produced a typed `plan` item: `item/started`, `item/plan/delta` fragments, then `item/completed` with the plan's text and without the tags. The turn completed successfully. There was no approval server request, and no plan or README file was made in the scratch repository.

To implement the real plan, the client sent a new `turn/start` with `collaborationMode.mode: default`, the same Luna model and low effort, and a prompt approving the plan. Luna then created the requested README. Codex reported two completed turns, one for planning and one for implementation. There is no pending approval request to answer after the plan turn: Roundhouse's plan-approval experience needs its own request and continuation, rather than pretending Codex supplied one.

## The scripted API

Codex can be pointed at a stand-in. The tested provider settings were:

```toml
model = "gpt-6-luna"
model_reasoning_effort = "low"
model_provider = "scripted"

[model_providers.scripted]
name = "Scripted model"
base_url = "http://127.0.0.1:PORT/v1"
wire_api = "responses"
requires_openai_auth = false
supports_websockets = false
request_max_retries = 0
stream_max_retries = 0
```

The stand-in needed no real key or sign-in. It answered HTTP POSTs with `text/event-stream`: a `response.created`, `response.output_item.added` and `response.output_item.done` for each item, text deltas for a message, then `response.completed` containing the response ID, output items and usage. A tool call caused another model request carrying its output. This was enough to run real CLI commands, steer, resume, ask questions and exercise approval and error paths. The HTTP-only provider had received no request when `thread/start` returned in the read-file trial; this doesn't establish that starting a thread on a real provider is free of model traffic.

The isolation follows the scripted Claude trials in [conformance.md](conformance.md):

- Start the CLI with an allowlisted environment, a scratch `HOME` and an existing scratch `CODEX_HOME`. A missing `CODEX_HOME` directory makes app-server exit before initialization. Keep the scratch git repository outside all checkouts, so their instructions don't reach the model.
- Use macOS `sandbox-exec` to deny outbound networking except the stand-in's loopback port, and to deny reading or writing the person's home except for reading the installed CLI release. Keep proxy variables pointed at a refusing loopback port, with only the scripted API bypassing them. The sandbox was checked to refuse access to the person's home directory and a separate listening loopback port while allowing the scripted API.
- Set `features.memories=false` and `features.goals=false`, including in `thread/start.config`. These overrides were accepted; the absence of every memory or goal side effect wasn't independently established. Use `analytics.enabled=false`, not `features.analytics`, which this CLI warns is unrecognized.
- Keep synthetic wire captures apart from real trials. A scripted provider and isolated home are what make a capture synthetic; filtering a real capture for a few known fields doesn't establish that it's safe to keep.

## Automatic titles

A title uses its own ephemeral thread, an empty working directory, a title-only instruction and explicit Luna/Low, with read-only permissions and no approvals. Its thread config disables hooks, apps, plugins, web search and project documents. Config overrides merge tables: setting `mcp_servers` to an empty object does not clear existing servers. Read the effective config and disable each configured server by name instead. The title call uses no worker tool token and changes no config file.

## Worker tools over MCP

Codex 0.169.0 initializes an HTTP MCP server with protocol `2025-06-18`, then sends that version in `MCP-Protocol-Version`. It does not send the `2026-07-28` request metadata or method/name headers. A server that accepts only the newer protocol fails startup before its tools reach the model. The worker accepts both envelopes on the same authenticated endpoint, and the older one exposes `get_pull_request` through the same hub ask. It doesn't expose waits, since that protocol has no tasks extension.

The [2025 lifecycle](https://modelcontextprotocol.io/specification/2025-06-18/basic/lifecycle) defines initialization and version negotiation; its [HTTP transport](https://modelcontextprotocol.io/specification/2025-06-18/basic/transports) defines the subsequent version header. No MCP session record is needed for these ordinary stateless tools.

## What isn't established

These observations don't qualify Windows or Linux process behavior, a CLI version other than 0.169.0, every tool or every failure path. They don't establish how to recover an unanswered request across process death, handle MCP tasks, compact, fork or rewind, interpret all usage fields, or obtain and refresh a subscription login for remote workers. The account's limits being reported on a real turn isn't evidence that a hub can share its credentials.

For the local path, `model/list` offered GPT-6.1 Sol and Terra, GPT-6 Astra, Sol and Luna, and GPT-5.6 Sol, Terra and Luna. All offered Low through Max; all except the two Luna models also offered Ultra. GPT-6.1 Sol was the native default, at Low. The adapter’s compiled catalog records those offerings; live account-specific discovery is not implemented. `account/read` with `refreshToken: false` identified a ChatGPT login, and the CLI used that login for the real trials. No credential was read or copied by the probe.

## Sources

- The installed CLI's generated schema and the live behavior described above.
- [App-server protocol](https://learn.chatgpt.com/docs/app-server), for the API's field and method contracts.
- [Advanced configuration](https://learn.chatgpt.com/docs/config-file/config-advanced), for model providers and configuration overrides.

## Request and process boundaries

Native request IDs belong to one app-server connection. The adapter allocates public approval IDs independently and retains the native ID only for responses and server-request resolution, so a delayed answer cannot approve a later run’s unrelated request.

A steering request can race with native turn completion. The native no-active-turn refusal proves that intake did not happen. The adapter reserves that delivery while the old turn settles and starts its next native turn once, after observing completion. A missing RPC reply or failed write does not prove refusal: the adapter stops and drains the process, preserving a native prompt-taken observation or reporting the unconsumed prompt dropped exactly once. It never asks the hub to resend an uncertain intake.

The process handle owns shutdown from before initialization. Abandoning startup or a one-off call kills its process group; supervision reaps the child and removes its PID record.

## Questions in each mode

In Codex 0.169.0, default collaboration mode did not expose `request_user_input` unless `features.default_mode_request_user_input` was enabled. With it enabled, a scripted native question carried `isBlocking: false` in default mode. Holding the response for 30 seconds produced no further model request or turn completion; answering let the turn continue. The adapter enables the feature and translates the question through the same waiting approval contract as Plan mode. The native flag alone does not establish whether model execution continues.

The native question’s `isOther` defaults to false. With options, that means choice-only; with Other enabled or no options, a typed answer is allowed. `isSecret` makes the typed answer secret. The adapter preserves those distinctions.
