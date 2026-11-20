//! A text's references as the kit parses it: each link to an issue or a pull request on github.com, whatever its
//! words, made a node, which the kit keeps that link, so a click opens it, and draws as the link while it's looked up.

use std::collections::HashSet;

use gpui_kit::base::text::editor_host::InlineNode;
use gpui_kit::component::text::markdown_ast::{Link, Node};
use gpui_kit::component::text::{MarkdownExtensions, MarkdownNode, MarkdownParseContext, MarkdownPlugin};
use parking_lot::Mutex;
use protocol::model::{IssueNumber, ReferenceAsk};

use super::addresses::addressed;

/// The name the kit knows a reference's node by.
pub(super) const REFERENCE: &str = "reference";

/// Whether `node` is a link to an issue or a pull request, whose text is the link's words.
pub(crate) fn named(node: &InlineNode) -> bool {
    node.as_custom().is_some_and(|custom| custom.name() == REFERENCE)
}

/// One place a text links to an issue or a pull request.
#[derive(Clone, Debug)]
pub(super) struct Named {
    pub(super) ask: ReferenceAsk,
    /// Whether it's the first time its text links to it, which shows its title.
    pub(super) first: bool,
}

/// What the parse of a text takes, as well as `extensions`. It's made for each parse, which is always of a text from
/// its start, since only the end of the chat parses a text again from near its end, and it draws no chips.
pub(crate) fn reading(extensions: MarkdownExtensions) -> MarkdownExtensions {
    extensions.plugin(ReadsReferences { named: Mutex::default() })
}

struct ReadsReferences {
    /// What the text has linked to so far, as the kit reads it from its start: each repository, as GitHub's names
    /// ignore case, in lower case, and its number.
    named: Mutex<HashSet<(String, IssueNumber)>>,
}

impl MarkdownPlugin for ReadsReferences {
    fn name(&self) -> &str {
        REFERENCE
    }

    /// A link to an issue or a pull request, whose text is the link's words as the kit draws them, each run of white
    /// space one space, or its address if it has none, such as one whose words are an image.
    fn parse(&self, node: &Node, _: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let Node::Link(Link { url, .. }) = node else { return None };
        let ask = addressed(url)?;
        let key = (ask.repository.as_str().to_ascii_lowercase(), ask.number);
        let first = self.named.lock().insert(key);
        let words = node.to_string().split_whitespace().collect::<Vec<_>>().join(" ");
        let text = if words.is_empty() { url.clone() } else { words };
        Some(MarkdownNode::new(REFERENCE, Named { ask, first }).text(text))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gpui_kit::base::text::editor_host::{BlockNode, parse_markdown};

    use super::*;

    /// Each reference `source` links to: its words, what it's in and its number, and whether it's the first time.
    fn named_in(source: &str) -> Vec<(String, String, bool)> {
        let blocks = parse_markdown(source, Arc::new(reading(MarkdownExtensions::default()))).expect("parsed");
        let mut found = Vec::new();
        for block in &blocks {
            let BlockNode::Paragraph(paragraph) = block else { continue };
            for node in paragraph.children().iter().filter(|node| named(node)) {
                let named = node.as_custom().and_then(MarkdownNode::data::<Named>).expect("a reference's node");
                let ask = format!("{}#{}", named.ask.repository.as_str(), named.ask.number);
                found.push((node.text().to_string(), ask, named.first));
            }
        }
        found
    }

    fn linked(words: &str, ask: &str, first: bool) -> (String, String, bool) {
        (words.to_string(), ask.to_string(), first)
    }

    #[test]
    fn a_link_to_an_issue_or_a_pull_request_is_one_whatever_its_words() {
        let source = "See [#12](https://github.com/o/r/pull/12), [the\nfix](https://github.com/o/r/pull/12), \
                      [`#3`](https://github.com/o/r/issues/3), [**#3**](https://github.com/O/R/issues/3), \
                      [![A badge](badge.png)](https://github.com/o/r/pull/5) and <https://github.com/a/b/pull/4>.";
        assert_eq!(
            named_in(source),
            [
                linked("#12", "o/r#12", true),
                linked("the fix", "o/r#12", false),
                linked("#3", "o/r#3", true),
                linked("#3", "O/R#3", false),
                linked("https://github.com/o/r/pull/5", "o/r#5", true),
                linked("https://github.com/a/b/pull/4", "a/b#4", true),
            ]
        );
    }

    #[test]
    fn a_number_alone_or_a_link_elsewhere_is_not_one() {
        for source in [
            "Fixed in #12 and o/r#41.",
            "[see #4](https://example.com)",
            "[the fix](https://github.com/o/r/pull/3/files)",
            "Run `open https://github.com/o/r/pull/3`",
        ] {
            assert_eq!(named_in(source), [], "{source}");
        }
    }
}
