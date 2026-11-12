//! A file as a commit held it, read a page of lines at a time, so one too big to send whole can still be shown. Each
//! read streams the file from git, keeping only the pages asked for, and stops once it has them. Nothing is kept
//! between reads, and each says in the worker's log how much it streamed and how long it took, so whether streaming
//! keeps up with the files people have can be read from there.

use std::ops::ControlFlow;
use std::path::Path;
use std::time::{Duration, Instant};

use protocol::attachment::MediaType;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{CommitId, FileOpened, FilePage, FilePages, OpenedText, PageLine};
use tracing::{debug, info};

use super::{MAX_TEXT_BYTES, SNIFF_BYTES, relative};
use crate::git::blob::{self, Streamed};

#[cfg(test)]
mod tests;

/// How many lines a page has: a few screens, so the first press on a fold's arrow reads what the next ones show.
const PAGE_LINES: u32 = 256;
/// The most lines a page can be asked to have, so a page stays a few screens.
const MOST_PAGE_LINES: u32 = 4096;
/// The most of a line of the file that's sent when it's read in pages, far more than anyone reads on one line.
const MAX_LINE_BYTES: usize = 16 * 1024;
/// What a line takes to send besides its text, roughly, so a page of short lines counts for what it is.
const LINE_BYTES: u64 = 16;
/// A read that takes longer than this is logged where it's seen without asking for more, so a slow one stands out.
const SLOW: Duration = Duration::from_millis(100);

/// Opens the file at `path` in `commit`, from the repository of the worktree at `worktree`, with every page when it's
/// no bigger than text's limit, none of its lines cut short. Refused as [`super::read_at`] is.
pub(crate) async fn open_at(worktree: &Path, commit: &CommitId, path: &str) -> Result<FileOpened, ApiError> {
    relative(path)?;
    let started = Instant::now();
    let blob = blob::find(worktree, commit, path).await?;
    let whole = blob.bytes <= MAX_TEXT_BYTES;
    let (wanted, line) = if whole { (Wanted::All, usize::MAX) } else { (Wanted::Pages(Vec::new()), MAX_LINE_BYTES) };
    let mut paging = Paging::new(wanted, Reads::ToTheEnd, PAGE_LINES, Limits { line, reply: MAX_TEXT_BYTES });
    let streamed = blob.stream(worktree, |chunk| paging.feed(chunk)).await?;
    let paged = paging.finish(&streamed);
    let lines = paged.lines;
    let opened = if paged.text {
        FileOpened::Text(OpenedText { bytes: blob.bytes, lines, page_lines: PAGE_LINES, pages: paged.pages })
    } else {
        FileOpened::NotText
    };
    let said = || format!("opened {path} at {commit}, of {} bytes and {lines} lines", blob.bytes);
    logged(!whole, &said, streamed, started);
    Ok(opened)
}

/// Pages `pages` of the file at `path` in `commit`, with `page_lines` lines a page, in the order the file has them,
/// as many as fit in text's limit and always the first, and none past its end. Refused as [`open_at`] is, and when
/// `page_lines` is 0 or more than a page can have.
pub(crate) async fn pages(
    worktree: &Path,
    commit: &CommitId,
    path: &str,
    page_lines: u32,
    pages: &[u32],
) -> Result<FilePages, ApiError> {
    relative(path)?;
    if !(1..=MOST_PAGE_LINES).contains(&page_lines) {
        return Err(ApiError::new(ErrorCode::InvalidRequest, format!("a page can't have {page_lines} lines")));
    }
    let started = Instant::now();
    let blob = blob::find(worktree, commit, path).await?;
    let limits = Limits { line: MAX_LINE_BYTES, reply: MAX_TEXT_BYTES };
    let mut paging = Paging::new(Wanted::Pages(pages.to_vec()), Reads::AsFarAsNeeded, page_lines, limits);
    let streamed = blob.stream(worktree, |chunk| paging.feed(chunk)).await?;
    let read = paging.finish(&streamed);
    let (sent, last) = (read.pages.len(), pages.iter().max().copied().unwrap_or(0));
    let said = || format!("read {sent} of {} pages of {path} at {commit}, up to page {last}", pages.len());
    logged(true, &said, streamed, started);
    Ok(FilePages { pages: read.pages })
}

