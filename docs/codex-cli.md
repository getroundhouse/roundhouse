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
| Reading a file | A scripted `cat README.md` was a `commandExecution`; its `commandActions` described a read, naming the file and its path | The CLI classifies some shell actions, but leaves out what a command does beyond them, as the next row says, so its classification can't say what a command does. The adapter reads the command itself |
| What a command does | Read from the command parser of Codex 0.169.0, `codex-rs/shell-command/src/parse_command.rs`, not seen in a recorded message. It reads a command to sum up what the agent explored, and types only a script of plain words, quotes, `\|`, `&&`, `\|\|`, `;` and new lines, reading anything else as `unknown`. It drops what it takes for formatting after a pipe or between commands, among them `head`, `sort`, `tee`, `awk` and `xargs` running anything but an in-place edit, such as `xargs rm`. A read through `\| sed -n` is given the whole script as its `command`, whatever else was dropped. It types some programs by what they're given, whatever else they do: `awk` given a file, and `sed -n` given a range whatever other scripts and flags it's given, as a `read`, `head -n`, `tail -n` and `nl` given several files as a `read` of the first, and `nl` given a value it doesn't know a flag takes, as in `nl -n ln a.rs`, as a `read` of that value, `find` with `-name` and `fd` as a `search`, and other `find`s and `fd`s, and some `python -c` scripts, as `listFiles` | Don't take Codex's reading for what a command does. The adapter reads the command's own words, inside the `sh`, `bash` or `zsh -c` Codex runs it with, as `agents::command_parts` says, and says it only reads, lists or searches where it's one program that does nothing else, of one file or place it names: `cat a.rs`, `sed -n '1,40p' a.rs`, `rg -n 'fn main' src` or `ls src`. Anything else, such as a pipe into another program, a list of commands, a glob, a redirect or a flag that runs another program, is `Other`, so the parts never say a command does less than it does |
| Finishing | `turn/completed` held `status: completed` for success, `interrupted` after an interrupt and `failed` after a model error. The thread could become idle before this notification | A thread's idle status isn't a turn outcome |
| What a completed turn holds | A successful command turn's `turn/completed.turn.items` contained the final assistant message, with `itemsView: summary`, and omitted the command and user message. A plan turn's items were empty, with `itemsView: notLoaded` | Build the transcript from item notifications. A turn summary isn't a replay of its items |
| Model failures | A scripted HTTP 500, with provider retries disabled, produced `error` with `willRetry: false` and `codexErrorInfo: internalServerError`, then `turn/completed` with `status: failed`. The app-server stayed alive | A failed turn isn't necessarily a dead process. Retry behavior with the CLI's defaults wasn't qualified |
| Usage | Both paths emitted `thread/tokenUsage/updated` and `account/rateLimits/updated` around model responses. Real trials also brought account and MCP-startup notifications | These aren't assistant text or turn boundaries. The interpretation of limits and context windows wasn't qualified |

### Code Mode and commands

`item/commandExecution/outputDelta` carries transient fragments of a command's output. Roundhouse records the completed item's aggregate output rather than each fragment as an untranslated event. It drops `item/reasoning/summaryPartAdded` and `turn/diff/updated` the same way, since the finished reasoning item and the turn's file changes say them whole, and `thread/readState/changed`, `remoteControl/status/changed` and `turn/moderationMetadata`, which say what Codex's own interfaces read. Other, unrecognized notifications still keep their raw lines for later normalization.

`thread/tokenUsage/updated` says what the thread's newest request held, under `last`, and its running total, under `total`. How full the context is is `last.totalTokens` of `modelContextWindow`, as Codex's own interface counts it. The window is the configuration's, which can be smaller than the model's, and the notification doesn't say which. A subagent's thread sends its own, which its record keeps. `account/rateLimits/updated` names the account's limit on every model as `codex`, with a window of 10,080 minutes seen, and can leave out what hasn't changed.

