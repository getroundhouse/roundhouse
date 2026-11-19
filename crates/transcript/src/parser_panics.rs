//! The `markdown` crate, which the transcript parses with, as the kit does, panics on some texts, such as
//! `"a\n===\n---\nb\n==="`, which the agent can write, and which would end the app each time its chat was drawn. So
//! everything here that parses Markdown goes through [`unless_it_panics`], and takes a text the parser panics on as
//! one it can't read, as it takes an error. The worker parses with the same crate, and catches its panic itself, since
//! neither crate depends on the other, and nothing both depend on parses Markdown.

use std::panic::UnwindSafe;

/// What `parse` made of a text, or `None` if it couldn't read it: it failed, or it panicked. Rust's default hook still
/// says so on stderr each time, which a short text drawn again after its blocks were put away, or counted by find, is.
pub(crate) fn unless_it_panics<T, E>(parse: impl FnOnce() -> Result<T, E> + UnwindSafe) -> Option<T> {
    std::panic::catch_unwind(parse).ok()?.ok()
}

#[cfg(test)]
mod tests {
    use crate::images_apart::images_apart;
    use crate::texts::{Leaf, leaves};

    /// Every parse of a text the parser panics on reads it as it's written: the kit's, which draws it as one
    /// paragraph, and the one that finds the images written one after another, which leaves it as it is.
    #[test]
    fn a_text_the_parser_panics_on_is_read_as_it_is_written() {
        let panics = "a\n===\n---\nb\n===";
        let with_images = format!("![Before](before.png)\n![After](after.png)\n\n{panics}");

        let read = std::panic::catch_unwind(|| leaves(panics));
        let apart = std::panic::catch_unwind(|| images_apart(&with_images).into_owned());

        let read = read.unwrap_or_else(|_| panic!("the kit's parse panicked"));
        assert_eq!(read.iter().map(Leaf::shows).collect::<Vec<_>>(), [panics], "the kit's parse");
        let apart = apart.unwrap_or_else(|_| panic!("finding the images written one after another panicked"));
        assert_eq!(apart, with_images, "finding the images written one after another");
    }
}
