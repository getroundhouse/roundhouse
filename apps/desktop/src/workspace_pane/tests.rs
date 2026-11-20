use protocol::model::{GithubRepositoryName, GithubUsername, InstalledApp, Reach, SessionId};

use super::chats::neighbor;
use super::pushing::said;

#[test]
fn closing_a_chat_moves_to_the_next_still_open_or_else_the_last() {
    let ids = |names: &[&str]| names.iter().map(|&name| SessionId::from(name)).collect::<Vec<_>>();
    let was_open = ids(&["a", "b", "c", "d", "e"]);

    assert_eq!(neighbor(&was_open, &"c".into(), &ids(&["a", "b", "d", "e"])), Some("d".into()));
    assert_eq!(neighbor(&was_open, &"c".into(), &ids(&["a", "d", "e"])), Some("d".into()), "with the one before");
    assert_eq!(neighbor(&was_open, &"e".into(), &ids(&["a", "b", "c", "d"])), Some("d".into()), "the last");
    assert_eq!(neighbor(&was_open, &"c".into(), &[]), None);
}

/// Each answer that stops a push says what's wrong, then GitHub's words or the block's, and what to do. One that
/// doesn't, or that nothing's known of, says nothing.
#[test]
fn a_workspace_says_why_agents_cant_push_to_its_repository_and_nothing_while_they_can() {
    let repository = GithubRepositoryName::named("cli/cli");
    let ada = || GithubUsername::from("ada");
    let reason = "ada, the default account, was disconnected. Until it's connected again, Roundhouse can't read pull \
                  requests, and agents on the providers it sets up can't push. Connect it again, or choose another \
                  default.";
    let said_of = |reach: Reach| {
        let said = said(&reach).unwrap();
        assert_eq!(said.install_at, None, "only an app's installation is given access on the app's page");
        (said.wrong, said.then)
    };

    let refused = said_of(Reach::Refused {
        repository: repository.clone(),
        username: ada(),
        reason: "Permission to cli/cli.git denied to ada".into(),
        installation: None,
    });
    let not_found = said_of(Reach::NotFound { repository: repository.clone(), username: ada(), installation: None });
    let token_refused =
        said_of(Reach::TokenRefused { repository: repository.clone(), username: ada(), installation: None });
    let blocked = said_of(Reach::Blocked { repository: repository.clone(), reason: reason.into() });

    let cant = "Agents can't push to cli/cli as ada, the default GitHub account.";
    assert_eq!(refused.0, cant);
    assert!(refused.1.starts_with("GitHub says: Permission to cli/cli.git denied to ada. Choose"), "{}", refused.1);
    assert!(refused.1.ends_with("ask cli to give ada access or to approve Roundhouse."), "{}", refused.1);
    assert_eq!(not_found.0, cant);
    assert!(not_found.1.starts_with("GitHub has no such repository that ada can see."), "{}", not_found.1);
    assert!(not_found.1.ends_with("or ask cli to give ada access."), "{}", not_found.1);
    assert_eq!(token_refused.0, cant);
    assert!(token_refused.1.starts_with("GitHub doesn't take its token"), "{}", token_refused.1);
    assert!(token_refused.1.contains("sign in with GitHub again"), "{}", token_refused.1);
    assert_eq!(blocked, ("Agents can't push to cli/cli.".to_string(), reason.to_string()));
    for quiet in [
        Reach::Pushes { repository: repository.clone(), username: ada() },
        Reach::CouldntTell { repository: repository.clone(), why: "GitHub answered 502".into() },
        Reach::MachineOwn { repository: repository.clone() },
        Reach::PushesAsMachineOwn { repository },
        Reach::NotOnGithub,
        Reach::Unknown,
    ] {
        assert_eq!(said(&quiet), None, "{quiet:?}");
    }
}

/// An app of your own's installation on adalabs is named as its bot, and given a repository of adalabs's on the app's
/// page, never by asking the owner to approve Roundhouse. It reaches no other owner's, whatever their case, so for
/// one of those the line says where it's installed, and has no way to the page.
#[test]
fn an_installations_line_names_its_bot_and_says_to_install_its_app_where_the_repository_is() {
    let installation = || Some(InstalledApp { slug: "ada-roundhouse".into(), on: "adalabs".into() });
    let bot = || GithubUsername::from("ada-roundhouse[bot]");
    let ours = GithubRepositoryName::named("AdaLabs/app");
    let theirs = GithubRepositoryName::named("cli/cli");
    let reason = |repository: &GithubRepositoryName| format!("Permission to {repository}.git denied to {}.", bot());
    let refused = |repository: &GithubRepositoryName| {
        let reason = reason(repository);
        said(&Reach::Refused { repository: repository.clone(), username: bot(), reason, installation: installation() })
            .unwrap()
    };
    let not_found = |repository: &GithubRepositoryName| {
        said(&Reach::NotFound { repository: repository.clone(), username: bot(), installation: installation() })
            .unwrap()
    };
    let page = Some("https://github.com/apps/ada-roundhouse/installations/new".to_string());

    let (refused_ours, refused_theirs) = (refused(&ours), refused(&theirs));
    let (not_found_ours, not_found_theirs) = (not_found(&ours), not_found(&theirs));
    let token_refused =
        said(&Reach::TokenRefused { repository: ours.clone(), username: bot(), installation: installation() }).unwrap();

    let install = "Install the app on AdaLabs with access to AdaLabs/app, choose another default, or connect an account \
                   that can push there.";
    assert_eq!(
        refused_ours.wrong,
        "Agents can't push to AdaLabs/app as ada-roundhouse[bot], the default GitHub account."
    );
    assert_eq!(refused_ours.then, format!("GitHub says: {} {install}", reason(&ours)));
    assert_eq!(refused_ours.install_at, page);
    let elsewhere = "Its app is installed only on adalabs, so it can't push to cli's repositories.";
    assert_eq!(
        refused_theirs.wrong,
        "Agents can't push to cli/cli as ada-roundhouse[bot], the default GitHub account."
    );
    assert!(refused_theirs.then.contains(elsewhere), "{}", refused_theirs.then);
    assert_eq!(refused_theirs.install_at, None);
    assert!(not_found_ours.then.contains("install the app on AdaLabs with access to it"), "{}", not_found_ours.then);
    assert_eq!(not_found_ours.install_at, page);
    assert!(not_found_theirs.then.contains("installed only on adalabs"), "{}", not_found_theirs.then);
    assert_eq!(not_found_theirs.install_at, None);
    assert!(token_refused.then.contains("uninstalled from adalabs. Install it there again"), "{}", token_refused.then);
    assert_eq!(token_refused.install_at, page);
    for then in [&refused_ours.then, &refused_theirs.then, &not_found_ours.then, &token_refused.then] {
        assert!(!then.contains("Roundhouse") && !then.contains("sign in"), "{then}");
    }
}
