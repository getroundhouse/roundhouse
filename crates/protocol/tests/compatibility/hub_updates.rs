//! What a hub installed as a service found in the update feed, spelled out, since a client and its hub can each be a
//! different build.

use protocol::client::HubNotice;
use protocol::model::{HubUpdate, Records, Rev};

use crate::keeps_its_form;

fn found() -> HubUpdate {
    HubUpdate { version: Some("0.1.9".into()), looked_at_ms: 1_795_230_000_000, problem: None, rev: Rev::new(5) }
}

#[test]
fn what_a_hub_found_keeps_its_form_as_a_notice_and_in_a_list() {
    let record = r#""version":"0.1.9","looked_at_ms":1795230000000,"problem":null,"rev":5"#;

    keeps_its_form(&HubNotice::HubUpdate(found()), &format!(r#"{{"notice":"hub_update",{record}}}"#));
    let listed = Records { hub_update: Some(found()), ..Records::default() };
    let json = serde_json::to_string(&listed).unwrap();
    assert!(json.contains(&format!(r#""hub_update":{{{record}}}"#)), "{json}");
}

/// A hub from before it read the feed, or that doesn't, lists none, and a client from before reads the notice as one it
/// doesn't know, rather than failing the feed.
#[test]
fn a_list_reads_without_it_and_a_client_from_before_skips_its_notice() {
    let before: Records =
        serde_json::from_str(r#"{"repos":[],"workspaces":[],"sessions":[],"comments":[],"rev":4}"#).unwrap();
    let unknown: HubNotice =
        serde_json::from_str(r#"{"notice":"hub_update_from_a_newer_hub","version":"0.1.9"}"#).unwrap();

    assert_eq!(before.hub_update, None);
    assert!(!serde_json::to_string(&before).unwrap().contains("hub_update"));
    assert_eq!(unknown, HubNotice::Unknown);
}
