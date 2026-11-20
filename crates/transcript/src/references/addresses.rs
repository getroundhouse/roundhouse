//! Where a text links to an issue or a pull request: its address on github.com, such as
//! `https://github.com/owner/name/pull/12`, which names its repository and its number, so nothing's guessed.

use protocol::model::{GithubRepositoryName, IssueNumber, ReferenceAsk};

/// What starts an address on github.com.
const GITHUB: [&str; 2] = ["https://github.com/", "http://github.com/"];

/// Whether `source` may link to an issue or a pull request, which it can only if it has an address on github.com.
pub(crate) fn may_link(source: &str) -> bool {
    GITHUB.iter().any(|github| source.contains(github))
}

/// What an address on github.com of an issue or a pull request names, such as
/// `https://github.com/owner/name/issues/12`, with or without a comment's anchor after it, or `None` for any other
/// address.
pub(crate) fn addressed(url: &str) -> Option<ReferenceAsk> {
    let path = GITHUB.iter().find_map(|github| url.strip_prefix(github))?;
    let path = path.split(['#', '?']).next()?.trim_end_matches('/');
    let mut parts = path.split('/');
    let (owner, name, kind, number) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || !matches!(kind, "issues" | "pull") {
        return None;
    }
    let number = IssueNumber::try_from(number.parse::<u64>().ok()?).ok()?;
    Some(ReferenceAsk { repository: GithubRepositoryName::new(owner, name).ok()?, number })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(ask: &ReferenceAsk) -> (String, u64) {
        (ask.repository.as_str().to_string(), ask.number.get())
    }

    #[test]
    fn an_address_on_github_names_its_issue_or_pull_request() {
        assert_eq!(
            addressed("https://github.com/o/r/pull/12#issuecomment-1").as_ref().map(named),
            Some(("o/r".into(), 12))
        );
        assert_eq!(addressed("https://github.com/o/r/issues/7/").as_ref().map(named), Some(("o/r".into(), 7)));
        for url in [
            "https://github.com/o/r",
            "https://github.com/o/r/blob/main/a.rs",
            "https://github.com/o/r/pull/12/files",
            "https://github.com/o/r/pull/0",
            "https://example.com/o/r/pull/1",
        ] {
            assert_eq!(addressed(url), None, "{url}");
        }
    }

    #[test]
    fn only_a_text_with_an_address_on_github_may_link_to_one() {
        assert!(may_link("See [#12](https://github.com/o/r/pull/12)."));
        assert!(!may_link("Fixed in #12."));
    }
}
