use protocol::model::{FilePage, OpenedText, PageLine};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::sync::Arc;

/// Cheap to clone, since its pages are shared.
#[derive(Clone, Debug)]
pub struct FileText {
    pub(super) lines: u32,
    pub(super) page_lines: u32,
    pub(super) pages: BTreeMap<u32, Arc<[PageLine]>>,
    pub(super) fetching: BTreeSet<u32>,
}

impl FileText {
    /// A file just opened, with any pages that were sent with it.
    pub fn new(opened: OpenedText) -> Self {
        let OpenedText { lines, page_lines, pages, .. } = opened;
        let mut text = Self { lines, page_lines: page_lines.max(1), pages: BTreeMap::new(), fetching: BTreeSet::new() };
        text.take(pages);
        text
    }

    /// How many lines the file has.
    pub fn lines(&self) -> u32 {
        self.lines
    }

    /// How many lines a page has, which pages are asked for with.
    pub fn page_lines(&self) -> u32 {
        self.page_lines
    }

    /// Whether it holds every page.
    pub fn is_whole(&self) -> bool {
        self.pages.len() == usize::try_from(self.page_count()).unwrap_or(usize::MAX)
    }

    /// Whether a line it holds was too long to be sent whole, so what it holds isn't quite the file.
    pub fn has_cut_lines(&self) -> bool {
        self.pages.values().flat_map(|lines| lines.iter()).any(|line| line.cut.is_some())
    }

    /// Line `number`, numbered from 1, if it holds the page it's on.
    pub fn line(&self, number: u32) -> Option<&PageLine> {
        let index = number.checked_sub(1)?;
        let page = self.pages.get(&(index / self.page_lines))?;
        page.get(usize::try_from(index % self.page_lines).ok()?)
    }

    /// The pages it holds, in order, each by its number and the lines it has.
    pub fn pages(&self) -> impl Iterator<Item = (u32, Range<u32>, &[PageLine])> {
        self.pages.iter().map(|(index, lines)| (*index, self.lines_on(*index), &lines[..]))
    }

    /// The lines on page `index`, by number.
    pub fn lines_on(&self, index: u32) -> Range<u32> {
        let start = index.saturating_mul(self.page_lines).saturating_add(1);
        start..start.saturating_add(self.page_lines).min(self.lines.saturating_add(1))
    }

    /// The lines it holds, by number, in order, with those next to each other joined.
    pub fn held(&self) -> Vec<Range<u32>> {
        self.pages.keys().fold(Vec::new(), |mut held: Vec<Range<u32>>, index| {
            let lines = self.lines_on(*index);
            match held.last_mut() {
                Some(last) if last.end == lines.start => last.end = lines.end,
                _ => held.push(lines),
            }
            held
        })
    }

    /// The pages to fetch so that `lines` can show, which then count as being fetched: those `lines` are on that it
    /// doesn't hold and isn't fetching, in order.
    pub fn to_fetch(&mut self, lines: &[Range<u32>]) -> Vec<u32> {
        let in_the_file = |lines: &Range<u32>| lines.start.max(1)..lines.end.min(self.lines.saturating_add(1));
        let pages = lines
            .iter()
            .map(in_the_file)
            .filter(|lines| !lines.is_empty())
            .flat_map(|lines| (lines.start - 1) / self.page_lines..=(lines.end - 2) / self.page_lines);
        let wanted: BTreeSet<u32> =
            pages.filter(|page| !self.pages.contains_key(page) && !self.fetching.contains(page)).collect();
        self.fetching.extend(&wanted);
        wanted.into_iter().collect()
    }

    /// Takes the pages fetched when `asked` were, and stops counting those asked as being fetched, so any that
    /// didn't come, or failed, are asked for again when they're next needed. Says which it holds now that it didn't.
    pub fn fetched(&mut self, asked: &[u32], pages: Vec<FilePage>) -> Vec<u32> {
        for page in asked {
            self.fetching.remove(page);
        }
        self.take(pages)
    }

    /// Keeps each page that's in the file and not held already, with no more lines than the page has.
    pub(super) fn take(&mut self, pages: Vec<FilePage>) -> Vec<u32> {
        let mut taken = Vec::new();
        for FilePage { index, mut lines } in pages {
            if index >= self.page_count() || self.pages.contains_key(&index) {
                continue;
            }
            lines.truncate(self.lines_on(index).len());
            self.pages.insert(index, lines.into());
            taken.push(index);
        }
        taken
    }

    pub(super) fn page_count(&self) -> u32 {
        self.lines.div_ceil(self.page_lines)
    }
}
