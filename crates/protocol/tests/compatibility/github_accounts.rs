//! GitHub accounts' and sign-ins' records, as the feed and a list carry them, spelled out, since a client of another
//! build finds a record by its kind's name and its key, what the settings say workers are sent for the default, and
//! what GitHub calls an account's user, as an observation.

use protocol::client::{ClientResponse, HubNotice};
use protocol::frame;
use protocol::model::{
    GithubAccount, GithubAppId, GithubAppSaid, GithubLoginKind, GithubSettings, GithubSignIn, GithubSignInStatus,
    GithubUser, GithubUserId, HeldGithubLogin, Observation, ObservationKey, Observed, RecordKey, Records, Rev,
    SentCredential, UnusableLogin,
};

use crate::keeps_its_form;

const ACCOUNT: &str = r#""id":"g","user":{"id":7,"name":"ada"},"held_login":{"how":"token","connected_at_ms":1,"ends_at_ms":null,"needs_sign_in":false},"rev":2"#;

const SIGN_IN: &str = r#""id":"s","status":{"state":"waiting","code":"WDJB-MJHT","address":"https://github.com/login/device","code_ends_at_ms":900000},"rev":3"#;

fn account() -> GithubAccount {
    GithubAccount {
        id: "g".into(),
        user: GithubUser { id: GithubUserId::new(7), name: "ada".into() },
        held_login: Some(HeldGithubLogin {
            how: GithubLoginKind::Token,
            connected_at_ms: 1,
            ends_at_ms: None,
            needs_sign_in: false,
        }),
        installation: None,
        rev: Rev::new(2),
    }
}

fn sign_in() -> GithubSignIn {
    GithubSignIn { rev: Rev::new(3), ..protocol::fixtures::github_sign_in("s") }
}

fn listed(line: &str) -> Records {
    let ClientResponse::Records(list) = frame::decode::<ClientResponse>(line.as_bytes()).unwrap() else { panic!() };
    list
}