`warning` carries a message for the person, which Roundhouse shows as a notice. Codex warns in every thread of an under-development feature it's running with, such as `default_mode_request_user_input`, which Roundhouse turns on, so it's launched with `suppress_unstable_features_warning=true`. `mcpServer/startupStatus/updated` says a server is `starting`, `ready`, `failed` or `cancelled`, and Roundhouse shows only a failure, with its `error`, or, when `failureReason` is `reauthenticationRequired`, that it needs signing in to again.

The standalone distribution uses its sibling `codex-code-mode-host` executable for Code Mode. A sandbox that permitted only the `codex` executable made the CLI report that the host couldn't be found. Permitting reads of that installed release's directory let commands work, while the rest of the person's home stayed inaccessible. A worker needs the distribution's supporting executables, not just a copied `codex` binary.

The scripted model invoked `functions.exec` as a Responses API `custom_tool_call`, with JavaScript calling `tools.exec_command`. App-server exposed the nested shell operation as `commandExecution`. The model's tool definitions were in an `additional_tools` input item, not a top-level `tools` array. The scripted API must accept the request the CLI actually sends.

An `exec_command` can yield a session while its command keeps running. Its `processId` on the app-server item is a tool-session identifier: the command's own `$$` was recorded separately to check whether the OS process survived an interrupt.

**A command can run before its item is reported.** In a scripted trial, a command wrote a marker and waited, and the client interrupted as soon as the marker existed. The process was stopped, but no `commandExecution` item had arrived. Waiting for a command's marker therefore doesn't guarantee the transcript already contains its call. When the tool yielded first, the start item arrived; the interruption behavior below was different too.

### Reasoning summaries

Sol and Astra's model metadata in Codex 0.169.0 defaults `default_reasoning_summary` to `none`. With that effective setting, completed reasoning items can have empty `summary` and `content` arrays; an encrypted reasoning item in the native history supplies no display text. The adapter requests `model_reasoning_summary: "auto"` in the thread configuration and `summary: "auto"` on each new root turn. This requests whatever summary the model makes available; it does not guarantee every reasoning item contains one. Title-generation threads keep summaries off.

App-server's reasoning item has separate `summary` and `content` fields; their presence in the schema does not guarantee readable raw reasoning. Roundhouse turns `summary` and `item/reasoning/summaryTextDelta` into thinking text. It does not deserialize `content` into the transcript and ignores `item/reasoning/textDelta`, so readable raw reasoning would not appear even if a model supplied it.

Summary entries can contain Markdown, including entries consisting only of a bold title. The adapter preserves that text and joins a completed item's entries with blank lines. The transcript renders thinking as Markdown in muted text, with bold emphasis at the surrounding text's regular weight, so these titles stay as quiet as other progress notes.

#### Controls and limits

