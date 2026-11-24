//! What a hub installed as a service found in the update feed, spelled out, since a client and its hub can each be a
//! different build: as its record, and as the observation of the release offered.

use protocol::client::HubNotice;
use protocol::model::{HubUpdate, Observation, ObservationKey, Observed, Records, ReleaseOffer, Rev, UpdateAttempt};

use crate::keeps_its_form;

fn found() -> HubUpdate {
    HubUpdate { looked_at_ms: 1_795_230_000_000, problem: None, attempt: None, rev: Rev::new(5) }
}

#[test]
fn what_a_hub_found_keeps_its_form_as_a_notice_and_in_a_list() {
    let record = r#""looked_at_ms":1795230000000,"problem":null,"rev":5"#;

    keeps_its_form(&HubNotice::HubUpdate(found()), &format!(r#"{{"notice":"hub_update",{record}}}"#));
    let listed = Records { hub_update: Some(found()), ..Records::default() };
    let json = serde_json::to_string(&listed).unwrap();
    assert!(json.contains(&format!(r#""hub_update":{{{record}}}"#)), "{json}");
}

/// How an update a client asked for is going, as a client reads it. A record says none until a client asks for an
/// update, and a state a newer hub adds reads as one this client doesn't know.
#[test]
fn an_update_under_way_or_failed_keeps_its_form_and_none_asked_for_reads_as_none() {
    let under_way = HubUpdate { attempt: Some(UpdateAttempt::UnderWay { to: "0.1.9".into() }), ..found() };
    let failed = UpdateAttempt::Failed { to: "0.1.9".into(), why: "offline".into() };
    let before: HubUpdate =
        serde_json::from_str(r#"{"version":"0.1.9","looked_at_ms":1795230000000,"problem":null,"rev":5}"#).unwrap();
    let newer: UpdateAttempt = serde_json::from_str(r#"{"state":"paused","to":"0.1.9"}"#).unwrap();

    let json = serde_json::to_string(&under_way).unwrap();
    assert!(json.contains(r#""attempt":{"state":"under_way","to":"0.1.9"}"#), "{json}");
    keeps_its_form(&failed, r#"{"state":"failed","to":"0.1.9","why":"offline"}"#);
    assert_eq!(before, found());
    assert_eq!(newer, UpdateAttempt::Unknown);
}

/// A hub that doesn't read the feed, since it isn't installed as a service, lists none, and a client reads a notice a
/// newer hub adds as one it doesn't know, rather than failing the feed.
#[test]
fn a_list_reads_without_it_and_a_client_skips_a_newer_hub_s_notice() {
    let before: Records =
        serde_json::from_str(r#"{"repos":[],"workspaces":[],"sessions":[],"comments":[],"rev":4}"#).unwrap();
    let unknown: HubNotice =
        serde_json::from_str(r#"{"notice":"hub_update_from_a_newer_hub","version":"0.1.9"}"#).unwrap();

    assert_eq!(before.hub_update, None);
    assert!(!serde_json::to_string(&before).unwrap().contains("hub_update"));
    assert_eq!(unknown, HubNotice::Unknown);
}

/// The release the update feed offers, which a hub sends at its own level to a client of a version that reads it so,
/// as a notice, of one offered and of none, and as its removal.
#[test]
fn the_release_offered_keeps_its_form_as_a_notice_and_a_removal() {
    let offered = |version: Option<&str>| {
        let offered = Observed::Current { value: ReleaseOffer { version: version.map(str::to_string) } };
        HubNotice::Observation(Observation::ReleaseOffered { offered, rev: Rev::new(5) })
    };
    let gone = HubNotice::ObservationRemoved { observation: ObservationKey::ReleaseOffered, rev: Rev::new(6) };

    keeps_its_form(
        &offered(Some("0.1.9")),
        r#"{"notice":"observation","kind":"release_offered","offered":{"freshness":"current","value":{"version":"0.1.9"}},"rev":5}"#,
    );
    keeps_its_form(
        &offered(None),
        r#"{"notice":"observation","kind":"release_offered","offered":{"freshness":"current","value":{}},"rev":5}"#,
    );
    keeps_its_form(&gone, r#"{"notice":"observation_removed","observation":{"kind":"release_offered"},"rev":6}"#);
}
