//! What GitHub calls a GitHub account's user, which a client of version 4 reads on the account's record, and a client
//! of the version that reads every observation reads as an observation, on a hub whose database is from before it left
//! the record.

use protocol::client::listed::Listed;
use protocol::client::subscriptions::{Follow, Subscribe};
use protocol::client::{EVERY_OBSERVATION_FROM, HubFrame, HubNotice};
use protocol::model::{DiffComment, DiffSide, GithubAccount, Observation, Observed, Rev};

use super::given::{ok, start};
use crate::hub::GithubUsername;
use crate::hub::tests::hub_in_again;
use crate::server::tests::Connection;
use crate::store::BEFORE_GITHUB_USERS_OBSERVED;
use crate::store::older::store_from_before;

/// The account's record, as the hub's level lists it to `client`, and the observations listed with it.
fn listed_account(client: &Connection) -> (GithubAccount, Vec<Observation>) {
    let records = client.sent.iter().find_map(|frame| {
        let HubFrame::Listed { listed: Listed::Hub(records), .. } = frame else { return None };
        Some(records)
    });
    let records = records.expect("the hub's level listed");
    let account = records.github_accounts.iter().find(|account| account.id.as_str() == "g").cloned();
    (account.expect("the account listed"), records.observations.clone())
}

/// Each copy of the account's record `client` was sent.
fn accounts_sent(client: &Connection) -> Vec<GithubAccount> {
    let copies = client.sent.iter().filter_map(|frame| {
        let HubFrame::Notice { notice: HubNotice::GithubAccount(account), .. } = frame else { return None };
        Some(account.clone())
    });
    copies.collect()
}

/// Whether `sent` tells what GitHub calls the account's user, as `username`.
fn told_its_username(sent: &[HubFrame], username: &str) -> bool {
    sent.iter().any(|frame| {
        let HubFrame::Notice {
            notice: HubNotice::Observation(Observation::GithubUsername { username: said, .. }), ..
        } = frame
        else {
            return false;
        };
        matches!(said, Observed::AsOf { value, .. } if value.as_str() == username)
    })
}

/// A hub on a database from before the username left it lists the account to a client of version 4 named by its
/// user's number, and to one of the version that reads every observation with no observation of it. What next says
/// it fills it, as the hub's read of GitHub does: the first client is sent the record again, named, under a newer
/// revision, and so again as it's renamed, and the second is sent the observation, and no copy of the record.
#[tokio::test]
async fn a_username_reaches_each_client_in_the_form_its_version_reads() {
    let dir = tempfile::tempdir().unwrap();
    let rows = "INSERT INTO github_accounts (id, user_id, username) VALUES ('g', 7, 'ada');";
    drop(store_from_before(dir.path(), BEFORE_GITHUB_USERS_OBSERVED, rows));
    let hub = hub_in_again(dir.path());
    let mut before = Connection::greeted(&hub, 4).await;
    let mut observing = Connection::speaking(&hub, EVERY_OBSERVATION_FROM);
    for client in [&mut before, &mut observing] {
        ok(client, 1, Subscribe { stop: Vec::new(), start: vec![start(1, Follow::Hub)] }).await;
        client.until("the list", |sent| sent.iter().any(|frame| matches!(frame, HubFrame::Listed { .. }))).await;
    }
    let (listed, observations) = listed_account(&before);
    assert_eq!(listed.user.name.as_str(), "#7", "not known, so named by its number");
    assert!(observations.is_empty(), "{observations:?}");
    assert!(listed_account(&observing).1.is_empty(), "not known");

    hub.observation_read::<GithubUsername>("g".into(), "ada".into(), 1_000).await.unwrap();
    hub.observation_read::<GithubUsername>("g".into(), "ada-lovelace".into(), 2_000).await.unwrap();

    let marker = HubNotice::Comment(DiffComment {
        id: "c".into(),
        workspace_id: "w".into(),
        path: "a.rs".into(),
        side: DiffSide::New,
        line: 1,
        text: "after".into(),
        created_at_ms: 1,
        // A copy with no revision is always sent.
        rev: Rev::default(),
    });
    hub.notify(marker.clone());
    for client in [&mut before, &mut observing] {
        let at_the_marker = |sent: &[HubFrame]| {
            sent.iter().any(|frame| matches!(frame, HubFrame::Notice { notice, .. } if *notice == marker))
        };
        client.until("the marker", at_the_marker).await;
    }

    let copies = accounts_sent(&before);
    let names: Vec<&str> = copies.iter().map(|account| account.user.name.as_str()).collect();
    assert_eq!(names, ["ada", "ada-lovelace"]);
    let revs: Vec<Rev> = copies.iter().map(|account| account.rev).collect();
    assert!(revs.is_sorted() && revs.iter().all(|rev| *rev > listed.rev), "{revs:?} after {:?}", listed.rev);
    assert!(!told_its_username(&before.sent, "ada"), "no observation to a client of version 4");
    assert!(told_its_username(&observing.sent, "ada") && told_its_username(&observing.sent, "ada-lovelace"));
    assert_eq!(accounts_sent(&observing), [], "no copy that moves only its username");
}
