# Terms

What this project's words mean. Code, docs and the interface use each word only in the sense given here. A new term, or a new sense of one, goes here in the change that introduces it.

Start with "Easily confused" and "Ending things", which cover the words most likely to go wrong.

## Parts

| Term | Meaning | In code |
|---|---|---|
| Roundhouse | This product: the desktop app, the CLI, and the hubs and workers behind them. Capitalized as a name, and lowercase only for a binary | `roundhouse`, `roundhoused`, `roundhouse-desktop` |
| Hub | The server that clients connect to. It owns workspaces, sessions and their logs, and leaves the work itself to workers | `hub` |
| Worker | The process that does a workspace's work: its agents, git and scripts. It reports to its hub. There's one per workspace | `worker` |
| Provider | A kind of place a hub can run workers, such as this machine, an Incus instance or a cloud sandbox. The local provider runs workers on this machine, and it isn't special | `Provider`, `LocalProvider` |
| Instance | Where a provider runs one workspace's worker | `instance_id` |
| Client | A program that talks to a hub, such as the desktop app or the CLI. Clients only talk to hubs | `client-core` |
| Agent | The coding agent a worker runs as a child process. Today that's only the Claude CLI | `AgentKind`, `AgentProcess` |
| Subagent | An agent that the agent starts inside its own process to do part of the work, such as a search, with a tool call. It isn't one the worker runs, and its messages name the call that started it as their parent. The transcript draws them in that call | `parent_tool_use_id`, `transcript::calls::agent` |
| Advisor | A stronger model the agent can ask for advice, which reads the whole conversation. The API runs it rather than the agent, so it's a tool call whose result comes in the agent's own messages, and the advice can come encrypted, for the model alone | `ToolCall::Advisor`, `agents::claude::normalize::advisor`, `ContentBlock::ToolResult.encrypted` |
| Adapter | The code that runs one kind of agent and turns its output into events | `agents::claude` |

## Work

