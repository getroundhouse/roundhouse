//! The tool servers of the agent's own thread, as Codex says each starts, and what it answers of each.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use protocol::capability::Report;
use protocol::capability::setup::{ServerName, ServerState, ToolServerState};
use protocol::worker::TOOLS_SERVER;
use serde_json::{Value, json};
use tracing::debug;

use super::why::why;
use crate::codex::{running::Shared, subagents::Scope};
use crate::{AgentError, AgentOutput};

/// What Codex last said of each server, in order of their names, so two reports of the same are equal. Held while what
/// it says is reported too, so a report never follows a newer one.
#[derive(Default)]
pub(in crate::codex) struct ToolServers {
    servers: BTreeMap<ServerName, Server>,
    /// How many times Codex has said one's startup moved.
    moves: u64,
    /// Those Codex has been asked to connect to again, and hasn't said it's trying to.
    asked_for: BTreeSet<ServerName>,
}

struct Server {
    state: ServerState,
    error: Option<String>,
    tools: Option<u32>,
    /// What the startup it's as of came to, which the same again says nothing new of.
    came_to: CameTo,
    /// Which of [`ToolServers::moves`] it's as of.
    moved: u64,
}

/// What a startup of a server is at, as Codex says it, with why it failed as it's shown.
#[derive(PartialEq, Eq)]
enum CameTo {
    Starting,
    Ready,
    Failed { reason: Option<String>, why: Option<String> },
    Cancelled,
    Unknown,
}

impl CameTo {
    fn of(params: &Value) -> Self {
        match params["status"].as_str() {
            Some("starting") => CameTo::Starting,
            Some("ready") => CameTo::Ready,
            Some("failed") => CameTo::Failed {
                reason: params["failureReason"].as_str().map(str::to_owned),
                why: params["error"].as_str().and_then(why),
            },
            Some("cancelled") => CameTo::Cancelled,
            _ => CameTo::Unknown,
        }
    }
}

/// What a startup's moving says.
#[derive(Debug, PartialEq, Eq)]
enum Moved {
    /// Nothing that wasn't said: it came to what it had, or it's being tried again, which is told once that's settled.
    NothingNew,
    /// Something new, and no more to ask of it.
    Said,
    /// Something new, as of this move, of which Codex can say more.
    MoreToAsk(u64),
}

impl Server {
    /// Whether the startup it's as of is over.
    fn settled(&self) -> bool {
        match self.came_to {
            CameTo::Ready | CameTo::Failed { .. } | CameTo::Cancelled => true,
            CameTo::Starting | CameTo::Unknown => false,
        }
    }
}

impl ToolServers {
    /// Takes what `params`, a startup's, say of the server `name`. A server that has settled is shown as it settled until
    /// it settles otherwise, unless it's the one asked for, which is shown as it starts: having every failed one tried
    /// again, as [`reload`] does, has Codex say that each is starting and has failed, and that each connected one is
    /// ready, which is nothing new of any but one that now starts.
    fn moved(&mut self, name: &ServerName, params: &Value) -> Moved {
        let came_to = CameTo::of(params);
        let asked_for = self.asked_for.remove(name);
        let again = |had: &Server| had.settled() && came_to == CameTo::Starting && !asked_for;
        if self.servers.get(name).is_some_and(|had| had.came_to == came_to || again(had)) {
            return Moved::NothingNew;
        }
        self.moves += 1;
        let (state, error, more) = match &came_to {
            CameTo::Starting => (ServerState::Starting, None, false),
            CameTo::Ready => (ServerState::Connected, None, true),
            CameTo::Failed { reason, .. } if reason.as_deref() == Some("reauthenticationRequired") => {
                (ServerState::NeedsSignIn, None, false)
            }
            // Also of one that has never been signed in to, which only its runtime status tells from the rest.
            CameTo::Failed { why, .. } => (ServerState::Failed, why.clone(), true),
            // Its startup was stopped, so the agent doesn't have it.
            CameTo::Cancelled => (ServerState::Failed, params["error"].as_str().and_then(why), false),
            CameTo::Unknown => (ServerState::Unknown, None, false),
        };
        self.servers.insert(name.clone(), Server { state, error, tools: None, came_to, moved: self.moves });
        if more { Moved::MoreToAsk(self.moves) } else { Moved::Said }
    }

