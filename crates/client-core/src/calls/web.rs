//! Reading the web: fetching a page to ask something of it, and searching. Claude answers a fetch with what a model
//! made of the page, and a search with the pages it found, as JSON on a `Links:` line, and then what it made of them.
//! A typed search says the pages it found apart from what it made of them, or, as Antigravity's does, says only words.

use protocol::capability::calls::CallOutput;
use serde_json::Value;

use super::{count, str_field};

pub struct Fetch<'a> {
    pub url: &'a str,
    /// What the agent asked of the page.
    pub prompt: Option<&'a str>,
}

impl<'a> Fetch<'a> {
    /// What its line says it did.
    pub const TITLE: &'static str = "Fetch";

    pub(super) fn read(input: &'a Value) -> Option<Self> {
        Some(Self { url: str_field(input, "url")?, prompt: str_field(input, "prompt") })
    }
}

pub struct WebSearch<'a> {
    pub query: &'a str,
}

impl<'a> WebSearch<'a> {
    /// What its line says it did.
    pub const TITLE: &'static str = "Search the web";

    pub(super) fn read(input: &'a Value) -> Option<Self> {
        Some(Self { query: str_field(input, "query")? })
    }
}

/// The site a page is on, without its scheme or a leading `www.`, which is enough to tell pages apart on a line.
pub fn site(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    host.trim_prefix("www.")
}

#[derive(Debug, PartialEq, Eq)]
pub struct Link {
    pub title: String,
    pub url: String,
}

/// What a web search sent back.
#[derive(Debug, PartialEq, Eq)]
pub enum Results<'a> {
    /// The pages it found, as Claude's search says them, and what was made of them.
    Pages { links: Vec<Link>, findings: &'a str },
    /// What it found, in words alone, as Antigravity's search says it, which can name pages among them.
    Words(&'a str),
}

impl<'a> Results<'a> {
    /// What a web search sent back: typed, as `output` says it, or, for one recorded as Claude's words, as its `text`
    /// says it.
    pub fn of(text: &'a str, output: Option<&'a CallOutput>) -> Self {
        let Some(output) = output else { return Self::read(text) };
        match output {
            CallOutput::WebResults { results, summary } => Results::Pages {
                links: results.iter().map(|page| Link { title: page.title.clone(), url: page.url.clone() }).collect(),
                findings: summary.as_deref().map_or("", str::trim),
            },
            // Words, as Antigravity's search says, or what this build doesn't know.
            CallOutput::Command(_)
            | CallOutput::Lines(_)
            | CallOutput::Image { .. }
            | CallOutput::Document { .. }
            | CallOutput::Changed { .. }
            | CallOutput::Found(_)
            | CallOutput::Paths(_)
            | CallOutput::Page { .. }
            | CallOutput::Report { .. }
            | CallOutput::Mcp { .. }
            | CallOutput::Words { .. }
            | CallOutput::Encrypted
            | CallOutput::Backgrounded { .. }
            | CallOutput::Unknown => Results::Words(text.trim()),
        }
    }

    /// What a web search sent back, in words, as Claude's says it.
    fn read(text: &'a str) -> Self {
        let (mut links, mut after_links, mut at) = (Vec::new(), None, 0);
        for line in text.split_inclusive('\n') {
            at += line.len();
            if let Some(found) = line.trim_end().strip_prefix("Links: ").and_then(read_links) {
                links.extend(found);
                after_links = Some(at);
            }
        }
        // Without links, the first line still only says what was searched for, which the call shows already.
        let summary = match after_links {
            Some(at) => &text[at..],
            None if text.starts_with("Web search results") => text.split_once('\n').map_or("", |(_, rest)| rest),
            None => text,
        };
        // Claude ends with a note to itself to cite the links, which isn't for a person.
        let summary = summary.rfind("\nREMINDER:").map_or(summary, |at| &summary[..at]);
        Results::Pages { links, findings: summary.trim() }
    }

    /// The pages it found, where it says which.
    pub fn links(&self) -> &[Link] {
        match self {
            Results::Pages { links, .. } => links,
            Results::Words(_) => &[],
        }
    }

    /// What was made of the pages, or, for a search that says only words, all it said.
    pub fn findings(&self) -> &'a str {
        match self {
            Results::Pages { findings, .. } => findings,
            Results::Words(words) => words,
        }
    }

    /// How many pages it found, in a few words, where it says which.
    pub fn summary(&self) -> Option<String> {
        let Results::Pages { links, .. } = self else { return None };
        Some(match links.len() {
            0 => "Nothing found".to_string(),
            n => count(n, "result", "results"),
        })
    }
}

