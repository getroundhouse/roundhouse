//! A world's base files, opened and paged as the worker opens and pages them, for the gallery's hub to answer with.

#![cfg(test)]

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{FileContents, FileOpened, FilePage, FilePages, OpenedText, PageLine};

use super::world::World;

/// How many lines a page of a base file has, as the worker pages them.
const PAGE_LINES: u32 = 256;

/// The base file at `path`, opened with every page.
pub(super) fn opened(world: &World, path: &str) -> Result<FileOpened, ApiError> {
    let text = base_text(world, path)?;
    let lines = u32::try_from(text.lines().count()).unwrap();
    let pages = pages_of(text, PAGE_LINES, &(0..lines.div_ceil(PAGE_LINES)).collect::<Vec<_>>());
    let bytes = u64::try_from(text.len()).unwrap();
    Ok(FileOpened::Text(OpenedText { bytes, lines, page_lines: PAGE_LINES, pages }))
}

/// Pages `pages` of the base file at `path`, with `page_lines` lines a page.
pub(super) fn paged(world: &World, path: &str, page_lines: u32, pages: &[u32]) -> Result<FilePages, ApiError> {
    Ok(FilePages { pages: pages_of(base_text(world, path)?, page_lines, pages) })
}

/// The text of a world's base file at `path`.
fn base_text<'a>(world: &'a World, path: &str) -> Result<&'a str, ApiError> {
    match world.base_files.get(path) {
        Some(FileContents::Text { text }) => Ok(text),
        _ => Err(ApiError::new(ErrorCode::NotFound, "no text in the base")),
    }
}

/// Pages `pages` of `text`, with `page_lines` lines a page, as the worker sends them.
fn pages_of(text: &str, page_lines: u32, pages: &[u32]) -> Vec<FilePage> {
    let (lines, size): (Vec<&str>, usize) = (text.lines().collect(), usize::try_from(page_lines).unwrap());
    let page = |index: u32| {
        let first = usize::try_from(index).unwrap() * size;
        let lines = lines.get(first..(first + size).min(lines.len()))?;
        let lines = lines.iter().map(|line| PageLine { text: (*line).to_string(), cut: None }).collect();
        Some(FilePage { index, lines })
    };
    pages.iter().filter_map(|index| page(*index)).collect()
}
