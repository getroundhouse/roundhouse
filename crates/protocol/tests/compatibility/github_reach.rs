//! Whether the default GitHub account can push to each repository's repository on GitHub, spelled out: its records, as
//! the feed and a list carry them, since a client of another build finds a record by its kind's name and its key.

use protocol::client::{ClientResponse, HubNotice};
use protocol::frame;
use protocol::model::{GithubReach, GithubRepositoryName, InstalledApp, Reach, RecordKey, Rev};

use super::keeps_its_form;

const CLI: GithubRepositoryName = GithubRepositoryName::named("cli/cli");

/// An app of your own's bot, as GitHub knows its installation.
const BOT: &str = "ada-roundhouse[bot]";

fn reach(reach: Reach) -> GithubReach {
    GithubReach { repo: "r".into(), reach, answered_at_ms: None, stale: false, rev: Rev::new(3) }
}

fn on_adalabs() -> InstalledApp {
    InstalledApp { slug: "ada-roundhouse".into(), on: "adalabs".into() }
}

#[test]
fn a_notice_of_each_reach_keeps_its_form() {
    let notice = |said| HubNotice::GithubReach(reach(said));
    let start = r#"{"notice":"github_reach","repo":"r","reach":{"state":"#;
    let forms = [
        (
            Reach::Pushes { repository: CLI, username: "ada".into() },
            r#""pushes","repository":"cli/cli","username":"ada""#,
        ),
        (
            Reach::Refused {
                repository: CLI,
                username: "ada".into(),
                reason: "Permission denied.".into(),
                installation: None,
            },
            r#""refused","repository":"cli/cli","username":"ada","reason":"Permission denied.""#,
        ),
        (
            Reach::Refused {
                repository: CLI,
                username: BOT.into(),
                reason: "Permission denied.".into(),
                installation: Some(on_adalabs()),
            },
            r#""refused","repository":"cli/cli","username":"ada-roundhouse[bot]","reason":"Permission denied.","installation":{"slug":"ada-roundhouse","on":"adalabs"}"#,
        ),
        (
            Reach::NotFound { repository: CLI, username: "ada".into(), installation: None },
            r#""not_found","repository":"cli/cli","username":"ada""#,
        ),
        (
            Reach::NotFound { repository: CLI, username: BOT.into(), installation: Some(on_adalabs()) },
            r#""not_found","repository":"cli/cli","username":"ada-roundhouse[bot]","installation":{"slug":"ada-roundhouse","on":"adalabs"}"#,
        ),
        (
            Reach::TokenRefused { repository: CLI, username: "ada".into(), installation: None },
            r#""token_refused","repository":"cli/cli","username":"ada""#,
        ),
        (
            Reach::TokenRefused { repository: CLI, username: BOT.into(), installation: Some(on_adalabs()) },
            r#""token_refused","repository":"cli/cli","username":"ada-roundhouse[bot]","installation":{"slug":"ada-roundhouse","on":"adalabs"}"#,
        ),
        (
            Reach::CouldntTell { repository: CLI, why: "offline".into() },
            r#""couldnt_tell","repository":"cli/cli","why":"offline""#,
        ),
        (Reach::MachineOwn { repository: CLI }, r#""machine_own","repository":"cli/cli""#),
        (
            Reach::Blocked { repository: CLI, reason: "ended".into() },
            r#""blocked","repository":"cli/cli","reason":"ended""#,
        ),
        (Reach::NotOnGithub, r#""not_on_github""#),
    ];

    for (said, form) in forms {
        keeps_its_form(&notice(said), &format!(r#"{start}{form}}},"answered_at_ms":null,"stale":false,"rev":3}}"#));
    }
}

/// What GitHub answered, and when, which the hub's last run left and this one hasn't checked again.
#[test]
fn an_answer_the_last_run_left_says_when_github_gave_it() {
    let refused = Reach::Refused {
        repository: CLI,
        username: "ada".into(),
        reason: "Permission denied.".into(),
        installation: None,
    };
    let left = GithubReach { answered_at_ms: Some(1_793_541_600_000), stale: true, ..reach(refused) };

    keeps_its_form(
        &HubNotice::GithubReach(left),
        r#"{"notice":"github_reach","repo":"r","reach":{"state":"refused","repository":"cli/cli","username":"ada","reason":"Permission denied."},"answered_at_ms":1793541600000,"stale":true,"rev":3}"#,
    );
}

/// Keyed by its repository, though the hub never removes one, as it never removes a repository.
#[test]
fn a_reachs_removal_names_its_repository() {
    let removed = HubNotice::Removed { record: RecordKey::GithubReach { repo: "r".into() }, rev: Rev::new(5) };

    keeps_its_form(&removed, r#"{"notice":"removed","record":{"kind":"github_reach","repo":"r"},"rev":5}"#);
}

#[test]
fn a_list_of_every_record_reads_its_reach_and_one_from_a_hub_that_checks_none_has_none() {
    let listed = r#"{"result":"records","repos":[],"workspaces":[],"sessions":[],"comments":[],"github_reach":[{"repo":"r","reach":{"state":"not_on_github"},"rev":3}],"rev":5}"#;
    let older = r#"{"result":"records","repos":[],"workspaces":[],"sessions":[],"comments":[],"rev":5}"#;
    let read = |line: &str| {
        let ClientResponse::Records(list) = frame::decode::<ClientResponse>(line.as_bytes()).unwrap() else { panic!() };
        list.github_reach
    };

    assert_eq!(read(listed), [reach(Reach::NotOnGithub)]);
    assert_eq!(read(older), []);
}

/// From a newer hub, which may say what this build has no word for.
#[test]
fn a_reach_from_a_newer_hub_reads_as_unknown_and_names_no_repository() {
    let newer = r#"{"notice":"github_reach","repo":"r","reach":{"state":"pushes_on_tuesdays","repository":"cli/cli"}}"#;

    let HubNotice::GithubReach(read) = frame::decode::<HubNotice>(newer.as_bytes()).unwrap() else { panic!() };

    assert_eq!((read.reach.repository(), read.rev), (None, Rev::default()));
    assert_eq!(read.reach, Reach::Unknown);
}
