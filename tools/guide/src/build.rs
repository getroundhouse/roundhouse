//! Reads the guide from a repository and writes it out as the site's pages.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

use crate::contents::Contents;
use crate::draw::draw;
use crate::error::{Error, io_error};
use crate::page::{Page, PagePath};
use crate::picture::Pictures;
use crate::shell::{FIRST_LABEL, Placed, around};
use crate::source::{GUIDE_DIR, RepoPath, Source};

/// How much was published.
#[derive(Debug, PartialEq, Eq)]
pub struct Built {
    pub pages: usize,
    pub pictures: usize,
}

/// Builds the guide into `out`, which must not exist, so a page that has gone from the guide can't be left there.
/// Nothing is written unless every page can be drawn.
pub fn build(source: &Source, out: &Path) -> Result<Built, Error> {
    if out.exists() {
        return Err(Error::OutExists(out.to_path_buf()));
    }
    let pages = read_pages(source)?;
    let first = pages.get(&PagePath::contents()).ok_or(Error::NoContents)?;
    let contents = Contents::read(source, first)?;
    let order = in_order(first, &contents, &pages)?;

    let mut pictures = Pictures::default();
    let mut drawn = Vec::new();
    for (at, placed) in order.iter().enumerate() {
        let content = draw(source, placed.page, &pages, &mut pictures)?;
        let before = at.checked_sub(1).and_then(|before| order.get(before)).copied();
        let html = around(placed.page, &contents, before, order.get(at + 1).copied(), &content);
        drawn.push((out.join(placed.page.path.dir()), html));
    }

    for (dir, html) in &drawn {
        fs::create_dir_all(dir).map_err(io_error(dir))?;
        let file = dir.join("index.html");
        fs::write(&file, html).map_err(io_error(&file))?;
    }
    let pictures = pictures.publish(source, out)?;
    Ok(Built { pages: drawn.len(), pictures })
}

/// Every page in the guide's directory, however deep.
fn read_pages(source: &Source) -> Result<BTreeMap<PagePath, Page>, Error> {
    let mut files = Vec::new();
    find_files(source, &RepoPath::of(&[GUIDE_DIR]), &mut files)?;
    let mut pages = BTreeMap::new();
    for page in files.iter().filter_map(PagePath::at) {
        let file = source.on_disk(&page.file());
        let text = fs::read_to_string(&file).map_err(io_error(&file))?;
        pages.insert(page.clone(), Page::parse(page, &text)?);
    }
    Ok(pages)
}

/// Every file in `dir` and the directories in it.
fn find_files(source: &Source, dir: &RepoPath, found: &mut Vec<RepoPath>) -> Result<(), Error> {
    let on_disk = source.on_disk(dir);
    for entry in fs::read_dir(&on_disk).map_err(io_error(&on_disk))? {
        let entry = entry.map_err(io_error(&on_disk))?;
        let path = entry.path();
        let name = entry.file_name();
        let Some(name) = name.to_str() else { return Err(Error::NotUtf8(path)) };
        let inside = RepoPath::of(&[dir.as_str(), name]);
        if path.is_dir() {
            find_files(source, &inside, found)?;
        } else {
            found.push(inside);
        }
    }
    Ok(())
}

/// The pages in the order the guide is read in: the page it opens on, then each page its contents list. The contents
/// list every other page, once, and nothing that isn't one.
fn in_order<'a>(
    first: &'a Page,
    contents: &'a Contents,
    pages: &'a BTreeMap<PagePath, Page>,
) -> Result<Vec<Placed<'a>>, Error> {
    let mut order = vec![Placed { page: first, label: FIRST_LABEL }];
    let mut listed = HashSet::new();
    for entry in contents.pages() {
        let Some(page) = pages.get(&entry.page) else {
            return Err(Error::Missing { page: first.path.clone(), target: entry.page.to_string() });
        };
        if !listed.insert(&entry.page) {
            return Err(Error::ListedTwice(entry.page.clone()));
        }
        order.push(Placed { page, label: &entry.label });
    }
    match pages.keys().find(|page| **page != first.path && !listed.contains(page)) {
        Some(left_out) => Err(Error::NotInContents(left_out.clone())),
        None => Ok(order),
    }
}
