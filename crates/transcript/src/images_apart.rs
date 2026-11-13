//! Images written one after another with nothing else between them, each made a paragraph of its own before the kit
//! parses them, so each is drawn as wide as it is, one under another. The kit draws a paragraph that's only
//! an image at its own width, and any image in a paragraph with text in it at the height of the text, and the line
//! break or space between two images is text to it. So a reply with a screenshot on each line would show them as
//! icons. A reply's own images, which `crate::linked_images` draws rather than the text view, are drawn by the same
//! rule. A prompt's text goes through here too, since images on lines one after another there would be as small.
//! What the hub keeps is the text as it was written, and only what's drawn changes.

use std::borrow::Cow;
use std::ops::Range;

use markdown::ParseOptions;
use markdown::mdast::{Link, LinkReference, ListItem, Node, Paragraph};
use markdown::unist::Point;

/// How many columns apart Markdown's tab stops are.
const TAB_STOP: usize = 4;

/// `text`, with what's between each two images in a paragraph that's only images made a blank line, and the line
/// after it started as the paragraph's first line is, so the image on it stays in the same list items, quotes and
/// footnotes. `text` itself when there's none.
pub(crate) fn images_apart(text: &str) -> Cow<'_, str> {
    // This is asked of every text that's drawn, and of the one the agent is writing each time it grows.
    if !an_image_may_follow_another(text) {
        return Cow::Borrowed(text);
    }
    // The parser panics on some texts, as `crate::texts::parse` says, and one it can't read is left as it is.
    let Ok(Ok(root)) = std::panic::catch_unwind(|| markdown::to_mdast(text, &ParseOptions::gfm())) else {
        return Cow::Borrowed(text);
    };
    let mut gaps = Vec::new();
    find_gaps(&root, &mut Vec::new(), text, &mut gaps);
    if gaps.is_empty() {
        return Cow::Borrowed(text);
    }
    let mut apart = String::with_capacity(text.len() + gaps.iter().map(|gap| gap.instead.len()).sum::<usize>());
    let mut at = 0;
    for Gap { between, instead } in gaps {
        apart.push_str(&text[at..between.start]);
        apart.push_str(&instead);
        at = between.end;
    }
    apart.push_str(&text[at..]);
    Cow::Owned(apart)
}

/// Whether some image may follow another with nothing but spaces, line breaks and quote markers between them, which
/// a text needs before it's worth parsing. It's one pass over the text, where parsing a long reply takes milliseconds,
/// and code such as Rust's `vec![` and `#![` doesn't pass, since what's before those isn't the end of an image.
fn an_image_may_follow_another(text: &str) -> bool {
    text.match_indices("![").any(|(at, _)| {
        let before = &text[..at];
        // A link around an image starts at the `[` before it.
        let before = before.trim_suffix('[');
        // A backslash at the end of a line breaks it.
        let before = before.trim_end_matches([' ', '\t', '\r', '\n', '>', '\\']);
        // An image, or a link around one, ends in `)`, or in `]` when it's a reference.
        before.ends_with([')', ']'])
    })
}

/// What's between two images of a paragraph that's only images, and the blank line and start of a line that take its
/// place.
struct Gap {
    between: Range<usize>,
    instead: String,
}

/// A footnote's label or a task's box, which is on its first line after any list marker and quote marker, and isn't
/// on the lines after it.
struct Label {
    at: Range<usize>,
    /// What a line after the first has where the label is: four spaces for a footnote, and nothing for a task.
    instead: &'static str,
}

/// The gaps between the images of each paragraph under `node` that's only images, in order. `labels` are those of the
/// footnotes and tasks `node` is in.
fn find_gaps(node: &Node, labels: &mut Vec<Label>, text: &str, gaps: &mut Vec<Gap>) {
    if let Node::Paragraph(paragraph) = node {
        paragraph_gaps(paragraph, labels, text, gaps);
        return;
    }
    let outer = labels.len();
    labels.extend(label(node, text));
    for child in node.children().into_iter().flatten() {
        find_gaps(child, labels, text, gaps);
    }
    labels.truncate(outer);
}

fn paragraph_gaps(paragraph: &Paragraph, labels: &[Label], text: &str, gaps: &mut Vec<Gap>) {
    let mut images = Vec::new();
    for child in &paragraph.children {
        if is_image(child) {
            let Some(at) = child.position() else { return };
            images.push(at.start.offset..at.end.offset);
        } else if !is_space(child) {
            return;
        }
    }
    let Some(start) = paragraph.position.as_ref().and_then(|at| line_start(&at.start, labels, text)) else { return };
    let instead = format!("\n{}\n{start}", start.trim_end());
    for pair in images.windows(2) {
        gaps.push(Gap { between: pair[0].end..pair[1].start, instead: instead.clone() });
    }
}

/// What a line starts with to be in the same list items, quotes and footnotes as the paragraph that starts at
/// `start`, from what's before it on its first line: quote markers as they are, anything else, such as a list marker
/// or a tab, as spaces as wide, and each of `labels` on that line as its `instead`. A line after the first can go on a
/// paragraph without starting as the first does, so only the first says which items and quotes the paragraph is in.
fn line_start(start: &Point, labels: &[Label], text: &str) -> Option<String> {
    let before = text.get(..start.offset)?;
    let line = before.rfind(['\n', '\r']).map_or(0, |end| end + 1);
    let mut starts = String::new();
    // As Markdown counts them, each byte a column, and a tab to the next stop.
    let mut column = 0;
    for (at, c) in before.get(line..)?.char_indices() {
        let wide = if c == '\t' { TAB_STOP - column % TAB_STOP } else { c.len_utf8() };
        column += wide;
        let at = line + at;
        match labels.iter().find(|label| label.at.start >= line && label.at.contains(&at)) {
            Some(label) if label.at.start == at => starts.push_str(label.instead),
            Some(_) => {}
            None if c == '>' => starts.push(c),
            None => starts.extend(std::iter::repeat_n(' ', wide)),
        }
    }
    // A paragraph can start partway into a tab, whose columns before that are its containers'.
    starts.extend(std::iter::repeat_n(' ', start.column.saturating_sub(1).saturating_sub(column)));
    Some(starts)
}

/// Where `node`'s label is, if it's a footnote or a task: from its start, or its box's, to its first child.
fn label(node: &Node, text: &str) -> Option<Label> {
    let start = node.position()?.start.offset;
    let first = node.children()?.first()?.position()?.start.offset;
    if let Node::FootnoteDefinition(_) = node {
        // Its lines after the first are four spaces in, however long its label.
        return Some(Label { at: start..first, instead: "    " });
    }
    if let Node::ListItem(ListItem { checked: Some(_), .. }) = node {
        // The box, `[ ]` or `[x]`, is the last thing before its first paragraph, which starts on the same line.
        let checkbox = text.get(start..first)?.rfind('[')?;
        return Some(Label { at: start + checkbox..first, instead: "" });
    }
    None
}

/// An image, or a link that's only an image.
fn is_image(node: &Node) -> bool {
    if let Node::Link(Link { children, .. }) | Node::LinkReference(LinkReference { children, .. }) = node {
        return matches!(children.as_slice(), [only] if is_image(only));
    }
    matches!(node, Node::Image(_) | Node::ImageReference(_))
}

/// A line break, or spaces.
fn is_space(node: &Node) -> bool {
    matches!(node, Node::Break(_)) || matches!(node, Node::Text(text) if text.value.trim().is_empty())
}

#[cfg(test)]
mod tests;