/// Says what a read did, and how long it took: where it's seen without asking for more when it's of a file too big to
/// send whole, which is what's read in pages, or when it was slow, and otherwise only when asked for.
fn logged(big: bool, what: &dyn Fn() -> String, streamed: Streamed, started: Instant) {
    let ms = started.elapsed().as_millis();
    let (bytes, stopped) = (streamed.bytes, streamed.stopped);
    if big || started.elapsed() >= SLOW {
        info!(ms, streamed = bytes, stopped_early = stopped, "{}", what());
    } else {
        debug!(ms, streamed = bytes, stopped_early = stopped, "{}", what());
    }
}

/// Which pages are kept.
enum Wanted {
    All,
    /// In any order.
    Pages(Vec<u32>),
}

/// How far a file is read.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Reads {
    /// To its end, to count its lines and check it's text.
    ToTheEnd,
    /// Only as far as the last page wanted.
    AsFarAsNeeded,
}

/// How much is sent: the most bytes of the file that a line is sent with, and the most a reply's pages can take.
#[derive(Clone, Copy)]
struct Limits {
    line: usize,
    reply: u64,
}

/// The pages wanted of a file that's given to it a chunk at a time, and how many lines it has.
struct Paging {
    page_lines: u32,
    limits: Limits,
    all: bool,
    /// In order, and the next one to read.
    wanted: Vec<u32>,
    next: usize,
    reads: Reads,
    /// The number of the line the next byte is in.
    line: u32,
    /// What's kept of the line the next byte is in, how long it is so far, and whether its last byte so far is a
    /// carriage return, which is part of its ending if a line feed follows.
    kept: Vec<u8>,
    length: u64,
    return_last: bool,
    page: Vec<PageLine>,
    pages: Vec<FilePage>,
    sent: u64,
    /// The last byte so far, which says whether the last line has an ending, so one without is still counted.
    last_byte: Option<u8>,
    /// The file's first bytes, which say whether it's text.
    start: Vec<u8>,
}

struct Paged {
    pages: Vec<FilePage>,
    lines: u32,
    text: bool,
}

impl Paging {
    fn new(wanted: Wanted, reads: Reads, page_lines: u32, limits: Limits) -> Self {
        let (all, mut wanted) = match wanted {
            Wanted::All => (true, Vec::new()),
            Wanted::Pages(pages) => (false, pages),
        };
        wanted.sort_unstable();
        wanted.dedup();
        Self {
            page_lines,
            limits,
            all,
            wanted,
            next: 0,
            reads,
            line: 1,
            kept: Vec::new(),
            length: 0,
            return_last: false,
            page: Vec::new(),
            pages: Vec::new(),
            sent: 0,
            last_byte: None,
            start: Vec::new(),
        }
    }

    /// The first line of the next page wanted, which is the line the next byte is in when every page is.
    fn next_start(&self) -> Option<u32> {
        if self.all {
            return Some(self.line);
        }
        self.wanted.get(self.next).map(|page| page.saturating_mul(self.page_lines).saturating_add(1))
    }

    fn feed(&mut self, chunk: &[u8]) -> ControlFlow<()> {
        let Some(last) = chunk.last() else { return ControlFlow::Continue(()) };
        self.last_byte = Some(*last);
        if self.reads == Reads::ToTheEnd && self.start.len() < SNIFF_BYTES {
            self.start.extend_from_slice(&chunk[..chunk.len().min(SNIFF_BYTES - self.start.len())]);
            if self.start.len() == SNIFF_BYTES && !is_text(&self.start) {
                return ControlFlow::Break(());
            }
        }
        let mut rest = chunk;
        while !rest.is_empty() {
            let Some(start) = self.next_start() else {
                if self.reads == Reads::AsFarAsNeeded {
                    return ControlFlow::Break(());
                }
                let endings = memchr::memchr_iter(b'\n', rest).count();
                self.line = self.line.saturating_add(u32::try_from(endings).unwrap_or(u32::MAX));
                return ControlFlow::Continue(());
            };
            if self.line < start {
                rest = self.skip(rest, start);
                continue;
            }
            let Some(ix) = memchr::memchr(b'\n', rest) else {
                self.keep(rest);
                break;
            };
            self.keep(&rest[..ix]);
            rest = &rest[ix + 1..];
            if self.end_line().is_break() {
                return ControlFlow::Break(());
            }
        }
        ControlFlow::Continue(())
    }