    /// Takes what Codex answered of the server `name`, `status`, unless its startup has moved since `moved`. Returns
    /// whether that says more than it had said.
    fn answered(&mut self, name: &ServerName, moved: u64, status: &Value) -> bool {
        let Some(server) = self.servers.get_mut(name).filter(|server| server.moved == moved) else { return false };
        match status["runtimeStatus"].as_str() {
            Some("connected") if status["toolsError"].is_null() => {
                server.tools = status["tools"].as_object().and_then(|tools| u32::try_from(tools.len()).ok());
                true
            }
            Some("authenticationRequired") => {
                server.state = ServerState::NeedsSignIn;
                server.error = None;
                true
            }
            _ => false,
        }
    }

    fn report(&self) -> AgentOutput {
        let servers = self.servers.iter().map(|(name, server)| ToolServerState {
            name: name.clone(),
            hubs: None,
            connected_to: None,
            state: server.state,
            error: server.error.clone(),
            tools: server.tools,
        });
        AgentOutput::Report(Report::ToolServers { servers: servers.collect() })
    }
}

/// How Codex says one of a thread's servers is starting, ready, stopped or failed. Its line is never kept, since the
/// `error` of one that failed can hold its URL with its query, and the body of what it replied.
pub(in crate::codex) const STARTUP_MOVED: &str = "mcpServer/startupStatus/updated";

/// [`STARTUP_MOVED`]: one of the thread's servers starting, ready, stopped or failed, which is reported with the rest,
/// and again once Codex has answered what it doesn't say here, unless it's nothing new. A subagent's thread starts the
/// servers again for itself, which says nothing of the agent's.
pub(in crate::codex) async fn moved(shared: &Arc<Shared>, params: &Value, scope: &Scope) {
    let Some(name) = params["name"].as_str().filter(|name| scope.parent.is_none() && *name != TOOLS_SERVER) else {
        return;
    };
    let name = ServerName::from(name);
    let mut servers = shared.tool_servers.lock().await;
    let moved = servers.moved(&name, params);
    if moved == Moved::NothingNew {
        return;
    }
    shared.run.emit(servers.report()).await;
    drop(servers);
    if let (Moved::MoreToAsk(moved), Some(thread)) = (moved, params["threadId"].as_str()) {
        // On a task of its own, since the answer comes through the loop that calls this.
        tokio::spawn(ask(shared.clone(), thread.to_owned(), name, moved));
    }
}

/// Has Codex connect to `server` again, by trying each of its servers that failed, which is all it can be asked: it leaves
/// the connected ones, and their running calls, alone. It answers before they've started, and then says how each comes
/// to.
pub(in crate::codex) async fn reload(shared: &Shared, server: &ServerName) -> Result<(), AgentError> {
    // Before it's asked, since it can say that the server is starting before it answers.
    shared.tool_servers.lock().await.asked_for.insert(server.clone());
    let reloaded = shared.rpc.ask("config/mcpServer/reload", Value::Null).await;
    if reloaded.is_err() {
        shared.tool_servers.lock().await.asked_for.remove(server);
    }
    reloaded.map(drop)
}

/// Reports the servers as a turn of the agent's own starts, whole, with none where it has none. The hub drops what a
/// run says before its launch is answered, and forgets it where it lost the worker's connection meanwhile, and Codex
/// says each server's startup only once.
pub(in crate::codex) async fn turn_started(shared: &Shared) {
    let servers = shared.tool_servers.lock().await;
    shared.run.emit(servers.report()).await;
}

