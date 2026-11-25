//! What GitHub does with the listener's pages, for tests: read the form the first posts to it, and send the person back
//! once they've made the app, with a code and the form's `state`.
#![expect(clippy::unwrap_used, reason = "only tests use it, so failing loudly is what it should do")]

use crate::loopback::browsing::{Fetched, fetch};

/// The form the listener's page posts to GitHub.
#[derive(Debug)]
pub struct Form {
    /// Where it's posted, with the `state` after it.
    pub action: String,
    pub manifest: serde_json::Value,
}

impl Form {
    /// The `state` it sends GitHub.
    pub fn state(&self) -> &str {
        self.action.split_once("?state=").unwrap().1
    }
}

/// The form `page` posts, as a browser reads it.
pub fn form(page: &str) -> Form {
    let quoted = |after: &str| unescaped(page.split_once(after).unwrap().1.split_once('"').unwrap().0);
    let action = quoted("action=\"");
    let manifest = serde_json::from_str(&quoted("name=\"manifest\" value=\"")).unwrap();
    Form { action, manifest }
}

/// What GitHub does once the person has made the app from the form the page at `page_url` posts: sends the browser
/// back where the manifest says, with `code` and the form's `state`. Answers with what the listener said to that.
pub fn come_back(page_url: &str, code: &str) -> Fetched {
    let form = form(&fetch(page_url).body);
    let back = form.manifest["redirect_url"].as_str().unwrap();
    fetch(&format!("{back}?code={code}&state={}", form.state()))
}

fn unescaped(text: &str) -> String {
    let replaced = [("&quot;", "\""), ("&lt;", "<"), ("&gt;", ">"), ("&#39;", "'"), ("&amp;", "&")];
    replaced.into_iter().fold(text.to_string(), |text, (escaped, was)| text.replace(escaped, was))
}