    /// What's left of `rest` once the lines before line `start` are passed over.
    fn skip<'a>(&mut self, rest: &'a [u8], start: u32) -> &'a [u8] {
        let (needed, mut passed) = (start - self.line, 0);
        for ix in memchr::memchr_iter(b'\n', rest) {
            passed += 1;
            if passed == needed {
                self.line = start;
                return &rest[ix + 1..];
            }
        }
        self.line += passed;
        &[]
    }

    fn keep(&mut self, part: &[u8]) {
        let Some(last) = part.last() else { return };
        let room = self.limits.line.saturating_sub(self.kept.len());
        self.kept.extend_from_slice(&part[..part.len().min(room)]);
        self.length += u64::try_from(part.len()).unwrap_or(u64::MAX);
        self.return_last = *last == b'\r';
    }

    fn end_line(&mut self) -> ControlFlow<()> {
        let (mut kept, mut length) = (std::mem::take(&mut self.kept), std::mem::take(&mut self.length));
        if std::mem::take(&mut self.return_last) {
            // The carriage return of a line ending isn't part of the line, whether or not it was kept.
            length -= 1;
            kept.truncate(usize::try_from(length).unwrap_or(usize::MAX));
        }
        self.page.push(page_line(kept, length));
        self.line = self.line.saturating_add(1);
        if self.page.len() == usize::try_from(self.page_lines).unwrap_or(usize::MAX) {
            return self.end_page();
        }
        ControlFlow::Continue(())
    }

    /// Keeps the page just read, unless it doesn't fit beside those kept already, when no more are kept.
    fn end_page(&mut self) -> ControlFlow<()> {
        let lines = std::mem::take(&mut self.page);
        let index = (self.line - 2) / self.page_lines;
        let size = |line: &PageLine| u64::try_from(line.text.len()).unwrap_or(u64::MAX) + LINE_BYTES;
        let bytes: u64 = lines.iter().map(size).sum();
        if !self.all {
            self.next += 1;
        }
        if !self.pages.is_empty() && self.sent + bytes > self.limits.reply {
            (self.all, self.next) = (false, self.wanted.len());
            return match self.reads {
                Reads::ToTheEnd => ControlFlow::Continue(()),
                Reads::AsFarAsNeeded => ControlFlow::Break(()),
            };
        }
        self.sent += bytes;
        self.pages.push(FilePage { index, lines });
        ControlFlow::Continue(())
    }

    fn finish(mut self, streamed: &Streamed) -> Paged {
        if !streamed.stopped && self.last_byte.is_some_and(|byte| byte != b'\n') {
            if self.next_start().is_some_and(|start| self.line >= start) {
                // A carriage return at the very end is left out too, as git's patch leaves it out of the line.
                let _ = self.end_line();
            } else {
                self.line = self.line.saturating_add(1);
            }
        }
        if !self.page.is_empty() {
            let _ = self.end_page();
        }
        // Its first bytes say whether it's text, however many there are, and they're all it has when it's short.
        let text = is_text(&self.start);
        Paged { pages: self.pages, lines: self.line - 1, text }
    }
}

/// A line, from what was kept of it and how long it is, cut where a character ends when only its start was kept.
fn page_line(mut kept: Vec<u8>, length: u64) -> PageLine {
    let cut = (u64::try_from(kept.len()).unwrap_or(u64::MAX) < length).then_some(length);
    if cut.is_some() {
        kept.truncate(whole_characters(&kept));
    }
    PageLine { text: String::from_utf8_lossy(&kept).into_owned(), cut }
}

/// How far into `bytes` the last character that's there whole ends, leaving out one that was cut, however the bytes
/// before it read.
fn whole_characters(bytes: &[u8]) -> usize {
    // A character in UTF-8 is at most four bytes, and only its first isn't `10xxxxxx`.
    let Some(first) = bytes.iter().rev().take(4).position(|byte| byte & 0xC0 != 0x80) else { return bytes.len() };
    let (at, lead) = (bytes.len() - 1 - first, bytes[bytes.len() - 1 - first]);
    let needs = match lead {
        0xF0..=0xF7 => 4,
        0xE0..=0xEF => 3,
        0xC0..=0xDF => 2,
        _ => 1,
    };
    if bytes.len() - at < needs { at } else { bytes.len() }
}

/// Whether a file that starts with `start` is text: not an image, and without a NUL byte, as git decides.
fn is_text(start: &[u8]) -> bool {
    !MediaType::sniff(start).is_image() && !start.contains(&0)
}
