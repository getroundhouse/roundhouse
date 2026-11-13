//! Where the images a text points to are. They're found the way the transcript's text view finds them, with the same
//! Markdown parser, so the address the worker keeps an image by is the one a client asks for.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use markdown::ParseOptions;
use markdown::mdast::Node;

/// Whether `text` might point to an image, which costs far less to ask than parsing it. Every image in Markdown, whether
/// it gives its address or refers to one, starts with `![`.
pub(super) fn might_have(text: &str) -> bool {
    text.contains("![")
}

/// The address of each image `text` points to, in the order they come, each once. An image that refers to an address
/// given elsewhere, as `![The board][board]` does, is at every address given under that name, since the text view takes
/// one of them.
pub(super) fn of(text: &str) -> Vec<String> {
    if !might_have(text) {
        return Vec::new();
    }
    // The text view also parses `$` as math, but draws math nobody claims as Markdown again, so an image inside it is
    // still an image, as it is here.
    //
    // The parser panics on some texts, such as `"a\n===\n---\nb\n==="`, which the agent can write. `super::keep_in`
    // would carry on past a panic, but send the event without any of its images, so a text it panics on is one it
    // can't read, which points to none, and the event's other texts keep theirs.
    let Ok(Ok(root)) = std::panic::catch_unwind(|| markdown::to_mdast(text, &ParseOptions::gfm())) else {
        return Vec::new();
    };
    let mut given = HashMap::new();
    walk(&root, &mut |node| {
        if let Node::Definition(definition) = node {
            given.entry(definition.identifier.clone()).or_insert_with(Vec::new).push(definition.url.clone());
        }
    });
    let (mut found, mut seen) = (Vec::new(), HashSet::new());
    walk(&root, &mut |node| {
        let addresses: &[String] = if let Node::Image(image) = node {
            std::slice::from_ref(&image.url)
        } else if let Node::ImageReference(image) = node {
            given.get(&image.identifier).map_or(&[], Vec::as_slice)
        } else {
            &[]
        };
        for address in addresses {
            if seen.insert(address.clone()) {
                found.push(address.clone());
            }
        }
    });
    found
}

fn walk<'a>(node: &'a Node, visit: &mut impl FnMut(&'a Node)) {
    visit(node);
    for child in node.children().into_iter().flatten() {
        walk(child, visit);
    }
}

/// The path `address` gives, in full or from the worktree, or `None` if it's somewhere else, such as on the web or
/// written into the text as a `data:` URL, which a client draws from the text. A path can be a `file://` URL, and either
/// can be percent-encoded, as an address with a space in it has to be.
pub(super) fn path(address: &str) -> Option<String> {
    if address.starts_with("data:") {
        return None;
    }
    let path = address.strip_prefix("file://").unwrap_or(address);
    if path.is_empty() || path.contains("://") {
        return None;
    }
    let decoded = percent_encoding::percent_decode_str(path).decode_utf8();
    Some(decoded.map_or_else(|_| path.to_string(), Cow::into_owned))
}