/// Asks how many tools the server `name` has, and whether it needs signing in to. Codex answers of one server of a
/// thread from the connection the thread has. Asked of all of them, it connects to each again.
async fn ask(shared: Arc<Shared>, thread: String, name: ServerName, moved: u64) {
    let params = json!({"threadId":thread,"serverName":name,"detail":"toolsAndAuthOnly"});
    let Ok(answer) = shared.rpc.ask("mcpServerStatus/list", params).await else {
        // Without Codex's words, which can hold what `why` takes out.
        return debug!(name: "codex.tool_server_unsaid", server = %name, "Codex didn't say more of a tool server");
    };
    let Some(status) = answer["data"].as_array().and_then(|all| all.iter().find(|one| one["name"] == name.as_str()))
    else {
        return;
    };
    let mut servers = shared.tool_servers.lock().await;
    if servers.answered(&name, moved, status) {
        shared.run.emit(servers.report()).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What's kept of the server `docs`, which was just said to be `status`, for `reason` where that's said.
    fn after(status: &str, reason: Option<&str>) -> (ToolServers, Moved) {
        let mut servers = ToolServers::default();
        let moved = servers.moved(&"docs".into(), &json!({"status":status,"failureReason":reason,"error":"refused"}));
        (servers, moved)
    }

    /// The move of which Codex can say more.
    fn asked(moved: &Moved) -> Option<u64> {
        if let Moved::MoreToAsk(moved) = moved { Some(*moved) } else { None }
    }

    fn kept(servers: &ToolServers) -> (ServerState, Option<&str>, Option<u32>) {
        let server = &servers.servers[&ServerName::from("docs")];
        (server.state, server.error.as_deref(), server.tools)
    }

    #[test]
    fn each_startup_is_a_state_and_only_a_failed_one_has_why() {
        let cases = [
            ("starting", None, (ServerState::Starting, None), false),
            ("ready", None, (ServerState::Connected, None), true),
            ("failed", None, (ServerState::Failed, Some("refused")), true),
            ("failed", Some("reauthenticationRequired"), (ServerState::NeedsSignIn, None), false),
            ("cancelled", None, (ServerState::Failed, Some("refused")), false),
            ("resting", None, (ServerState::Unknown, None), false),
        ];

        for (status, reason, (state, why), more) in cases {
            let (servers, moved) = after(status, reason);
            assert_eq!((kept(&servers), asked(&moved).is_some()), ((state, why, None), more), "{status} {reason:?}");
        }
    }

    #[test]
    fn an_answer_says_the_tools_of_one_thats_connected_and_whether_one_that_failed_needs_signing_in_to() {
        let answered = |status, answer: Value| {
            let (mut servers, moved) = after(status, None);
            servers.answered(&"docs".into(), asked(&moved).unwrap(), &answer);
            let (state, why, tools) = kept(&servers);
            (state, why.map(str::to_owned), tools)
        };

        let connected = json!({"runtimeStatus":"connected","tools":{"find":{},"read":{}}});
        assert_eq!(answered("ready", connected), (ServerState::Connected, None, Some(2)));
        let unlisted = json!({"runtimeStatus":"connected","tools":{},"toolsError":"refused"});
        assert_eq!(answered("ready", unlisted), (ServerState::Connected, None, None), "not none, but not known");
        let sign_in = json!({"runtimeStatus":"authenticationRequired","tools":{}});
        assert_eq!(answered("failed", sign_in), (ServerState::NeedsSignIn, None, None));
        let failed = json!({"runtimeStatus":"failed","tools":{}});
        assert_eq!(answered("failed", failed), (ServerState::Failed, Some("refused".into()), None));
    }

    #[test]
    fn an_answer_of_before_the_server_moved_again_is_dropped() {
        let (mut servers, moved) = after("ready", None);
        servers.moved(&"docs".into(), &json!({"status":"failed","error":"refused"}));

        let connected = json!({"runtimeStatus":"connected","tools":{"find":{}}});
        assert!(!servers.answered(&"docs".into(), asked(&moved).unwrap(), &connected));

        assert_eq!(kept(&servers), (ServerState::Failed, Some("refused"), None));
    }

    /// As a reload has Codex say of every server, the one that's now well among them.
    #[test]
    fn a_server_tried_again_is_as_it_was_until_it_comes_to_something_else() {
        let docs = ServerName::from("docs");
        let failed = json!({"status":"failed","error":"refused"});
        let (mut servers, moved) = after("failed", None);
        let sign_in = json!({"runtimeStatus":"authenticationRequired","tools":{}});
        servers.answered(&docs, asked(&moved).unwrap(), &sign_in);
        let needs = (ServerState::NeedsSignIn, None, None);

        for again in [json!({"status":"starting"}), failed, json!({"status":"starting"})] {
            assert_eq!(servers.moved(&docs, &again), Moved::NothingNew, "{again}");
            assert_eq!(kept(&servers), needs);
        }
        servers.asked_for.insert(docs.clone());
        assert_eq!(servers.moved(&docs, &json!({"status":"starting"})), Moved::Said, "it's the one asked for");
        assert_eq!(kept(&servers), (ServerState::Starting, None, None));
        let ready = json!({"status":"ready"});
        let moved = servers.moved(&docs, &ready);
        servers.answered(&docs, asked(&moved).unwrap(), &json!({"runtimeStatus":"connected","tools":{"find":{}}}));
        assert_eq!(servers.moved(&docs, &ready), Moved::NothingNew, "a connected server is left alone");
        assert_eq!(kept(&servers), (ServerState::Connected, None, Some(1)), "with its tools");
        let elsewise = json!({"status":"failed","error":"unreachable"});
        assert!(asked(&servers.moved(&docs, &elsewise)).is_some());
        assert_eq!(kept(&servers), (ServerState::Failed, Some("unreachable"), None));
    }
}
