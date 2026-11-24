// Against a stand-in hub on a Unix socket.
#![cfg(unix)]

use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, ReplyOutcome, admin};
use protocol::model::LoginDescription;

use super::*;
use crate::stand_in_hub::{self, Does};

/// A kind of agent as a hub describes it.
fn described(kind: AgentKind) -> AgentDescription {
    AgentDescription {
        kind,
        name: "Agent".to_string(),
        usage_heading: "Agent usage".to_string(),
        ..protocol::fixtures::claude()
    }
}

/// What a client holds of the agents of a hub that answers asking for them with `answer`.
fn read_from(answer: ReplyOutcome) -> Result<Agents> {
    let dir = tempfile::tempdir().unwrap();
    let mut answer = Some(answer);
    let client = stand_in_hub::connect(&dir.path().join("hub.sock"), move |request| {
        if request != ClientRequest::AgentList {
            return Does::Nothing;
        }
        answer.take().map_or(Does::HangsUp, |answer| Does::Answers(Box::new(answer)))
    });
    Agents::read(&client)
}

#[test]
fn a_new_chat_runs_the_first_kind_the_hub_describes_that_this_build_can_name() {
    let newer = described(AgentKind::Unknown);
    let listed = ClientResponse::Agents(admin::Agents { agents: vec![newer, described(AgentKind::Claude)] });

    let agents = read_from(ReplyOutcome::Response(listed)).unwrap();

    assert_eq!(agents.for_new_chat(), AgentKind::Claude);
    assert_eq!(agents.new_chat(), Some(&described(AgentKind::Claude)));
    assert_eq!(agents.of(AgentKind::Claude), Some(&described(AgentKind::Claude)));
    assert_eq!(agents.of(AgentKind::Unknown), None, "a chat of a kind from a newer hub has no description here");
}

#[test]
fn only_losing_the_connection_fails_asking() {
    let refused = ReplyOutcome::Error(ApiError::new(ErrorCode::Internal, "something else"));
    let agents = read_from(refused).unwrap();
    assert_eq!(agents, Agents::default());
    assert_eq!((agents.new_chat(), agents.for_new_chat()), (None, AgentKind::Claude), "a new chat runs Claude");

    let dir = tempfile::tempdir().unwrap();
    let client = stand_in_hub::connect(&dir.path().join("hub.sock"), |_| Does::HangsUp);

    assert!(Agents::read(&client).is_err());
}

#[test]
fn a_kind_has_a_login_the_hub_can_be_given_when_its_description_says_what_makes_one() {
    let login = Some(LoginDescription { made_with: "claude setup-token".to_string(), lasts_ms: None });
    let claude = AgentDescription { login: login.clone(), ..described(AgentKind::Claude) };
    let newer = AgentDescription { login, ..described(AgentKind::Unknown) };

    let agents = Agents { described: vec![newer, described(AgentKind::Claude), claude.clone()] };

    assert_eq!(agents.with_logins().collect::<Vec<_>>(), [&claude], "not one this build can't name, nor one without");
    assert_eq!(agents.named().count(), 2);
}
