//! What a new chat in a workspace asks the hub for, which every client asks alike.

use protocol::capability::settings::Settings;
use protocol::model::{AgentKind, Session};

/// The settings a new chat that runs `agent` asks for, made from the chat `from`, if any: its model and effort, when it
/// runs the same agent, but not plan mode, which belongs to the task at hand. An effort this build can't name, such as
/// a newer hub's, can't be asked for, so the new chat gets what its agent starts one with.
pub fn new_chat_settings(from: Option<&Session>, agent: AgentKind) -> Settings {
    let same = from.filter(|session| session.agent == agent);
    Settings {
        model: same.map(|session| session.model.clone()),
        effort: same.and_then(|session| session.effort.known()),
        ..Settings::default()
    }
}
