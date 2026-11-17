//! What each of GitHub's answers, and each credential the hub can't check with, comes to. What GitHub's answers are, and
//! the time limit, is tested in `crate::github`.

use protocol::model::{GithubRepositoryName, GithubToken, InstalledApp, Reach};
use protocol::worker::{GitCredential, GitIdentity};

use super::super::reaching::reach;
use super::support::{CLI, account};
use crate::github::{Answer, Github, GithubConfig, Script, StandIn};
use crate::github_logins::DefaultCredential;
use crate::hub::tests::NO_GITHUB;

#[tokio::test]
async fn each_answer_github_gives_an_accounts_push_is_what_the_push_would_meet() {
    let github = StandIn::with(Script {
        pushes: vec![
            ("ada/app", vec![Answer::takes_pushes()]),
            ("cli/cli", vec![Answer::git_says(403, "Permission to cli/cli.git denied to ada.")]),
            ("ada/revoked", vec![Answer::git_says(401, "Invalid username or token.")]),
        ],
        ..Script::default()
    })
    .await;
    let github = Github::new(GithubConfig::at(github.api()));
    let ada = account("ada", "a-token");
    let named = GithubRepositoryName::named;

    let reached = [
        reach(&github, Some(named("ada/app")), &ada).await,
        reach(&github, Some(CLI), &ada).await,
        reach(&github, Some(named("ada/gone")), &ada).await,
        reach(&github, Some(named("ada/revoked")), &ada).await,
    ];

    let username = || "ada".into();
    let reason = "Permission to cli/cli.git denied to ada.".to_string();
    assert_eq!(
        reached,
        [
            Reach::Pushes { repository: named("ada/app"), username: username() },
            Reach::Refused { repository: CLI, username: username(), reason, installation: None },
            Reach::NotFound { repository: named("ada/gone"), username: username(), installation: None },
            Reach::TokenRefused { repository: named("ada/revoked"), username: username(), installation: None },
        ]
    );
}

/// GitHub is asked as git asks it, as `x-access-token` with the installation's token, and the answer names the bot, as
/// GitHub knows it, and where its app is installed.
#[tokio::test]
async fn an_installations_answer_names_its_bot_and_where_its_installed_not_the_username_git_gives() {
    let denied = "Permission to cli/cli.git denied to ada-roundhouse[bot].";
    let github =
        StandIn::with(Script { pushes: vec![("cli/cli", vec![Answer::git_says(403, denied)])], ..Script::default() })
            .await;
    let calls = Github::new(GithubConfig::at(github.api()));
    let identity = GitIdentity {
        name: "ada-roundhouse[bot]".into(),
        email: "41+ada-roundhouse[bot]@users.noreply.github.com".into(),
    };
    let credential = GitCredential {
        username: "x-access-token".into(),
        token: GithubToken::new("ghs_first"),
        identity: Some(identity),
    };
    let installed = InstalledApp { slug: "ada-roundhouse".into(), on: "adalabs".into() };
    let bots = DefaultCredential::Account {
        credential,
        username: "ada-roundhouse[bot]".into(),
        installation: Some(installed.clone()),
    };

    let refused = reach(&calls, Some(CLI), &bots).await;

    let username = "ada-roundhouse[bot]".into();
    assert_eq!(
        refused,
        Reach::Refused { repository: CLI, username, reason: denied.into(), installation: Some(installed) }
    );
    // `x-access-token:ghs_first`.
    assert!(
        github.asked()[0].contains("authorization: basic ec1hy2nlc3mtdg9rzw46z2hzx2zpcnn0\r\n"),
        "{:?}",
        github.asked()
    );
}

/// Each machine's own login, which the hub holds no token for, a block, and a repository on no GitHub, none of which
/// asks GitHub, and a GitHub that can't be reached.
#[tokio::test]
async fn what_the_hub_cant_check_with_says_why_without_asking_github() {
    let github = StandIn::start(Vec::new()).await;
    let calls = Github::new(GithubConfig::at(github.api()));
    let blocked = DefaultCredential::Blocked { reason: "ada was disconnected".into() };
    let nowhere = Github::new(GithubConfig::at(NO_GITHUB.to_string()));

    let machines_own = reach(&calls, Some(CLI), &DefaultCredential::MachineOwn).await;
    let block = reach(&calls, Some(CLI), &blocked).await;
    let not_on_github = reach(&calls, None, &account("ada", "a-token")).await;
    let unreachable = reach(&nowhere, Some(CLI), &account("ada", "a-token")).await;

    assert_eq!(machines_own, Reach::MachineOwn { repository: CLI });
    assert_eq!(block, Reach::Blocked { repository: CLI, reason: "ada was disconnected".into() });
    assert_eq!(not_on_github, Reach::NotOnGithub);
    assert!(github.asked().is_empty(), "{:?}", github.asked());
    assert!(matches!(&unreachable, Reach::CouldntTell { repository, .. } if *repository == CLI), "{unreachable:?}");
}