Codex accepts `auto`, `concise`, `detailed` and `none` for `model_reasoning_summary` and a turn's `summary`. OpenAI's [reasoning guide](https://developers.openai.com/api/docs/guides/reasoning#reasoning-summaries) describes `auto` as requesting the most detailed summarizer available for the model, usually equivalent to `detailed`, and says raw reasoning tokens are not exposed. This documents the public API's contract; it does not establish how much summary prose the ChatGPT sign-in route returns. The [CLI's request builder](https://github.com/openai/codex/blob/rust-v0.169.0/codex-rs/core/src/client.rs) passes the requested summary mode to a provider and model that support it; it does not enforce a minimum amount of summary text. Roundhouse explicitly requests `auto` at both thread and turn start, so changing only the person's `model_reasoning_summary` in `config.toml` does not change that request.

`show_raw_agent_reasoning = true` controls displaying raw reasoning that a provider already emits; it does not request or decrypt hidden reasoning. The [Codex configuration guide](https://learn.chatgpt.com/docs/config-file/config-advanced#hide-or-surface-reasoning-events) makes its effect conditional on the model emitting that content. Reasoning effort controls how much the model reasons, and `model_verbosity` controls answer detail; neither is a control for revealing raw reasoning.

### Subagent threads

Codex 0.169.0 multiplexes subagent notifications over the parent's app-server connection. `threadId` identifies the conversation, and `turnId`, or `turn.id` on a turn notification, identifies an invocation. Child turn starts and completions must not change the root turn, consume its queued native user-message echoes, or settle its calls and requests.

A `subAgentActivity` item of kind `started` identifies the spawn call, child thread, task path, resolved model and effort. Its start and completion notifications describe the same spawn operation; neither completion means the child finished. These activity items do not provide the delegated prompt. The observed child work followed the activity and included commands, reasoning and replies on its own thread. `collabAgentToolCall` items separately describe operations such as waiting for agents.

The adapter names each child's subagent by its thread, which is one subagent however many turns it takes, and gives the thread's output that subagent as `parent_tool_use_id`. The spawn call is the call that started it. A child turn's completion supplies its outcome and its last assistant message its report, and only the completion of the turn the child is taking ends it, so a late one for an earlier turn can't end a follow-up. Only the first completion answers the spawn call, and a follow-up's report is its record's. Activity kinds `interacted`, `interrupted` and `completed` do not identify a child turn, so they cannot close a newer invocation on the same thread. A new child turn after one that ended starts the same subagent again, under the spawn call, and late notifications of earlier turns stay with it. A collaboration call names each subagent it's for by its thread, so it's typed for a thread the adapter didn't hear start, such as before it started again, but not for the root's own thread. An unassociated foreign thread is retained as untranslated data, named by its thread, without affecting the root. Root cancellation settles root work only; it does not establish that a child stopped. Live children remain in the background-task list until their native turns end or the process exits.

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

A title uses its own ephemeral thread, an empty working directory, a title-only instruction and explicit Luna/Low, with read-only permissions and no approvals. Its thread config disables hooks, apps, plugins, web search and project documents, and the features a title has no use for: questions in default mode, subagents, the browser, the computer and making images. Codex 0.169.0 offers its model the same tools with these on or off, as seen by capturing its requests to a stand-in model, and refuses a call to one that's off, so only a model that asks would show whether a title's thread setting outweighs the process's `-c` that turns questions on. Nothing answers a title's requests, so one that Codex passes on anyway ends the title at once, without one, rather than waiting out its 90 seconds. Config overrides merge tables: setting `mcp_servers` to an empty object does not clear existing servers. Read the effective config and disable each configured server by name instead. The title call uses no worker tool token and changes no config file.

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

Steering a turn can cross the turn's end. Codex 0.169.0 then refuses the steer as `no active turn to steer`, with code `-32600`, and an interrupt as `no active turn to interrupt`, as seen by asking each of a thread with no turn running. Codex says every refusal with that code, such as of a turn that can't be steered, so only its words tell the turn having ended from another reason. For that one, the adapter waits to hear the turn end, then starts the next turn with the prompt, once, or, for an interrupt, records the turn as it ended. A turn that hasn't ended within 30 seconds leaves the steer refused, or stops the process, for an interrupt. Any other refusal is the prompt's at once, and any other failure of an interrupt stops the process, since the turn may still be going. If Codex's words change, a crossing is taken as another refusal, which is safe.

A refusal, with any code but `-32603`, proves Codex took nothing. `-32603` is an error inside Codex as it carried the request out, which, like a reply with no result, a missing reply or a failed write, doesn't: the adapter stops and drains the process, preserving a native prompt-taken observation or reporting the unconsumed prompt dropped exactly once. It never asks the hub to resend an uncertain intake.

Answering a request goes the same way. When Codex refuses the turn that approving a plan starts, the adapter ends the plan's turn with the refusal and leaves Codex running, and stops it after a failure that may have reached it.

The process handle owns shutdown from before initialization. Abandoning startup or a one-off call kills its process group; supervision reaps the child and removes its PID record.

## Questions in each mode

In Codex 0.169.0, default collaboration mode did not expose `request_user_input` unless `features.default_mode_request_user_input` was enabled. With it enabled, a scripted native question carried `isBlocking: false` in default mode. Holding the response for 30 seconds produced no further model request or turn completion; answering let the turn continue. The adapter enables the feature, but not for a title, and translates the question through the same waiting approval contract as Plan mode. The native flag alone does not establish whether model execution continues.

The native question’s `isOther` defaults to false. With options, that means choice-only; with Other enabled or no options, a typed answer is allowed. `isSecret` makes the typed answer secret. The adapter preserves those distinctions.
