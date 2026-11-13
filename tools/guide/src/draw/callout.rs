//! A note set apart from the page's words, written as GitHub writes one: a quote whose first line is only a marker,
//! such as `[!NOTE]` or `[!WARNING]`.

use markdown::mdast::{Blockquote, Node};

#[derive(Clone, Copy)]
pub(crate) enum Callout {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

impl Callout {
    /// The kind a marker names, between its `[!` and `]`, in either case, as GitHub reads it.
    fn named(word: &str) -> Option<Self> {
        Some(match word.to_ascii_uppercase().as_str() {
            "NOTE" => Self::Note,
            "TIP" => Self::Tip,
            "IMPORTANT" => Self::Important,
            "WARNING" => Self::Warning,
            "CAUTION" => Self::Caution,
            _ => return None,
        })
    }

    /// What the note is headed with. Its class is the same word in lower case.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Note => "Note",
            Self::Tip => "Tip",
            Self::Important => "Important",
            Self::Warning => "Warning",
            Self::Caution => "Caution",
        }
    }
}

/// Which kind of note `quote` is, and what it says without its marker, or `None` for a plain quote. The marker has
/// its line to itself, as GitHub asks, so a quote that only starts with one reads the same in both places.
pub(crate) fn of(quote: &Blockquote) -> Option<(Callout, Vec<Node>)> {
    let Some(Node::Paragraph(first)) = quote.children.first() else { return None };
    let Some(Node::Text(start)) = first.children.first() else { return None };
    let (word, rest) = start.value.strip_prefix("[!")?.split_once(']')?;
    let kind = Callout::named(word)?;

    let mut first = first.clone();
    if let Some(after_its_line) = rest.strip_prefix('\n') {
        if let Some(Node::Text(start)) = first.children.first_mut() {
            start.value = after_its_line.to_string();
        }
    } else if rest.trim().is_empty() {
        // The marker is all of its text. Its line ends there if the paragraph does, or at a line break.
        first.children.remove(0);
        match first.children.first() {
            None => {}
            Some(Node::Break(_)) => {
                first.children.remove(0);
            }
            Some(_) => return None,
        }
    } else {
        return None;
    }
    // A marker with nothing after it in its paragraph leaves no paragraph to draw.
    let mut body = if first.children.is_empty() { Vec::new() } else { vec![Node::Paragraph(first)] };
    body.extend(quote.children.iter().skip(1).cloned());
    Some((kind, body))
}
