//! What the hub asks GitHub of repositories, and a read against a stand-in answering from the repositories a test
//! sets.

use protocol::client::ErrorCode;
use protocol::model::{GithubRepositoryName, GithubRepositorySaid, GithubToken, RepoUrl};
use serde_json::json;

use super::query::query;
use super::read::taken;
use super::{RepositoryAsk, RepositoryFound};
use crate::{Github, GithubConfig, Leaving, Repositories, Script, StandIn};

fn named(repository: &str) -> RepositoryAsk {
    RepositoryAsk::Named(GithubRepositoryName::try_from(repository.to_owned()).unwrap())
}

/// The repository `id`, as GitHub says of it while it's called `repository`.
fn found(id: &str, repository: &str) -> RepositoryFound {
    let address = |url: String| RepoUrl::try_from(url).unwrap();
    let said = GithubRepositorySaid {
        repository: GithubRepositoryName::try_from(repository.to_owned()).unwrap(),
        https: address(format!("https://github.com/{repository}")),
        ssh: address(format!("git@github.com:{repository}.git")),
    };
    RepositoryFound { id: id.into(), said }
}

/// One by its owner and name, and one by its ID, as one query asks for them, whole, so a change to what's asked is
/// seen here.
#[test]
fn a_query_asks_for_a_repository_by_its_name_or_its_id_and_whats_left_of_githubs_limit() {
    let asks = [named("example/project"), RepositoryAsk::Id("R_kgDO\"1".into())];

    let asked = query(&asks);

    assert_eq!(
        asked,
        "query { g0: repository(owner:\"example\",name:\"project\") { id nameWithOwner url sshUrl } \
         g1: node(id:\"R_kgDO\\\"1\") { ... on Repository { id nameWithOwner url sshUrl } } \
         rateLimit { limit remaining resetAt } }"
    );
}

/// An error about one ask that isn't GitHub having none fails it alone, and one about the query itself fails them all.
/// An ID that's of something other than a repository is none.
#[test]
fn an_error_fails_the_ask_it_names_or_every_ask_when_it_names_none() {
    let not_a_repository = json!({});
    let data = json!({"g0": null, "g1": not_a_repository});
    let about_one = json!({"data": data, "errors": [{"type": "FORBIDDEN", "path": ["g0"], "message": "SAML"}]});
    let about_all = json!({"data": null, "errors": [{"type": "MAX_NODE_LIMIT_EXCEEDED", "message": "too many"}]});

    let one = taken(&about_one, 2);
    let all = taken(&about_all, 2);

    assert_eq!(one[0].as_ref().unwrap_err().code, ErrorCode::Unavailable);
    assert_eq!(one[1], Ok(None));
    assert!(all.iter().all(|answer| answer.as_ref().is_err_and(|e| e.code == ErrorCode::Worker)), "{all:?}");
}

/// A repository found by its name, whatever its case, then by its ID once it's renamed, and by its new name, while its
/// old name, one that isn't there and one that's hidden find none. More than a query carries take two, and once GitHub
/// fails, every ask does.
#[tokio::test]
async fn a_read_finds_a_repository_by_its_name_and_after_a_rename_by_its_id() {
    let repositories = Repositories::default();
    repositories.set("R_project", "example/project");
    repositories.set("R_private", "example/private");
    repositories.hide("R_private", true);
    let script = Script { repositories: Some(repositories.clone()), ..Script::default() };
    let github = StandIn::with(script).await;
    let calls = Github::new(GithubConfig::at(github.api()));
    let token = GithubToken::new("a-token");

    let before =
        calls.read_repositories(&token, &[named("Example/Project"), named("example/gone")], Leaving::ForOthers).await;
    repositories.set("R_project", "example/renamed");
    let asks = [
        RepositoryAsk::Id("R_project".into()),
        named("example/renamed"),
        named("example/project"),
        named("example/private"),
        RepositoryAsk::Id("R_private".into()),
    ];
    let after = calls.read_repositories(&token, &asks, Leaving::ForOthers).await;
    let many: Vec<_> = (0..101).map(|_| RepositoryAsk::Id("R_project".into())).collect();
    let paged = calls.read_repositories(&token, &many, Leaving::ForOthers).await;
    repositories.fail(true);
    let failed = calls.read_repositories(&token, &asks[..1], Leaving::ForOthers).await;

    assert_eq!(before.answers, [Ok(Some(found("R_project", "example/project"))), Ok(None)]);
    let renamed = Some(found("R_project", "example/renamed"));
    assert_eq!(after.answers, [Ok(renamed.clone()), Ok(renamed), Ok(None), Ok(None), Ok(None)]);
    assert_eq!((after.rate_limit.unwrap().remaining, after.refused), (4900, false));
    assert!(paged.answers.iter().all(|answer| answer.as_ref().is_ok_and(Option::is_some)));
    assert_eq!(repositories.queries(), 2 + 2 + 1, "a hundred to a query");
    assert_eq!(failed.answers[0].as_ref().unwrap_err().code, ErrorCode::Worker);
}

/// Each token's user sees the repositories GitHub shows them, a page at a time, by owner and name, whatever their case,
/// with whether each is private, and a page's end says where the next starts.
#[tokio::test]
async fn the_repositories_a_token_can_see_are_listed_a_page_at_a_time() {
    let repositories = Repositories::default();
    for at in 0..101 {
        repositories.set(&format!("R_{at:03}"), &format!("example/r{at:03}"));
    }
    repositories.set("R_mine", "ada/Mine");
    repositories.show_only_to("R_mine", "a-token");
    repositories.hide("R_000", true);
    let github = StandIn::with(Script { repositories: Some(repositories), ..Script::default() }).await;
    let calls = Github::new(GithubConfig::at(github.api()));
    let (ada, grace) = (GithubToken::new("a-token"), GithubToken::new("g-token"));

    let first = calls.repositories_seen(&ada, None).await.unwrap();
    let second = calls.repositories_seen(&ada, first.next.as_deref()).await.unwrap();
    let graces = calls.repositories_seen(&grace, None).await.unwrap();

    let named = |page: &super::SeenPage| -> Vec<String> {
        page.repositories.iter().map(|seen| seen.found.said.repository.to_string()).collect()
    };
    assert_eq!(first.repositories.len(), 100);
    assert_eq!(first.repositories[0].found, found("R_mine", "ada/Mine"));
    assert!(first.repositories[0].private && !first.repositories[1].private);
    assert_eq!((named(&second), second.next), (vec!["example/r100".to_owned()], None));
    assert!(!named(&graces).contains(&"ada/Mine".to_owned()), "only its owner sees it");
    assert!(github.asked().iter().any(|asked| asked.contains("ownerAffiliations")));
}
