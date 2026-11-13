//! How many times the needle is in Markdown, as the chat shows it: in the text the kit renders of it, so `**parser** is` holds
//! `parser is`, and a link's address holds nothing. That's read leaf by leaf, from the kit's own parse as
//! `crate::prose` keys it, so a match never runs from one leaf into the next. Counting parses a text again rather than
//! asking `crate::prose` for it, since it runs on another thread too.

use protocol::find::Needle;

use crate::prose::leaves;

/// How many matches `source` shows, rendered.
pub(crate) fn count(needle: &Needle, source: &str) -> usize {
    if !needle.may_be_in_markdown(source) {
        return 0;
    }
    leaves(source).iter().map(|leaf| needle.ranges(leaf).count()).sum()
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
    fn a_needle_across_an_image_is_not_found() {
        assert_eq!(count(&needle("before after"), "before ![a picture](a.png)after"), 0);
    }
}
