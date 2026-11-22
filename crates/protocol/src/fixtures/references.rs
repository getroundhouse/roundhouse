//! Synthetic issues a chat's chips name, for the tests of the crates that draw them.

use crate::model::{About, GithubRepositoryName, Issue, IssueNumber, IssueState, IssueSummary, Read, Reference};

/// Open issue `number` of `repository`, written as `owner/name`, titled `title`, read at 0, with nothing more to it.
#[expect(clippy::unwrap_used, reason = "the synthetic numbers a test gives are valid")]
pub fn issue(repository: &'static str, number: u64, title: &str) -> Reference {
    let about = About {
        url: format!("https://github.com/{repository}/issues/{number}"),
        title: title.to_string(),
        author: None,
        opened_at_ms: 0,
        comments: 0,
        labels: Vec::new(),
        body: String::new(),
    };
    Reference::Issue(Issue {
        repository: GithubRepositoryName::named(repository),
        number: IssueNumber::try_from(number).unwrap(),
        summary: Read { said: IssueSummary { about, state: IssueState::Open }, read_at_ms: 0 },
    })
}
