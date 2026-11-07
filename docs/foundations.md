# Foundations

Some problems keep coming back in this project. Each one has a single mechanism, listed here. A new feature uses it. If it doesn't fit, change it where it lives, along with everything that uses it, rather than solving the problem a second way.

This describes the code as it is. `docs/architecture.md` explains the wider design, `docs/decisions.md` says why, and `docs/terms.md` says what the words mean.

## A session's history

| Problem | Mechanism | Where |
|---|---|---|
| Events from several sources need one order, with no gaps | `Store::append` gives each event `last_seq + 1` in a transaction, on the store's one writer connection. `LiveSession::record` takes its `recording` lock, so events reach the store one at a time | `hub::store`, `hub::session` |
| Nobody should see a status without its event, or the reverse | A status change commits in the same transaction as its event, through `append`'s `new_status` | `hub::store` |
| A retried prompt mustn't count twice | Prompts carry a `client_msg_id`, unique within a session. A repeat returns the original event, marked as a duplicate | `hub::store` |
| A prompt has to survive whatever happens next | It's recorded before anything else is tried, and the client gets its reply at that point. Later failures go in the log, where every client can see them | `LiveSession::handle_prompt` |
| A slow or returning client mustn't lose anything or hold others back | Clients get events from the log, and live items only say the log moved. A client that falls behind goes back to the log. To reconnect, subscribe from one past the last `seq` | `hub::server::pump` |
| A client joining mid-message needs the partial text, with no gap or overlap | A `Tail` snapshot, then deltas. `version` orders every live change, so a subscriber skips what its snapshot already has. Events are only read up to the `seq` a live item names, so an event can't overtake the deltas before it. Deltas aren't stored | `protocol::tail`, `hub::session`, `hub::server` |
| Clients need to hear when a record changes | `Hub::notify` sends a `HubNotice` on the feed. A client that falls behind gets `Resync` and lists everything again | `hub::server::pump_feed` |
| Clients opening a workspace at once mustn't each give it a first session | `session_ensure` returns the newest open session, or makes one, one call at a time | `Hub::session_ensure` |
| Normalizing agent output might turn out to be wrong | Each event keeps the agent's raw line beside its normalized body, in a column that range reads don't load | `hub::store` |
| A long session can't be held in memory | A client holds a window of the log, paged by `seq`, and never all of it | `client_core::session_log` |

## Agents

| Problem | Mechanism | Where |
|---|---|---|
| A late event from a replaced agent mustn't change the session | Each launch has a run ID. Events from any other run go in the log, but don't change the session's status | `hub::session::ingest_event` |
| An approval nobody can answer would leave a session stuck | Whenever an agent goes away, its open approvals are closed with `cancelled_approval` | `LiveSession::cancel_approvals`, `hub::recover` |
| Waiting on a worker mustn't stop agent output being read | Replies and output share one stream, so each session has two tasks: `run_ingest` records what arrives, and `run_commands` does the waiting | `hub::session` |
| What's shown as a session's model and approval mode has to be true | A change goes to a running agent first, and is saved only once it agrees. The agent reports every change of mode itself, including leaving plan mode, and the hub's record follows that | `LiveSession::configure`, `LiveSession::note_mode` |
| A change has to reach whichever agent runs next | `configuring` is held while a change is made and while an agent starts, so a change can't fall between the two | `hub::session` |
| A conversation has to continue across launches | `agent_opened` decides between `--session-id` and `--resume`, and the adapter retries once with the other flag if the CLI refuses | `LiveSession::ensure_agent`, `agents::claude` |
| A CLI upgrade can change what the agent sends | A contract test replays captured real output. After an upgrade, re-capture it and rerun the test. What has no normalized form becomes `AgentOther` | `crates/agents/tests/claude_contract.rs`, `tools/smoke/capture_claude_fixture.py` |

## Processes

