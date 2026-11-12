//! Where the needle is in Markdown, as the chat shows it: in the text the kit renders of it, so `**parser** is` holds
//! `parser is`, and a link's address holds nothing. That's read leaf by leaf, from the kit's own parse as
//! `crate::prose` keys it, so a match never runs from one leaf into the next. Counting parses a text again rather than
//! asking `crate::prose` for it, since it runs on another thread too.

use std::ops::Range;

use protocol::find::Needle;

use crate::prose::{ATOM, leaves};

/// How many matches `source` shows, rendered.
pub(crate) fn count(needle: &Needle, source: &str) -> usize {
    if !needle.may_be_in_markdown(source) {
        return 0;
    }
    leaves(source).iter().map(|leaf| needle.ranges(leaf).count()).sum()
}

/// Where the needle is in `rendered`, the text the kit's text view renders of `source`, in order. The view renders an
/// image as nothing, where a leaf has a character for it, but a needle that would run across one isn't in `source`,
/// whose image has its address between the words, so this finds what counting does.
pub(crate) fn ranges(needle: &Needle, source: &str, rendered: &str) -> Vec<Range<usize>> {
    if !needle.may_be_in_markdown(source) {
        return Vec::new();
    }
    let mut found = Vec::new();
    let mut from = 0;
    for text in leaves(source).iter().map(|leaf| leaf.replace(ATOM, "")).filter(|text| !text.is_empty()) {
        let Some(at) = rendered.get(from..).and_then(|rest| rest.find(text.as_str())) else { continue };
        let start = from + at;
        found.extend(needle.ranges(&text).map(|range| start + range.start..start + range.end));
        from = start + text.len();
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn needle(query: &str) -> Needle {
        Needle::new(query).unwrap()
    }

    #[test]
    fn markdown_is_found_as_it_renders() {
        assert_eq!(count(&needle("parser is"), "The **parser** is _fixed_"), 1);
        assert_eq!(count(&needle("run cargo"), "Run `cargo test`"), 1);
        assert_eq!(count(&needle("example.com"), "See [the docs](https://example.com)"), 0, "not a link's address");
        assert_eq!(count(&needle("**"), "**bold**"), 0, "nor what marks it");
        assert_eq!(count(&needle("cell"), "| a cell | another cell |\n|---|---|\n| cell | x |"), 3);
        assert_eq!(count(&needle("item"), "- an item\n- another item\n\n```\nitem\n```"), 3);
    }

    #[test]
    fn a_view_s_matches_are_those_counted_where_it_renders_them() {
        let source = "# The parser\n\nThe **parser** is _fixed_.";
        let rendered = "The parser\nThe parser is fixed.\n";

        let found = ranges(&needle("parser"), source, rendered);

        let texts: Vec<&str> = found.iter().map(|range| &rendered[range.clone()]).collect();
        assert_eq!(texts, ["parser", "parser"]);
        assert_eq!(found[1].start, "The parser\nThe ".len(), "in the second block");
        assert_eq!(found.len(), count(&needle("parser"), source));
    }

    #[test]
    fn a_needle_across_an_image_is_found_neither_way() {
        let (source, rendered) = ("before ![a picture](a.png)after", "before after");

        assert_eq!(count(&needle("before after"), source), 0);
        assert!(ranges(&needle("before after"), source, rendered).is_empty());
    }
}
