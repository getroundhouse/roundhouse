//! The repositories every connected GitHub account can see, together, searched and a page at a time.

use protocol::client::ErrorCode;
use protocol::client::github_repositories::GithubRepositoryList;

use super::{holding, hub_reading, named};

/// Each repository once, by owner and name, with every account that sees it, the default's first, and one only its
/// owner's account sees, as private. A search finds what has it in its owner and name, whatever its case, and a long
/// list comes a page at a time.
#[tokio::test]
async fn every_accounts_repositories_are_listed_together_searched_and_a_page_at_a_time() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, repositories, _, _github) = hub_reading(dir.path(), "https://gitlab.com/ada/elsewhere.git").await;
    let none = hub.github_repository_list(GithubRepositoryList::default()).await.unwrap_err();
    repositories.set("R_project", "example/project");
    repositories.set("R_mine", "ada/Mine");
    repositories.show_only_to("R_mine", "a-token");
    for at in 0..60 {
        repositories.set(&format!("R_{at:02}"), &format!("many/r{at:02}"));
    }
    holding(&hub, "a-token", 7, "ada");
    holding(&hub, "g-token", 8, "grace");
    let list = |search: &str, after: Option<&str>| {
        hub.github_repository_list(GithubRepositoryList { search: search.into(), after: after.map(str::to_owned) })
    };

    let mine = list("MINE", None).await.unwrap();
    let project = list("example/", None).await.unwrap();
    let first = list("many/", None).await.unwrap();
    let rest = list("many/", first.after.as_deref()).await.unwrap();
    let nowhere = list("", Some("a page")).await.unwrap_err();

    assert_eq!(none.code, ErrorCode::Unavailable, "no account to ask as");
    let [seen] = &mine.repositories[..] else { panic!("{mine:?}") };
    assert_eq!((&seen.repository, seen.private, &seen.seen_by), (&named("ada/Mine"), true, &vec!["ada".into()]));
    let [both] = &project.repositories[..] else { panic!("{project:?}") };
    assert_eq!((both.id.as_str(), &both.seen_by), ("R_project", &vec!["ada".into(), "grace".into()]));
    assert_eq!((first.repositories.len(), first.after.as_deref()), (50, Some("50")));
    assert_eq!((rest.repositories.len(), rest.after), (10, None));
    assert_eq!(rest.repositories[0].repository, named("many/r50"));
    assert_eq!(nowhere.code, ErrorCode::InvalidRequest);
    assert_eq!(repositories.queries(), 2, "a page each account, read once for every search within a minute");
}
