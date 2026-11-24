//! What each warning says, and what it offers to do.

use protocol::model::{GithubLoginKind, InstalledApp, RepositoryUnseen};

use crate::github_app_pages::{install_page, installed_on_owner};

/// One of a workspace's warnings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Warning {
    /// What's wrong, in a warning's colour.
    pub(crate) wrong: String,
    /// Why, and what to do, quieter, under it.
    pub(crate) then: String,
    /// The buttons under it, in order.
    pub(crate) answers: Vec<Answer>,
}

/// A button under a warning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Answer {
    /// Opens a page on GitHub, where what the warning says to do is done, labelled `label`.
    OnGithub { label: &'static str, page: String },
    /// Opens Git in Settings, where another default is chosen and accounts are connected.
    OpenGit,
}

impl Warning {
    /// GitHub showing the default no repository where the workspace's branch's pull requests are, as `unseen` says,
    /// and what the default's kind of login can do about it: a sign-in's organization approving Roundhouse, which is
    /// done on the page `approve_at` names, a token being given the repository, an app of your own being installed
    /// where it is, or `gh` logging in as someone who can see it, or any of them being given access by the
    /// repository's owner, or another account connected.
    pub(crate) fn unseen(unseen: &RepositoryUnseen) -> Self {
        let RepositoryUnseen { repository, username, login, installation, said, approve_at } = unseen;
        let owner = unseen.owner();
        let wrong = format!("Roundhouse can't see {repository} on GitHub as {username}.");
        let ask = format!("ask {owner} to give {username} access");
        let connect = "connect an account that can see it";
        let (to_do, answer) = match (login, installation) {
            (_, Some(InstalledApp { slug, on })) if installed_on_owner(repository, on) => {
                let to_do = format!("Install the app on {owner} with access to {repository}, or {connect}.");
                (to_do, Some(Answer::OnGithub { label: "Install on GitHub", page: install_page(Some(slug)) }))
            }
            (_, Some(InstalledApp { on, .. })) => {
                let to_do = format!(
                    "Its app is installed only on {on}, so it can't see {owner}'s private repositories. {}.",
                    capitalized(connect)
                );
                (to_do, None)
            }
            (GithubLoginKind::SignIn, None) => {
                let to_do = format!(
                    "If {owner} limits which apps can reach it, approve Roundhouse for {owner}. Otherwise, {ask}, or \
                     {connect}."
                );
                (to_do, approve_at.clone().map(|page| Answer::OnGithub { label: "Approve on GitHub", page }))
            }
            (GithubLoginKind::Token, None) => (format!("Give the token access to it, {ask}, or {connect}."), None),
            (GithubLoginKind::Gh, None) => (format!("Log gh in as someone who can see it, {ask}, or {connect}."), None),
            (GithubLoginKind::App | GithubLoginKind::Unknown, None) => {
                (format!("{}, or {connect}.", capitalized(&ask)), None)
            }
        };
        let then = format!(
            "So it can't show this workspace's pull request or its checks. GitHub says: {} {to_do}",
            sentence(said)
        );
        Self { wrong, then, answers: answer.into_iter().chain([Answer::OpenGit]).collect() }
    }
}

/// `words` ending as a sentence does, so what follows them starts one of its own.
pub(crate) fn sentence(words: &str) -> String {
    let words = words.trim();
    if words.ends_with(['.', '!', '?', '…']) { words.to_string() } else { format!("{words}.") }
}

/// `words` starting as a sentence does.
fn capitalized(words: &str) -> String {
    let mut letters = words.chars();
    letters.next().map_or_default(|first| first.to_uppercase().chain(letters).collect())
}