| Term | Meaning | In code |
|---|---|---|
| Repository | A git repository added to a hub | `Repo` |
| Project | What the interface calls a repository added to a hub: the entry that holds a codebase's workspaces and settings, with one git repository behind it | `Repo` |
| Workspace | The unit of delegation: a name, a branch and a worktree in one repository, with its sessions and diff comments | `Workspace` |
| Worktree | A workspace's git worktree, the files on disk | `Workspace.path` |
| Title | What a workspace or a chat is called on screen. It comes from the first prompt, or is typed by hand, and one typed by hand is never replaced | `Workspace.title`, `Session.title`, `TitleSource` |
| Base ref | What a workspace's branch was created from. Diffs are taken against it | `base_ref` |
| Session | One conversation with one agent, with its log. A workspace can have several | `Session` |
| Run | One launch of an agent. A session can have many, because an agent can exit and be started again. Its ID never repeats, even across hub restarts | `RunId`, `AgentSpec.run` |
| Turn | From a prompt until the agent finishes responding. A run can have many. A prompt sent mid-turn folds into the running one if the agent has more to do, and otherwise starts the next. The agent starts one by itself when a background task finishes | `TurnEnded`, `Session.turn_started_at_ms`, `SessionStatus::in_turn` |
| Background task | Work an agent started that carries on after its turn ends, such as a command or a subagent. The agent is still at work until it finishes | `AgentTasks`, `SessionStatus::Background` |
| Reply | What the agent said in a turn, as of the event that ended it. A turn the agent was cut off from isn't one | `Session.reply_seq` |
| Prompt | What a person sends to an agent. It's real once it's recorded, and until then a client shows it as pending | `UserPrompt`, `PendingPrompt` |
| Attachment | A file a prompt carries, of any kind, or an image a tool sent the agent, such as a screenshot it read. The worker puts a prompt's files in the worktree's `.context/attachments/`, and the hub keeps every image. The interface says "file" or "image" | `Attachment`, `AttachmentId`, `AttachmentFile`, `NewAttachment`, the `attachments` table |
| Tool call | An agent using one of its tools, such as running a command or editing a file, and the result it got back. They arrive as two events, and the transcript draws them as one line that opens to what the call did | `ContentBlock::ToolUse`, `ContentBlock::ToolResult`, `transcript::calls` |
| Steer | Send a prompt while the agent works, which it reads at its next chance, such as when a running tool finishes. Until then the prompt is waiting, and a stop drops it | `PromptTaken`, `PromptDropped`, `PromptState` |
| Queue | Prompts waiting for the agent to finish, on the session's record. The hub sends each as a turn of its own. It's held, waiting for the person, after a stop or once the agent goes away mid-turn | `Session.queued`, `Session.queue_held`, `QueuedPrompt`, `SessionQueue`, `SessionSendQueued` |
| Approval | An agent asking before it uses a tool. Any client can answer | `ApprovalRequested`, `ApprovalResolved` |
| Question | An approval where the agent is asking a person something, so it needs an answer rather than a yes | `ApprovalDecision::Answer` |
| Approval mode | How much an agent may do without asking, such as "Ask first", "Plan" or "Full access" | `permission_mode` |
| Context | What an agent's model holds of its conversation: the system prompt, tools, messages and so on, counted in tokens, up to the most the model takes. How full it is belongs to the session's record, as the agent last said, and isn't in the log | `ContextUsage`, `ContextPart`, `Session.context`, `WorkerFrame::AgentContext` |
| Limit | How much of an account's use of its agent is left in a window, such as five hours or seven days, and when the window ends. It's the account's, not a session's, so the hub keeps it by account | `UsageLimit`, `LimitWindow`, `EventBody::AgentLimits`, the `account_limits` table |
| Account | The login an agent's CLI runs under on its worker's machine: the model provider, the plan, and the email. Agents under one account share its limits, and there can be several accounts with one model provider. Its ID is the adapter's, opaque, and the same for every agent under that login | `AgentAccount`, `AccountId`, `SessionAccount` |
| Model provider | Who serves an agent's model, such as Anthropic's own API or a cloud's. It's part of an account | `AgentAccount.model_provider` |
| Effort | How hard an agent thinks before it answers, from Low to Max. `None` leaves it to the user's own Claude settings | `Effort`, `Session.effort`, `model_picker` |
| Setup | What happens once a worktree exists: copying local files and running the repository's setup script | `SetupStatus`, `worker::setup` |
| Diff comment | A comment on one line of a workspace's diff, which can be sent to the agent | `DiffComment` |

## On screen

| Term | Meaning | In code |
|---|---|---|
| Mark | A workspace's or chat's status, drawn as a half circle of fine dots: its pull request's colour, a white light while its agent works, amber or red when it needs you, and all white for a reply you haven't read. `docs/spec/look.md` has the rules | `look::mark` |
| Your turn | A chat waiting on the person for an answer or an approval. Its mark turns amber | `Signal::NeedsYou` |
| Sidebar of changes | The sidebar on the right that lists the files a workspace changes against its base. `docs/spec/diff.md` has the rules | `Changes` |
| Home | Every workspace in one list, which shows in place of the workspace from the top of the sidebar. `docs/spec/home.md` has the rules | `home::Home`, `AppView::open_home` |
| Pull request's bar, standing | The strip at the top of the sidebar of changes, in the pull request's colour. It says the workspace's standing, the first thing in the way of merging its pull request, and offers the one thing to do about it. `docs/spec/workspaces.md` has the rules | `desktop::pull_request::bar`, `Standing` |
| Diff tab | A workspace's one tab for a changed file's diff, before its chats. Clicking another file shows that one in it, rather than opening another | `WorkspacePane::diff_tab`, `Showing::Diff` |
| Working line | The line a chat ends with while its agent works: the mark with its light sweeping, and how long the turn has taken so far | `working_line` |
| Outline | The ticks at the right of a chat, one for each prompt, with the one being read in white, and the list of prompts that opens over them, for jumping to one. `docs/spec/chats.md` has the rules | `Transcript::with_outline`, `SessionLog::prompt_lines`, `PromptLine`, `SessionPromptList` |
| Prompt being read | The prompt whose part of the chat is at the top: the last to have reached the top, or at the end of the chat, the last prompt. The outline lights it. It says where someone is in a chat, and has nothing to do with whether a reply is unread | `being_read` |
| Unread | A chat whose newest reply the person hasn't read. The hub keeps how far each chat is read, and a client marks it read as it shows it | `Session::unread`, `read_seq`, `SessionMarkRead` |