#[test]
fn a_notice_of_a_github_account_reads_as_its_record() {
    let line = format!(r#"{{"notice":"github_account",{ACCOUNT}}}"#);

    let notice = frame::decode::<HubNotice>(line.as_bytes()).unwrap();

    assert_eq!(notice, HubNotice::GithubAccount(account()));
}

#[test]
fn a_list_of_every_record_reads_its_github_accounts_and_one_from_a_hub_that_keeps_none_has_none() {
    let line = format!(
        r#"{{"result":"records","repos":[],"workspaces":[],"sessions":[],"comments":[],"github_accounts":[{{{ACCOUNT}}}],"rev":5}}"#
    );
    let older = r#"{"result":"records","repos":[],"workspaces":[],"sessions":[],"comments":[],"rev":5}"#;

    assert_eq!(listed(&line).github_accounts, [account()]);
    assert_eq!(listed(older).github_accounts, []);
}

/// A login from a hub from before a sign-in could need signing in again says nothing of it.
#[test]
fn a_signed_in_login_keeps_its_form_and_one_from_a_hub_from_before_needs_no_sign_in() {
    let held =
        HeldGithubLogin { how: GithubLoginKind::SignIn, connected_at_ms: 1, ends_at_ms: Some(9), needs_sign_in: true };
    let older = r#"{"how":"sign_in","connected_at_ms":1,"ends_at_ms":9}"#;

    let written = serde_json::to_string(&held).unwrap();
    let read: HeldGithubLogin = serde_json::from_str(older).unwrap();

    assert_eq!(written, r#"{"how":"sign_in","connected_at_ms":1,"ends_at_ms":9,"needs_sign_in":true}"#);
    assert_eq!(read, HeldGithubLogin { needs_sign_in: false, ..held });
}

#[test]
fn a_notice_of_a_sign_in_reads_as_its_record_and_its_removal_by_its_key() {
    let line = format!(r#"{{"notice":"github_sign_in",{SIGN_IN}}}"#);
    let removed = r#"{"notice":"removed","record":{"kind":"github_sign_in","id":"s"},"rev":4}"#;

    let notice = frame::decode::<HubNotice>(line.as_bytes()).unwrap();
    let removal = frame::decode::<HubNotice>(removed.as_bytes()).unwrap();

    assert_eq!(notice, HubNotice::GithubSignIn(sign_in()));
    assert_eq!(removal, HubNotice::Removed { record: RecordKey::GithubSignIn { id: "s".into() }, rev: Rev::new(4) });
}

#[test]
fn every_state_of_a_sign_in_keeps_its_form() {
    let states = [
        (GithubSignInStatus::Approved { account: "g".into() }, r#"{"state":"approved","account":"g"}"#),
        (GithubSignInStatus::Refused, r#"{"state":"refused"}"#),
        (GithubSignInStatus::Ended, r#"{"state":"ended"}"#),
        (GithubSignInStatus::Cancelled, r#"{"state":"cancelled"}"#),
        (GithubSignInStatus::Failed { message: "made up".to_string() }, r#"{"state":"failed","message":"made up"}"#),
    ];

    for (status, form) in states {
        assert_eq!(serde_json::to_string(&status).unwrap(), form);
        assert_eq!(serde_json::from_str::<GithubSignInStatus>(form).unwrap(), status);
    }
}

#[test]
fn a_list_of_every_record_reads_its_sign_ins_and_one_from_a_hub_that_cant_sign_in_has_none() {
    let line = format!(
        r#"{{"result":"records","repos":[],"workspaces":[],"sessions":[],"comments":[],"github_sign_ins":[{{{SIGN_IN}}}],"rev":5}}"#
    );
    let older =
        r#"{"result":"records","repos":[],"workspaces":[],"sessions":[],"comments":[],"github_accounts":[],"rev":5}"#;

    assert_eq!(listed(&line).github_sign_ins, [sign_in()]);
    assert_eq!(listed(older).github_sign_ins, []);
}

/// What the hub's settings say it sends every worker for the default, in each form, and the settings of a hub from
/// before they said, which say nothing of it.
#[test]
fn what_workers_are_sent_keeps_its_form_on_the_settings_and_one_from_a_hub_from_before_says_none() {
    let reason = "The token of ada, the default account, has ended.";
    let forms = [
        (SentCredential::MachineOwn, r#"{"kind":"machine_own"}"#.to_string()),
        (
            SentCredential::NoAccount { reason: "Connect one.".into() },
            r#"{"kind":"no_account","reason":"Connect one."}"#.to_string(),
        ),
        (SentCredential::Account { account: "g".into() }, r#"{"kind":"account","account":"g"}"#.to_string()),
        (
            SentCredential::Blocked { account: "g".into(), why: UnusableLogin::Ended, reason: reason.to_string() },
            format!(r#"{{"kind":"blocked","account":"g","why":"ended","reason":"{reason}"}}"#),
        ),
    ];
    let whys = [
        (UnusableLogin::Disconnected, "disconnected"),
        (UnusableLogin::Ended, "ended"),
        (UnusableLogin::Renewing, "renewing"),
        (UnusableLogin::SignInAgain, "sign_in_again"),
        (UnusableLogin::Newer, "newer"),
        (UnusableLogin::Unreadable, "unreadable"),
        (UnusableLogin::GhLoggedOut, "gh_logged_out"),
        (UnusableLogin::GhOtherUser, "gh_other_user"),
        (UnusableLogin::GhUnread, "gh_unread"),
    ];
    let older: GithubSettings = serde_json::from_str(r#"{"account":"g"}"#).unwrap();

    for (sent, form) in forms {
        let settings = GithubSettings {
            account: Some("g".into()),
            sent: Some(sent),
            left_to_machine: Vec::new(),
            set_up: Vec::new(),
        };
        let written = format!(r#"{{"account":"g","sent":{form}}}"#);
        assert_eq!(serde_json::to_string(&settings).unwrap(), written);
        assert_eq!(serde_json::from_str::<GithubSettings>(&written).unwrap(), settings);
    }
    for (why, form) in whys {
        assert_eq!(serde_json::to_value(why).unwrap(), form);
    }
    assert_eq!(
        older,
        GithubSettings { account: Some("g".into()), sent: None, left_to_machine: Vec::new(), set_up: Vec::new() }
    );
}

/// The providers the hub's settings say it leaves GitHub to the machines of, and those it sets up, by name, and the
/// settings of a hub from before they said, which say none, so every provider's workspaces are taken to be set up, as
/// that hub's were.
#[test]
fn the_providers_left_to_their_machines_keep_their_form_and_a_hub_from_before_leaves_none() {
    let settings = GithubSettings {
        account: None,
        sent: None,
        left_to_machine: vec!["keyed".into(), "local".into()],
        set_up: vec!["incus".into()],
    };
    let written = r#"{"account":null,"left_to_machine":["keyed","local"],"set_up":["incus"]}"#;
    let older: GithubSettings = serde_json::from_str("{}").unwrap();

    assert_eq!(serde_json::to_string(&settings).unwrap(), written);
    assert_eq!(serde_json::from_str::<GithubSettings>(written).unwrap(), settings);
    assert!(!settings.sets_up(&"local".into()) && settings.sets_up(&"vms".into()));
    assert!(older.sets_up(&"local".into()), "{older:?}");
    assert!(older.set_up.is_empty(), "{older:?}");
}

/// What a hub sends while it has no default, as an older client reads it: as one it can't tell, which says nothing of
/// whether agents can push.
#[test]
fn no_account_is_one_an_older_client_cant_tell() {
    #[derive(serde::Deserialize, Debug, PartialEq)]
    #[serde(tag = "kind", rename_all = "snake_case")]
    enum OlderSent {
        MachineOwn,
        Account,
        #[serde(other)]
        Unknown,
    }

    let read: OlderSent = serde_json::from_str(r#"{"kind":"no_account","reason":"Connect one."}"#).unwrap();

    assert_eq!(read, OlderSent::Unknown);
}

/// What GitHub calls a GitHub account's user, which a hub sends at its own level to a client of a version that reads it
/// so, as a notice and as its removal.
#[test]
fn a_github_username_keeps_its_form_as_a_notice_and_a_removal() {
    let username = Observed::Current { value: "ada".into() };
    let copy =
        HubNotice::Observation(Observation::GithubUsername { account: "g-1".into(), username, rev: Rev::new(5) });
    let gone = HubNotice::ObservationRemoved {
        observation: ObservationKey::GithubUsername { account: "g-1".into() },
        rev: Rev::new(6),
    };

    keeps_its_form(
        &copy,
        r#"{"notice":"observation","kind":"github_username","account":"g-1","username":{"freshness":"current","value":"ada"},"rev":5}"#,
    );
    keeps_its_form(
        &gone,
        r#"{"notice":"observation_removed","observation":{"kind":"github_username","account":"g-1"},"rev":6}"#,
    );
}

/// What GitHub says of an app of your own, its bot, and where an installation is, which a hub sends at its own level to
/// a client of a version that reads them so, as notices.
#[test]
fn what_github_says_of_an_app_keeps_its_form_as_a_notice() {
    let app = GithubAppId::new(9);
    let owner = GithubUser { id: GithubUserId::new(2), name: "adalabs".into() };
    let said = GithubAppSaid { slug: "ada-roundhouse".into(), name: "Ada's Roundhouse".into(), owner };
    let said = HubNotice::Observation(Observation::GithubAppSaid {
        app,
        said: Observed::Current { value: said },
        rev: Rev::new(5),
    });
    let bot = GithubUser { id: GithubUserId::new(41), name: "ada-roundhouse[bot]".into() };
    let bot = HubNotice::Observation(Observation::GithubAppBot {
        app,
        bot: Observed::Current { value: bot },
        rev: Rev::new(6),
    });
    let on = Observed::AsOf { value: "adalabs".into(), as_of_ms: 1_795_230_000_000 };
    let on = HubNotice::Observation(Observation::InstalledOn { account: "g".into(), on, rev: Rev::new(7) });

    keeps_its_form(
        &said,
        r#"{"notice":"observation","kind":"github_app_said","app":9,"said":{"freshness":"current","value":{"slug":"ada-roundhouse","name":"Ada's Roundhouse","owner":{"id":2,"name":"adalabs"}}},"rev":5}"#,
    );
    keeps_its_form(
        &bot,
        r#"{"notice":"observation","kind":"github_app_bot","app":9,"bot":{"freshness":"current","value":{"id":41,"name":"ada-roundhouse[bot]"}},"rev":6}"#,
    );
    keeps_its_form(
        &on,
        r#"{"notice":"observation","kind":"installed_on","account":"g","on":{"freshness":"as_of","value":"adalabs","as_of_ms":1795230000000},"rev":7}"#,
    );
}
