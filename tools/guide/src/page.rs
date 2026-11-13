//! A page of the guide: one Markdown file, with its title and its headings.

use std::fmt;

use markdown::ParseOptions;
use markdown::mdast::{Heading as HeadingNode, Node};

use crate::error::{Error, NotDrawn};
use crate::slug::{Slug, Slugs};
use crate::source::{GUIDE_DIR, RepoPath};

/// Which page of the guide: its path from the guide's directory without its `.md`, with forward slashes, such as
/// `reference/chats`. The contents and the other pages name a page by its file, `reference/chats.md`, which is how one
/// is written out. One is made only from a file found in the guide, or a link resolved to one.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PagePath(String);

impl PagePath {
    /// The page the guide opens on, which holds its contents.
    pub(crate) fn contents() -> Self {
        Self("README".to_string())
    }

    /// The page at `file`, if that's a Markdown file in the guide's directory.
    pub(crate) fn at(file: &RepoPath) -> Option<Self> {
        let page = file.as_str().strip_prefix(GUIDE_DIR)?.strip_prefix('/')?.strip_suffix(".md")?;
        // A file named only `.md` has no name to be published under, in the guide's directory or in one inside it.
        (!page.is_empty() && !page.ends_with('/')).then(|| Self(page.to_string()))
    }

    /// Where the page's file is in the repository.
    pub(crate) fn file(&self) -> RepoPath {
        RepoPath::of(&[GUIDE_DIR, &self.to_string()])
    }

    /// The directory the page is published as, from the guide's own: none for the page the guide opens on, and
    /// `reference/chats/` for `reference/chats.md`, so a page with pages under it is their directory's index.
    pub(crate) fn dir(&self) -> String {
        if *self == Self::contents() { String::new() } else { format!("{}/", self.0) }
    }

    /// The way from the page's directory up to the guide's, such as `../../`, so every link a page makes is relative
    /// and the site can be served from any address.
    pub(crate) fn up(&self) -> String {
        let depth = self.dir().matches('/').count();
        if depth == 0 { "./".to_string() } else { "../".repeat(depth) }
    }

    /// The address of the page `to`, as a link on this page writes it.
    pub(crate) fn href(&self, to: &Self) -> String {
        format!("{}{}", self.up(), to.dir())
    }
}

impl fmt::Display for PagePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.md", self.0)
    }
}

pub(crate) struct Page {
    pub(crate) path: PagePath,
    pub(crate) title: String,
    /// What a search engine or a link's preview says of the page: its first paragraph.
    pub(crate) summary: String,
    pub(crate) root: Node,
    /// Every heading, in the order the page has them. The first is its title, and there's no other `# ` heading.
    pub(crate) headings: Vec<Heading>,
}

pub(crate) struct Heading {
    pub(crate) depth: u8,
    pub(crate) text: String,
    pub(crate) slug: Slug,
}

impl Page {
    pub(crate) fn parse(path: PagePath, text: &str) -> Result<Self, Error> {
        // The parser panics on some texts, such as `"a\n===\n---\nb\n==="`, as the comment on it in the workspace's
        // manifest says, and a page that makes it is refused as any other the build can't read.
        let root = match std::panic::catch_unwind(|| markdown::to_mdast(text, &ParseOptions::gfm())) {
            Ok(Ok(root)) => root,
            Ok(Err(message)) => return Err(Error::Markdown { page: path, message: message.to_string() }),
            Err(_) => return Err(Error::Markdown { page: path, message: "the parser panicked".to_string() }),
        };
        let blocks = root.children().map_or(&[][..], Vec::as_slice);
        let Some(Node::Heading(HeadingNode { depth: 1, children: title, .. })) = blocks.first() else {
            return Err(Error::NoTitle(path));
        };
        let title = plain_all(title);
        let mut headings = Vec::new();
        collect_headings(&root, &mut Slugs::default(), &mut headings);
        let not_drawn = if headings.iter().skip(1).any(|heading| heading.depth == 1) {
            Some(NotDrawn::SecondTitle)
        } else if headings.iter().any(|heading| heading.slug.is_empty()) {
            Some(NotDrawn::HeadingWithoutWords)
        } else {
            None
        };
        if let Some(what) = not_drawn {
            return Err(Error::Unsupported { page: path, what });
        }
        let summary = blocks.iter().find(|block| matches!(block, Node::Paragraph(_))).map_or_default(plain);
        Ok(Self { path, title, summary, root, headings })
    }

    pub(crate) fn has_heading(&self, fragment: &str) -> bool {
        self.headings.iter().any(|heading| heading.slug.as_str() == fragment)
    }
}

fn collect_headings(node: &Node, slugs: &mut Slugs, found: &mut Vec<Heading>) {
    if let Node::Heading(heading) = node {
        let text = plain_all(&heading.children);
        found.push(Heading { depth: heading.depth, slug: slugs.next(&text), text });
        return;
    }
    for child in node.children().into_iter().flatten() {
        collect_headings(child, slugs, found);
    }
}

/// What a node says, without what marks it up: a heading's words for its slug, or a paragraph's for a summary.
pub(crate) fn plain(node: &Node) -> String {
    if let Node::Text(text) = node {
        return text.value.replace('\n', " ");
    }
    if let Node::InlineCode(code) = node {
        return code.value.clone();
    }
    if let Node::Image(image) = node {
        return image.alt.clone();
    }
    if let Node::Break(_) = node {
        return " ".to_string();
    }
    node.children().map_or_default(|children| plain_all(children))
}

/// What these nodes say, one after another.
pub(crate) fn plain_all(nodes: &[Node]) -> String {
    nodes.iter().map(plain).collect()
}
