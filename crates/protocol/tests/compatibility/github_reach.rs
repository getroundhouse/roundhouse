//! Whether the default GitHub account can push to each repository's repository on GitHub, spelled out, as the hub's
//! level sends it, as an observation.

use protocol::client::HubNotice;
use protocol::frame;
use protocol::model::{GithubRepositoryName, InstalledApp, Observation, ObservationKey, Observed, Reach, Rev};

use super::keeps_its_form;

const CLI: GithubRepositoryName = GithubRepositoryName::named("cli/cli");

/// An app of your own's bot, as GitHub knows its installation.
const BOT: &str = "ada-roundhouse[bot]";

/// The repository `r`'s reach, as `reach` says, as the hub sends it while it checks it.
fn reach(reach: Reach) -> HubNotice {
    HubNotice::Observation(Observation::RepositoryReach {
        repo: "r".into(),
        reach: Observed::Current { value: reach },
        rev: Rev::new(3),
    })
}

fn on_adalabs() -> InstalledApp {
    InstalledApp { slug: "ada-roundhouse".into(), on: "adalabs".into() }
}

#[test]
fn each_reach_keeps_its_form() {
    let start = r#"{"notice":"observation","kind":"repository_reach","repo":"r","reach":{"freshness":"current","value":{"state":"#;
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
        (
            Reach::Blocked { repository: CLI, reason: "ended".into() },
            r#""blocked","repository":"cli/cli","reason":"ended""#,
        ),
        (Reach::NotOnGithub, r#""not_on_github""#),
    ];

    for (said, form) in forms {
        keeps_its_form(&reach(said), &format!(r#"{start}{form}}}}},"rev":3}}"#));
    }
}

/// From a newer hub, which may say what this build has no word for.
#[test]
fn a_reach_from_a_newer_hub_reads_as_unknown_and_names_no_repository() {
    let newer = r#"{"notice":"observation","kind":"repository_reach","repo":"r","reach":{"freshness":"current","value":{"state":"pushes_on_tuesdays","repository":"cli/cli"}},"rev":3}"#;

    let read = frame::decode::<HubNotice>(newer.as_bytes()).unwrap();

    assert_eq!(read, reach(Reach::Unknown));
    assert_eq!(Reach::Unknown.repository(), None);
}

/// Whether the default can push to a repository, as of a read, as a notice and as its removal.
#[test]
fn a_repositorys_reach_keeps_its_form_as_a_notice_and_a_removal() {
    let reach = Reach::Pushes { repository: GithubRepositoryName::named("cli/cli"), username: "ada".into() };
    let reach = Observed::AsOf { value: reach, as_of_ms: 1_795_230_000_000 };
    let copy = HubNotice::Observation(Observation::RepositoryReach { repo: "r".into(), reach, rev: Rev::new(5) });
    let gone = HubNotice::ObservationRemoved {
        observation: ObservationKey::RepositoryReach { repo: "r".into() },
        rev: Rev::new(6),
    };

    keeps_its_form(
        &copy,
        r#"{"notice":"observation","kind":"repository_reach","repo":"r","reach":{"freshness":"as_of","value":{"state":"pushes","repository":"cli/cli","username":"ada"},"as_of_ms":1795230000000},"rev":5}"#,
    );
    keeps_its_form(
        &gone,
        r#"{"notice":"observation_removed","observation":{"kind":"repository_reach","repo":"r"},"rev":6}"#,
    );
}
