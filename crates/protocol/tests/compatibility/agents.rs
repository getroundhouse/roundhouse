//! How a hub answers a client asking what each kind of agent it runs offers, which a client of any later version has to
//! read, so it can offer what an older hub's agents take. How the client asks is in `client_calls`.

use protocol::client::{ClientResponse, HubFrame, ReplyOutcome, admin};
use protocol::frame;
use protocol::model::{AgentFeature, AgentKind, Effort, LoginDescription, ModelName, PermissionMode};

#[test]
fn what_each_kind_offers_reads_and_what_a_newer_hub_adds_reads_as_unknown() {
    let reply = br#"{"frame":"reply","id":5,"response":{"result":"agents","agents":[{"kind":"claude","name":"Claude Code","usage_heading":"Claude usage","models":[{"model":"opus","label":"Opus"},{"model":"haiku","label":"Haiku"}],"efforts":["low","xhigh","ultra"],"modes":["bypassPermissions","plan"],"defaults":{"model":"opus","permission_mode":"bypassPermissions","effort":"xhigh"},"features":["compacts","flies"],"login":{"made_with":"claude setup-token","lasts_ms":31536000000,"from_a_newer_hub":true},"from_a_newer_hub":true},{"kind":"from_a_newer_hub","name":"Other","usage_heading":"Other usage","models":[],"efforts":[],"modes":[],"defaults":{"model":"other","permission_mode":"bypassPermissions","effort":"low"}}]}}"#;

    let HubFrame::Reply { outcome: ReplyOutcome::Response(ClientResponse::Agents(admin::Agents { agents })), .. } =
        frame::decode(reply).unwrap()
    else {
        panic!("not a list of agents");
    };

    let [claude, other] = agents.as_slice() else { panic!("{agents:?}") };
    assert_eq!(
        (claude.kind, claude.name.as_str(), claude.usage_heading.as_str()),
        (AgentKind::Claude, "Claude Code", "Claude usage")
    );
    assert_eq!(claude.label(&ModelName::named("haiku")), Some("Haiku"));
    assert_eq!(claude.efforts, [Effort::Low, Effort::Xhigh, Effort::Unknown]);
    assert_eq!(claude.modes, [PermissionMode::FullAccess, PermissionMode::Plan]);
    assert_eq!((claude.defaults.model.as_str(), claude.defaults.effort), ("opus", Effort::Xhigh));
    assert_eq!(claude.features, [AgentFeature::Compacts, AgentFeature::Unknown]);
    assert_eq!(other.kind, AgentKind::Unknown);
    assert!(other.features.is_empty(), "one left out does nothing it might not");
    let login = LoginDescription { made_with: "claude setup-token".to_string(), lasts_ms: Some(31_536_000_000) };
    assert_eq!(claude.login, Some(login));
    assert_eq!(other.login, None, "one left out is a kind the hub can hold no login for");
}
