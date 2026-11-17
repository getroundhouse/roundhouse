//! Codex's `webSearch` item: a search of the web, a page opened, or a look for words in one. Its start says none of
//! that, with an empty query and no action, so the call is recorded once the item completes, saying what it did.

use protocol::capability::calls::{
    Call, CallOutput,
    call::{Fetch, FindInPage, WebSearch},
    output::WebResult,
};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
pub(super) struct Searched {
    pub(super) id: String,
    #[serde(default)]
    query: String,
    #[serde(default)]
    action: Option<Action>,
    #[serde(default)]
    results: Option<Vec<Found>>,
}

/// What the search did, as its completed item says.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Action {
    Search {
        #[serde(default)]
        query: Option<String>,
    },
    OpenPage {
        #[serde(default)]
        url: Option<String>,
    },
    /// A look for `pattern` in a page, which names none when it's the page opened last.
    FindInPage {
        #[serde(default)]
        url: Option<String>,
        #[serde(default)]
        pattern: Option<String>,
    },
    #[serde(other)]
    Other,
}

/// One of what it found, which Codex sends as `text_result`s.
#[derive(Deserialize)]
struct Found {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    snippet: Option<String>,
}

impl Searched {
    /// The call, as the completed item says it. A look in the page opened last names no page, so its URL is empty,
    /// and an action this build doesn't know is any other tool's, with the query it gave.
    pub(super) fn call(&self) -> Call {
        match &self.action {
            Some(Action::Search { query }) => Call::WebSearch(WebSearch {
                query: query.clone().unwrap_or_else(|| self.query.clone()),
                only_domains: Vec::new(),
                not_domains: Vec::new(),
            }),
            Some(Action::OpenPage { url }) => Call::Fetch(Fetch { url: url.clone().unwrap_or_default(), asking: None }),
            Some(Action::FindInPage { url, pattern }) => Call::FindInPage(FindInPage {
                url: url.clone().unwrap_or_default(),
                pattern: pattern.clone().unwrap_or_default(),
            }),
            Some(Action::Other) | None => {
                Call::Other { tool: "webSearch".into(), input: json!({ "query": self.query }) }
            }
        }
    }

    /// What came back: the pages it found, or opened, with what each said.
    pub(super) fn output(self) -> CallOutput {
        let results = self
            .results
            .unwrap_or_default()
            .into_iter()
            .map(|found| WebResult { title: found.title, url: found.url, snippet: found.snippet })
            .collect();
        CallOutput::WebResults { results, summary: None }
    }
}