## History and delivery

| Term | Meaning | In code |
|---|---|---|
| Event | One entry in a session's log | `SessionEvent`, `EventBody` |
| Log | A session's events, in order. It's the source of truth for what happened | The `events` table |
| Seq | An event's position in its log. The first is 1, and there are no gaps | `Seq` |
| Raw line | The agent's original output, stored beside the event made from it | `raw` |
| Tail | Text the agent is still writing, which isn't in the log yet | `Tail` |
| Delta | One change to the tail. Deltas aren't stored | `StreamDelta` |
| Transcript | What a person sees: the log and tail, drawn as rows | `crates/transcript` |
| Frame | One message on the wire, as one line of JSON | `protocol::frame`, `HubFrame`, `WorkerFrame` |
| Hello | The first request on a connection, which compares protocol versions | `ClientRequest::Hello` |
| Protocol version | The number a client and hub compare in their hello. It only changes for a real break | `protocol::PROTOCOL_VERSION` |
| On-demand hub | One a client started because it needed one, as opposed to one someone chose to run. Only this kind can be restarted from a client | `HubStatus::on_demand`, `roundhoused hub --on-demand` |
| Build version | Which build a binary is, for people to read. A package stamps its published version, `0.1.N`, an install the commit it came from, and anything else says `-dev`. Whether two binaries can talk is up to the protocol version | `protocol::BUILD_VERSION` |
| Build number | Puts the builds an install or a package makes in order, so the app can tell a hub is older. Any other build has none, so it's never replaced | `protocol::BUILD_NUMBER` |
| Stamped build | A build stamped with both a build version and a build number, as an install or a package does. Any other build says `-dev` whatever its code | `protocol::StampedBuild`, `protocol::STAMPED_BUILD` |
| Checkout | A copy of this repository that the app is built from, such as one in a workspace. A build that isn't stamped names its checkout in the sidebar and the window's title, with the note it was opened with, so copies open at once can be told apart | `desktop::checkout::Checkout`, `--note` |
| Hub's copy | The copy of the daemon that a stamped build's hubs run from, at `daemons/<version>-<number>/roundhoused` in the data directory | `Paths::hub_copy`, `Daemon` |
| Subscription | A client following one session's log, from a given seq | `subscription_id` |
| Feed | The hub-wide stream that says a record changed, for sidebars and the like | `subscribe_feed`, `HubNotice` |
| Update feed | The signed list of published builds that the app checks to update itself. Always written in full, so it isn't confused with the hub's feed | `releases.osx.json`, `updates::source`, `UpdateFeedKey` |
| Record | Something a hub owns that changes in place, rather than being added to a log: a repository, workspace, session or comment, or per-user state such as unread | `Repo`, `Workspace`, `Session`, `DiffComment` |
| Revision | When a record last changed, from one counter per hub, so a client can tell which of two copies is newer | `Rev`, `rev` |

## Easily confused

