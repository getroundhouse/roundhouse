use protocol::model::{GithubRepositoryName, GithubUsername, Reach, SessionId};

use super::chats::neighbor;
use super::pushing::{Said, said};

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
    let reason = "Roundhouse's default GitHub account, ada, was disconnected, so this workspace's agents push to GitHub \
                  as nobody. Connect it again in Roundhouse's Settings, or choose another default there.";
    let said_of = |reach: Reach| said(&reach).map_or_default(|Said { wrong, then }| (wrong, then));

    let refused = said_of(Reach::Refused {
        repository: repository.clone(),
        username: ada(),
        reason: "Permission to cli/cli.git denied to ada".into(),
    });
    let not_found = said_of(Reach::NotFound { repository: repository.clone(), username: ada() });
    let token_refused = said_of(Reach::TokenRefused { repository: repository.clone(), username: ada() });
    let blocked = said_of(Reach::Blocked { repository: repository.clone(), reason: reason.into() });

    let cant = "Agents can't push to cli/cli as ada, the default GitHub account.";
    assert_eq!(refused.0, cant);
    assert!(refused.1.starts_with("GitHub says: Permission to cli/cli.git denied to ada. Choose"), "{}", refused.1);
    assert!(refused.1.ends_with("ask cli to give ada access or to approve Roundhouse."), "{}", refused.1);
    assert_eq!(not_found.0, cant);
    assert!(not_found.1.starts_with("GitHub has no such repository that ada can see."), "{}", not_found.1);
    assert_eq!(token_refused.0, cant);
    assert!(token_refused.1.starts_with("GitHub doesn't take its token"), "{}", token_refused.1);
    assert_eq!(blocked, ("Agents can't push to cli/cli.".to_string(), reason.to_string()));
    for quiet in [
        Reach::Pushes { repository: repository.clone(), username: ada() },
        Reach::CouldntTell { repository: repository.clone(), why: "GitHub answered 502".into() },
        Reach::MachineOwn { repository },
        Reach::NotOnGithub,
        Reach::Unknown,
    ] {
        assert_eq!(said(&quiet), None, "{quiet:?}");
    }
}
