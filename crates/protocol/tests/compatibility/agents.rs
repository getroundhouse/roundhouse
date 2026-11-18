//! How a hub answers a client asking what each kind of agent it runs offers, which a client of any later version has to
//! read, so it can offer what an older hub's agents take. How the client asks is in `client_calls`.

use protocol::capability::settings::Effort;
use protocol::client::{ClientResponse, HubFrame, ReplyOutcome, admin};
use protocol::frame;
use protocol::model::{AgentKind, LoginDescription, ModelName};

/// A kind is described by its claims, which say the models a chat can be given and the one it starts on, beside what
/// isn't a capability. A hub from before said each in fields of their own, which a client of this build doesn't read,
/// so it can't read that hub's agents, as it can't read its sessions.
#[test]
fn what_each_kind_offers_reads_from_its_claims_and_what_a_newer_hub_adds_reads_as_unknown() {
    let reply = br#"{"frame":"reply","id":5,"response":{"result":"agents","agents":[{"kind":"claude","name":"Claude Code","usage_heading":"Claude usage","capabilities":{"settings":{"models":{"offered":[{"model":"opus","label":"Opus","efforts":["low","xhigh","future"],"default_effort":"xhigh"},{"model":"haiku","label":"Haiku"}],"default_model":"opus"},"effort":{}},"context":{"compaction":{"keeping":"read"}},"from_a_newer_hub":{}},"login":{"made_with":"claude setup-token","lasts_ms":31536000000,"from_a_newer_hub":true},"from_a_newer_hub":true},{"kind":"from_a_newer_hub","name":"Other","usage_heading":"Other usage","capabilities":{}}]}}"#;

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
    let opus = ModelName::named("opus");
    assert_eq!(claude.efforts_for(&opus), [Effort::Low, Effort::Xhigh, Effort::parse("future").unwrap()]);
    assert_eq!(claude.modes(), [], "claims that offer no mode offer none");
    assert_eq!((claude.defaults().model, claude.defaults().effort), (Some(opus), Some(Effort::Xhigh)));
    assert!(claude.compacts());
    assert_eq!(other.kind, AgentKind::Unknown);
    assert!(!other.compacts(), "one that claims nothing does nothing it might not");
    assert!(other.models().is_empty());
    let login = LoginDescription { made_with: "claude setup-token".to_string(), lasts_ms: Some(31_536_000_000) };
    assert_eq!(claude.login, Some(login));
    assert_eq!(other.login, None, "one left out is a kind the hub can hold no login for");
}

#[test]
fn every_known_agent_kind_round_trips_by_name_and_on_the_wire() {
    for (kind, name) in
        [(AgentKind::Claude, "claude"), (AgentKind::Codex, "codex"), (AgentKind::Antigravity, "antigravity")]
    {
        assert_eq!(kind.as_str(), name);
        assert_eq!(AgentKind::parse(name), Some(kind));
        let encoded = serde_json::to_string(&kind).unwrap();
        assert_eq!(encoded, format!("\"{name}\""));
        assert_eq!(serde_json::from_str::<AgentKind>(&encoded).unwrap(), kind);
    }
    assert_eq!(AgentKind::parse("unknown"), None);
    assert_eq!(AgentKind::parse("future"), None);
}