| Word | The senses | What to write |
|---|---|---|
| Session, chat, conversation | The same thing at three levels. A session is ours, a chat is what the interface shows, and a conversation is the agent's own record, which is what it resumes | "Session" in code and docs, "chat" in text people read, and "conversation" only for the agent's side, as in `agent_session_id` |
| Workspace | Ours, or another tool's, such as a Cargo workspace | A bare "workspace" means ours. Always qualify the others |
| Project, repository | The same record. A project is what people see, and a repository is the git codebase behind it | "Project" in the desktop app and its spec, and "repository" in code, the CLI and the other docs, and wherever git itself is meant, as in "repository settings" |
| Claude, Claude Code | The agent, and Anthropic's own product. The Agent SDK's terms bar calling an agent in another product "Claude Code" | "Claude" or "Claude Agent" in the interface. "Claude CLI" for the `claude` binary in docs |
| Workspace, worktree, instance | The record, the files, and where its worker runs | The one you mean. Archiving removes the worktree, not the workspace |
| Name, title | A workspace's name, such as `holly`, is its directory and the branch it started on, and never changes. Its title is what the sidebar shows | "Name" only for the directory and branch. Renaming a workspace changes its title |
| Worker, agent | A worker is our process, and an agent is the CLI it runs | The one you mean. A worker can outlive its agents, and agents can outlive a killed worker |
| Hub, daemon | `roundhoused` is one binary that runs as a hub or as a worker | "Hub" or "worker". "Daemon" only for the binary |
| Run, turn | A run is a process's lifetime, and a turn is one exchange within it | The one you mean. Counting turns can't give a session's status, as a prompt mid-turn adds none |
| Approval mode, permission mode | The same setting. `permission_mode` and `bypassPermissions` are the CLI's names | "Approval mode" in docs and interface text, and `permission_mode` in code |
| Status | `SessionStatus`, `WorkspaceStatus`, `SetupStatus` or `GitStatus` | Say which |
| Provider, model provider | A provider is where a hub runs workers. A model provider is who serves an agent's model | "Provider" alone only for the first. Always "model provider" for the second, in code and in the interface |
| Account | An agent's account is the login its CLI runs under. A hub's accounts, once a hub serves several people, are who may use it | "Account" alone means the agent's, until hubs have their own. Then say which |
| Notice | `EventBody::Notice` is a message in a session's log. `HubNotice` is on the feed, and says a record changed | "Log notice" or "feed notice" |
| Snapshot | `session::Snapshot` is what a subscriber starts from. A provider's `snapshot` saves a filesystem, and isn't built | Say which |
| Restore | `workspace_restore` recreates an archived worktree from its branch. A provider's `restore` rolls back a filesystem, and isn't built | "Restore" alone means the workspace's |
| Log | A session's log, or a log file such as `hub.log` or the setup log | A bare "log" means a session's. Name the file otherwise |
| Settings | The app's own, chosen in Settings and kept on this Mac, or a repository's, in its `.conductor/settings.toml` files | "Settings" alone means the app's. Say "repository settings" for the others |

## Ending things

These sound alike, and do different things.

| Word | Applies to | What it does | Afterwards |
|---|---|---|---|
| Cancel | A turn | Ends the current turn and any setup still running. The agent's process stays up. `session_cancel`, `AgentCancel` | It takes the next prompt |
| Stop, in the interface | A turn | The Stop button and `Cmd-.` mean cancel, not a process stop | As for cancel |
| Interrupt | A turn | What the agent CLI calls a cancel. Only the adapter says this, as in `AgentProcess::interrupt` | As for cancel |
| Interrupted | A session | A status, not an action: the agent went away mid-turn without being asked. A cancel doesn't cause it | The next prompt resumes the conversation |
| Stop, in code | A process | Ends it, and gives it time to finish. `AgentStop`, `Provider::stop` | The next prompt starts a new one |
| Kill | An instance | Like stop, without the time. For a worker that isn't responding. `Provider::kill` | The next prompt starts a new one |
| Close | A session | Stops its agent and hides it from the tabs. The history stays. `session_close` | It can be reopened with `session_reopen` |
| Archive | A workspace | Removes the worktree and keeps the branch. Refuses if work would be lost, unless forced. `workspace_archive` | It can be restored |
| Destroy | An instance | Removes it. `Provider::destroy` | It's gone |
| Discard | A half-made workspace | Removes a workspace that failed while being created, or was left by a crash. `discard_workspace`, `take_unfinished_workspaces` | It's gone |
