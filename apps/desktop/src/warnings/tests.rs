use protocol::model::{GithubLoginKind, GithubRepositoryName, InstalledApp, RepositoryUnseen};

use super::{Answer, Warning};

const APPROVE_AT: &str = "https://github.com/settings/connections/applications/an-app";

/// `railworks/roundhouse` unseen as `ada`, whose login is held as `login`.
fn unseen(login: GithubLoginKind) -> RepositoryUnseen {
    RepositoryUnseen {
        repository: GithubRepositoryName::named("railworks/roundhouse"),
        username: "ada".into(),
        login,
        installation: None,
        said: "Could not resolve to a Repository with the name 'railworks/roundhouse'".into(),
        approve_at: (login == GithubLoginKind::SignIn).then(|| APPROVE_AT.to_string()),
    }
}

/// What's wrong names the repository and the account, then says what can't be shown, in GitHub's words ended as a
/// sentence, and what the account's kind of login can do: a sign-in's is approving Roundhouse, on its app's page, a
/// token's is being given the repository, and `gh`'s is logging in as someone else. Each can be given access, and
/// every warning opens Git.
#[test]
fn what_to_do_goes_by_how_the_default_s_login_is_held() {
    let signed_in = Warning::unseen(&unseen(GithubLoginKind::SignIn));
    let token = Warning::unseen(&unseen(GithubLoginKind::Token));
    let gh = Warning::unseen(&unseen(GithubLoginKind::Gh));

    assert_eq!(signed_in.wrong, "Roundhouse can't see railworks/roundhouse on GitHub as ada.");
    assert!(
        signed_in.then.starts_with(
            "So it can't show this workspace's pull request or its checks. GitHub says: Could not resolve to a \
             Repository with the name 'railworks/roundhouse'. If railworks limits which apps can reach it, approve \
             Roundhouse for railworks."
        ),
        "{}",
        signed_in.then
    );
    let approve = Answer::OnGithub { label: "Approve on GitHub", page: APPROVE_AT.to_string() };
    assert_eq!(signed_in.answers, [approve, Answer::OpenGit]);
    assert!(
        token.then.ends_with(
            "Give the token access to it, ask railworks to give ada access, or connect an account that can see it."
        ),
        "{}",
        token.then
    );
    assert!(gh.then.contains("Log gh in as someone who can see it"), "{}", gh.then);
    for warning in [&token, &gh] {
        assert_eq!(warning.answers, [Answer::OpenGit], "nothing to approve but a sign-in's app");
    }
}

/// An app of your own's bot reaches only what it's given where it's installed: on the repository's owner, the app
/// is given the repository there, and on another owner, it can't be, so only another account will do.
#[test]
fn an_app_of_your_own_is_given_the_repository_where_it_s_installed() {
    let installed_on = |on: &str| RepositoryUnseen {
        installation: Some(InstalledApp { slug: "railworks-roundhouse".into(), on: on.into() }),
        ..unseen(GithubLoginKind::App)
    };

    let on_owner = Warning::unseen(&installed_on("Railworks"));
    let elsewhere = Warning::unseen(&installed_on("ada"));

    let install = "https://github.com/apps/railworks-roundhouse/installations/new".to_string();
    assert_eq!(on_owner.answers, [Answer::OnGithub { label: "Install on GitHub", page: install }, Answer::OpenGit]);
    assert!(
        on_owner.then.ends_with(
            "Install the app on railworks with access to railworks/roundhouse, or connect an account that can see it."
        ),
        "{}",
        on_owner.then
    );
    assert!(elsewhere.then.contains("Its app is installed only on ada"), "{}", elsewhere.then);
    assert_eq!(elsewhere.answers, [Answer::OpenGit]);
}
