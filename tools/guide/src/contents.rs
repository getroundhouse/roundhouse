//! The guide's contents: the sections `README.md` lists its pages under. The site's navigation follows them, and so
//! do each page's links to the pages before and after it.

use markdown::mdast::{List, Node};

use crate::error::Error;
use crate::links::{Target, target};
use crate::page::{Page, PagePath, plain};
use crate::source::Source;

pub(crate) struct Contents {
    pub(crate) sections: Vec<Section>,
}

pub(crate) struct Section {
    pub(crate) title: String,
    pub(crate) entries: Vec<Entry>,
}

pub(crate) struct Entry {
    /// What the navigation calls the page, which can be shorter than its title.
    pub(crate) label: String,
    pub(crate) page: PagePath,
    /// The pages that are parts of this one, such as the parts of a chat under Chats.
    pub(crate) under: Vec<Entry>,
}

impl Contents {
    /// Reads the contents from the page the guide opens on. Each `## ` heading starts a section, and each item of a
    /// list under it starts with a link to a page, with the pages under that page in a list of the item's own.
    /// Anything else on the page is only drawn.
    pub(crate) fn read(source: &Source, first: &Page) -> Result<Self, Error> {
        let mut sections: Vec<Section> = Vec::new();
        for block in first.root.children().into_iter().flatten() {
            if let Node::Heading(heading) = block {
                if heading.depth == 2 {
                    sections.push(Section { title: plain(block), entries: Vec::new() });
                }
            } else if let (Node::List(list), Some(section)) = (block, sections.last_mut()) {
                section.entries.extend(entries(source, list)?);
            }
        }
        Ok(Self { sections })
    }

    /// Every page they list, in the order they list them, each before the pages under it.
    pub(crate) fn pages(&self) -> Vec<&Entry> {
        let mut pages = Vec::new();
        for section in &self.sections {
            flatten(&section.entries, &mut pages);
        }
        pages
    }
}

fn flatten<'a>(entries: &'a [Entry], pages: &mut Vec<&'a Entry>) {
    for entry in entries {
        pages.push(entry);
        flatten(&entry.under, pages);
    }
}

fn entries(source: &Source, list: &List) -> Result<Vec<Entry>, Error> {
    list.children.iter().map(|item| entry(source, item)).collect()
}

/// An item of the contents: a link to another page, read as a link anywhere on the page is, then whatever says what
/// the page is for, then perhaps a list of the pages under it.
fn entry(source: &Source, item: &Node) -> Result<Entry, Error> {
    let not_a_page = || Error::NotAPage(plain(item));
    let parts = item.children().ok_or_else(not_a_page)?;
    let Some(Node::Paragraph(first)) = parts.first() else { return Err(not_a_page()) };
    let Some(Node::Link(link)) = first.children.first() else { return Err(not_a_page()) };
    let here = PagePath::contents();
    let Ok(Target::Page { page, fragment: None }) = target(source, &here, &link.url) else { return Err(not_a_page()) };
    if page == here {
        return Err(not_a_page());
    }
    let mut under = Vec::new();
    for part in parts {
        if let Node::List(list) = part {
            under.extend(entries(source, list)?);
        }
    }
    Ok(Entry { label: link.children.iter().map(plain).collect(), page, under })
}