| Problem | Mechanism | Where |
|---|---|---|
| Two starts of one worker mustn't race, and a slow start mustn't block other workspaces | One gate per workspace. `ensure_worker` checks again once it holds the gate, and `worker_gone` cleans up under the same gate | `Hub::ensure_worker`, `Hub::worker_gone` |
| Stopping has to reach what a process started | A worker has its stdin closed, gets a grace period, then is killed. An agent gets `SIGTERM` on its process group, stops its own tools, and gets `SIGKILL` after a grace period. A setup script's whole group is signalled. Signalling a group is written in three places today, which #76 covers | `LocalProvider::end`, `agents::claude::process`, `worker::serve::stop_setup_script` |
| A killed worker can't stop its agents | Each agent's PID and start time go in a file in the instance directory. The provider stops anything still recorded before the next worker starts. The start time means a reused PID is left alone | `write_pid_file`, `stop_orphaned_agents` |
| A worker can die at any point | When its stream ends, `worker_gone` stops the instance and tells its sessions. The next prompt starts a new worker and agent | `Hub::worker_gone`, `LiveSession::agent_lost` |
| A hub can die at any point | `recover` runs at startup. It closes open approvals, records an `AgentExited` with `HubRestart` and sets `Interrupted` on sessions that were active, fails setups that were cut off, and discards half-made workspaces | `hub::recover` |
| Blocked signals carry across exec, and the desktop app's background threads have them blocked. Without `SIGCHLD`, tokio never sees a child exit | `trestled` clears its signal mask first thing, so everything it starts inherits an empty one | `unblock_signals` in `apps/daemon` |
| Only one hub should run on a data directory | A hub holds `hub.lock` while it runs, so any socket it finds is stale. A client connects, or starts a hub if there's none | `hub::run`, `Client::connect_or_start` |
| Only this user should reach a local hub | The socket is mode 600, which is what authenticates a local client | `hub::run` |
| Agents need the user's environment, not the app's | An app started from Finder has a minimal `PATH`, so the provider asks the login shell for the real one. The adapter removes a parent agent's variables, listed in `SCRUBBED_ENV` | `read_login_path`, `agents::claude::process` |

## Git and files

| Problem | Mechanism | Where |
|---|---|---|
| Archiving mustn't lose work | It refuses uncommitted changes, including untracked files, and commits on no branch, unless forced. It checks before stopping anything, and again once the agent has stopped | `Hub::workspace_archive`, `worker::git::remove_workspace` |
| Git has to behave the same everywhere, and never wait for input | Git runs through `worker::git`'s `git`, which sets `GIT_TERMINAL_PROMPT`, `GIT_OPTIONAL_LOCKS` and `LC_ALL`. `worker::setup` still runs git itself, which #76 covers | `worker::git` |
| Workspace names can collide | `UNIQUE (repo_id, name)` is the real check. A generated name gets 8 tries and a chosen one gets 1, and a clash in git counts too | `Hub::workspace_create`, `hub::names` |
| Servers in different workspaces need different ports | Each workspace gets 10 ports, starting from 55,000, passed in as `CONDUCTOR_PORT` | `Store::next_port_base`, `workspace_env` |
| A prompt can arrive while setup is running | The prompt is recorded straight away and delivered once setup has ended. Cancelling reaches setup directly, because the waiting prompt holds up the session's commands | `Hub::wait_for_setup`, `Hub::session_cancel` |

## Storage and the wire

| Problem | Mechanism | Where |
|---|---|---|
| SQLite is synchronous, and the hub is async | All SQL is in `hub::store`, and it's only called through `Hub::blocking` | `hub::store`, `Hub::blocking` |
| A large read mustn't delay an agent's events | The store has a separate reader connection | `hub::store` |
| The schema changes, and a user can go back to an older build | `MIGRATIONS` run in order, and `PRAGMA user_version` records how many have. A database from a newer build is refused rather than damaged | `hub::store::migrate` |
| Messages have to cross any byte stream | One JSON value per line, up to `MAX_FRAME_BYTES` | `protocol::frame` |
| Peers can be on different versions | Both sides say hello and compare `PROTOCOL_VERSION`. Unknown fields are ignored, and unknown values decode as `Unknown`. #51 lists the enums that don't do this yet | `protocol` |
| A client can't depend on an async runtime | `client-core` is blocking calls and channels. The desktop app calls it through `cx.background_spawn` | `client_core`, `apps/desktop` |
| Errors have to cross processes and still be told apart | `ApiError`, with a closed set of `ErrorCode`s. See "Errors" in `docs/rust.md` | `protocol::client` |

## No mechanism yet

GitHub issues labelled `gap` list problems without one, such as buffering events while a hub is away. Fixing one adds its row here.