fn read_links(json: &str) -> Option<Vec<Link>> {
    let links: Vec<Value> = serde_json::from_str(json).ok()?;
    let link = |link: &Value| {
        let url = str_field(link, "url")?.to_string();
        Some(Link { title: str_field(link, "title").map_or_else(|| url.clone(), str::to_string), url })
    };
    Some(links.iter().filter_map(link).collect())
}

#[cfg(test)]
mod tests {
    use protocol::capability::calls::CallOutput;
    use protocol::capability::calls::output::WebResult;

    use super::super::typed;
    use super::{Link, Results, site};

    /// Whether what a typed web search found, read from what came back for it as a client is given it, is `expected`,
    /// and says `summary` on its line.
    fn found(output: &CallOutput, expected: &Results, summary: Option<&str>) {
        let text = typed::output(output);
        let results = Results::of(&text, Some(output));
        assert_eq!(&results, expected, "{output:?}");
        assert_eq!(results.summary().as_deref(), summary);
    }

    #[test]
    fn a_typed_web_search_says_its_pages_apart_from_what_it_made_of_them() {
        let page =
            WebResult { title: "Railway signal".into(), url: "https://example.org/signal".into(), snippet: None };
        let link = Link { title: "Railway signal".into(), url: "https://example.org/signal".into() };
        let pages = CallOutput::WebResults { results: vec![page], summary: Some("A signal shows stop.\n".into()) };

        found(&pages, &Results::Pages { links: vec![link], findings: "A signal shows stop." }, Some("1 result"));
        let nothing = CallOutput::WebResults { results: Vec::new(), summary: None };
        found(&nothing, &Results::Pages { links: Vec::new(), findings: "" }, Some("Nothing found"));
    }

    /// Antigravity's search says what it found in words alone, which may name pages, so it says nothing of how many.
    #[test]
    fn a_web_search_said_in_words_says_no_count() {
        let words = CallOutput::Words { text: "Signals show stop, caution or clear.\n".into() };

        found(&words, &Results::Words("Signals show stop, caution or clear."), None);
    }

    #[test]
    fn a_search_s_links_and_what_it_made_of_them_are_read_apart() {
        let text = "Web search results for query: \"signal aspects\"\n\n\
                    Links: [{\"title\":\"Railway signal\",\"url\":\"https://en.wikipedia.org/wiki/Railway_signal\"}]\n\n\
                    A signal shows clear, caution or stop.\n\n\nREMINDER: You MUST include the sources above.";

        let results = Results::read(text);

        assert_eq!(
            results.links(),
            [Link { title: "Railway signal".into(), url: "https://en.wikipedia.org/wiki/Railway_signal".into() }]
        );
        assert_eq!(results.findings(), "A signal shows clear, caution or stop.");
        assert_eq!(results.summary().as_deref(), Some("1 result"));
    }

    #[test]
    fn a_search_without_links_keeps_what_it_said() {
        let results = Results::read("Web search results for query: \"x\"\n\nNo results.");

        assert_eq!(results, Results::Pages { links: Vec::new(), findings: "No results." });
        assert_eq!(results.summary().as_deref(), Some("Nothing found"));
    }

    #[test]
    fn a_page_is_known_by_its_site() {
        assert_eq!(site("https://www.example.org/docs/a?b=1"), "example.org");
        assert_eq!(site("docs.rs/signals"), "docs.rs");
    }
}
