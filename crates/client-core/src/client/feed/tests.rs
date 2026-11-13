// Against a stand-in hub on a Unix socket.
#![cfg(unix)]

use std::path::Path;
use std::time::{Duration, Instant};

use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, ReplyOutcome, admin};
use protocol::model::{AgentDescription, AgentKind, Effort, ModelName, PermissionMode, Rev, SessionSettings};

use super::*;
use crate::stand_in_hub::{self, Does};

const TIMEOUT: Duration = Duration::from_secs(10);

fn claude() -> AgentDescription {
    AgentDescription {
        kind: AgentKind::Claude,
        name: "Claude".to_string(),
        usage_heading: "Claude usage".to_string(),
        models: Vec::new(),
        efforts: vec![Effort::High],
        modes: vec![PermissionMode::FullAccess],
        defaults: SessionSettings {
            model: ModelName::named("opus"),
            permission_mode: PermissionMode::FullAccess,
            effort: Effort::High,
        },
        features: Vec::new(),
    }
}

/// Follows a stand-in hub with one workspace, which answers asking for its agents with `agents`, and waits until the
/// copy has caught up.
fn following(dir: &Path, agents: ReplyOutcome) -> RecordsFeed {
    let mut agents = Some(agents);
    let client = stand_in_hub::connect(&dir.join("hub.sock"), move |request| {
        let answer = if matches!(request, ClientRequest::FeedSubscribe(_)) {
            ReplyOutcome::Response(ClientResponse::Ok)
        } else if request == ClientRequest::AgentList {
            agents.take().unwrap()
        } else if request == ClientRequest::RecordsList {
            let workspaces = vec![protocol::fixtures::workspace("w")];
            ReplyOutcome::Response(ClientResponse::Records(Records {
                workspaces,
                rev: Rev::new(1),
                ..Records::default()
            }))
        } else {
            return Does::Nothing;
        };
        Does::Answers(Box::new(answer))
    });
    let feed = RecordsFeed::start(&client);
    let deadline = Instant::now() + TIMEOUT;
    while !feed.connected() {
        assert!(Instant::now() < deadline, "the copy never caught up");
        std::thread::sleep(Duration::from_millis(10));
    }
    feed
}

#[test]
fn the_copy_holds_what_the_hubs_agents_offer_beside_its_records() {
    let dir = tempfile::tempdir().unwrap();

    let feed =
        following(dir.path(), ReplyOutcome::Response(ClientResponse::Agents(admin::Agents { agents: vec![claude()] })));

    let shown = feed.snapshot();
    assert_eq!(shown.agents().of(AgentKind::Claude), Some(&claude()));
    assert_eq!(shown.workspaces().len(), 1);
}

#[test]
fn a_hub_too_old_to_describe_its_agents_still_has_its_records_followed() {
    let dir = tempfile::tempdir().unwrap();
    let refused = ReplyOutcome::Error(ApiError::new(ErrorCode::Unsupported, "an older hub"));

    let feed = following(dir.path(), refused);

    let shown = feed.snapshot();
    assert_eq!(*shown.agents(), Agents::default());
    assert_eq!(shown.workspaces().len(), 1);
}
