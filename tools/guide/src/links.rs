//! Where a link or a picture in a page points.

use crate::error::{Error, NotDrawn};
use crate::page::PagePath;
use crate::source::{RepoPath, Source};

pub(crate) enum Target {
    /// Somewhere on the web, or an address to write to, published as it's written.
    Elsewhere(String),
    /// A page of the guide, and a heading in it if the link names one.
    Page { page: PagePath, fragment: Option<String> },
    /// A file or a directory in the repository that isn't a page of the guide, and a place in it if the link names
    /// one. It's there, since one that isn't is refused.
    File { file: RepoPath, fragment: Option<String> },
}

/// Where `url`, written in the page `from`, points. A path is from the page's own directory, as GitHub reads it, so
/// the same link works there.
pub(crate) fn target(source: &Source, from: &PagePath, url: &str) -> Result<Target, Error> {
    if ["https://", "http://", "mailto:"].iter().any(|kind| url.starts_with(kind)) {
        return Ok(Target::Elsewhere(url.to_string()));
    }
    // Anything else before a colon is another kind of address, such as `javascript:`, which nothing should publish.
    if url.split(['/', '#', '?']).next().is_some_and(|start| start.contains(':')) {
        return Err(Error::Unsupported { page: from.clone(), what: NotDrawn::AddressKind });
    }
    let (path, fragment) = match url.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment.to_string())),
        None => (url, None),
    };
    if path.is_empty() {
        return Ok(Target::Page { page: from.clone(), fragment });
    }
    let file =
        from_the_root(from, path).ok_or_else(|| Error::Outside { page: from.clone(), target: url.to_string() })?;
    if let Some(page) = PagePath::at(&file) {
        return Ok(Target::Page { page, fragment });
    }
    if !source.on_disk(&file).exists() {
        return Err(Error::Missing { page: from.clone(), target: url.to_string() });
    }
    Ok(Target::File { file, fragment })
}

/// `path`, written in the page `from`, as a path from the repository's root, or `None` if it leaves the repository
/// or starts from the root of something else.
fn from_the_root(from: &PagePath, path: &str) -> Option<RepoPath> {
    if path.starts_with('/') {
        return None;
    }
    let page = from.file();
    let mut names: Vec<&str> = page.as_str().split('/').collect();
    // The page's own name, which leaves the directory it's in.
    names.pop();
    for name in path.split('/') {
        match name {
            "" | "." => {}
            ".." => {
                names.pop()?;
            }
            name => names.push(name),
        }
    }
    Some(RepoPath::of(&names))
}
