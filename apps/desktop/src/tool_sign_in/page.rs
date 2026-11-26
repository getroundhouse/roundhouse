//! The page a person approves a sign-in on, as the hub gives it, checked before the browser is opened on it, since a
//! server anyone can name leads to it.

use protocol::client::tool_sign_ins::ToolSignInPage;
use url::Url;

use crate::loopback::Query;

protocol::tokens!(
    /// What's in the page's address that the person is sent back with, which says the redirect is this sign-in's.
    State,
);

/// A page a browser can be opened on, which says what the person is sent back with.
pub(super) struct SignInPage {
    url: Url,
    host: String,
    state: State,
}

impl SignInPage {
    /// `page`, where it's on the web, with no login in its address, and has one `state` in it. A page with none would
    /// have any redirect taken for this sign-in's.
    pub(super) fn checked(page: &ToolSignInPage) -> Option<SignInPage> {
        let url = Url::parse(page.as_str()).ok()?;
        let on_the_web = matches!(url.scheme(), "http" | "https");
        let bare = url.username().is_empty() && url.password().is_none();
        let host = url.host_str()?.to_string();
        let state = Query::new(url.query()?).value("state").filter(|state| !state.is_empty())?;
        (on_the_web && bare).then(|| SignInPage { url, host, state: State::new(state) })
    }

    /// The host it's on, to say where the person approves it.
    pub(super) fn host(&self) -> &str {
        &self.host
    }

    pub(super) fn state(&self) -> &State {
        &self.state
    }

    /// Its address, for the browser and nothing else.
    pub(super) fn url(&self) -> &str {
        self.url.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_is_opened_only_on_the_web_with_no_login_and_one_state() {
        let checked = |page: &str| SignInPage::checked(&ToolSignInPage::new(page));

        let page = checked("https://auth.example.com/authorize?state=a%2Bb&scope=read").unwrap();
        assert_eq!((page.host(), page.state().as_str()), ("auth.example.com", "a+b"));
        assert_eq!(page.url(), "https://auth.example.com/authorize?state=a%2Bb&scope=read");
        assert!(checked("http://127.0.0.1:5000/authorize?state=s").is_some(), "a server of the person's own");
        for refused in [
            "file:///Applications/Example.app?state=s",
            "javascript:alert(1)?state=s",
            "example:open?state=s",
            "example://open/page?state=s",
            "https://ada:secret@auth.example.com/authorize?state=s",
            "https://auth.example.com/authorize",
            "https://auth.example.com/authorize?state=",
            "https://auth.example.com/authorize?state=s&state=t",
            "not a url",
        ] {
            assert!(checked(refused).is_none(), "{refused}");
        }
    }
}
