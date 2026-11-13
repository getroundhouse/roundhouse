//! Reading the web: fetching a page to ask something of it, and searching. Claude answers a fetch with what a model
//! made of the page, and a search with the pages it found, as JSON on a `Links:` line, and then what it made of them.

use serde_json::Value;

use super::{count, str_field};

pub struct Fetch<'a> {
    pub url: &'a str,
    /// What the agent asked of the page.
    pub prompt: Option<&'a str>,
}

impl<'a> Fetch<'a> {
    pub(super) fn read(input: &'a Value) -> Option<Self> {
        Some(Self { url: str_field(input, "url")?, prompt: str_field(input, "prompt") })
    }
}

pub struct WebSearch<'a> {
    pub query: &'a str,
}

impl<'a> WebSearch<'a> {
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
pub struct Results<'a> {
    pub links: Vec<Link>,
    /// What was made of the pages, after the links.
    pub summary: &'a str,
}

impl<'a> Results<'a> {
    pub fn read(text: &'a str) -> Self {
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
        Self { links, summary: summary.trim() }
    }

    /// How many pages it found, in a few words.
    pub fn summary(&self) -> String {
        match self.links.len() {
            0 => "Nothing found".to_string(),
            n => count(n, "result", "results"),
        }
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
    use super::{Link, Results, site};

    #[test]
    fn a_search_s_links_and_what_it_made_of_them_are_read_apart() {
        let text = "Web search results for query: \"signal aspects\"\n\n\
                    Links: [{\"title\":\"Railway signal\",\"url\":\"https://en.wikipedia.org/wiki/Railway_signal\"}]\n\n\
                    A signal shows clear, caution or stop.\n\n\nREMINDER: You MUST include the sources above.";

        let results = Results::read(text);

        assert_eq!(
            results.links,
            [Link { title: "Railway signal".into(), url: "https://en.wikipedia.org/wiki/Railway_signal".into() }]
        );
        assert_eq!(results.summary, "A signal shows clear, caution or stop.");
        assert_eq!(results.summary(), "1 result");
    }

    #[test]
    fn a_search_without_links_keeps_what_it_said() {
        let results = Results::read("Web search results for query: \"x\"\n\nNo results.");

        assert!(results.links.is_empty());
        assert_eq!(results.summary, "No results.");
        assert_eq!(results.summary(), "Nothing found");
    }

    #[test]
    fn a_page_is_known_by_its_site() {
        assert_eq!(site("https://www.example.org/docs/a?b=1"), "example.org");
        assert_eq!(site("docs.rs/signals"), "docs.rs");
    }
}
