//! What the file tab shows of a changed file: its patch, and its two sides as they're drawn. A file shows plain from
//! its patch at once, then in colour, and then with the file as it was in the diff's commit, which client-core's
//! `OpenFile` opens through the hub, to fill in what git left out. A small file comes whole when it's opened. One too
//! big to send whole is read a page at a time, as its lines are to show, and each page is coloured as it comes. The
//! sides are made off the main thread, always from one patch, which is what shows with them. The file as it was stays
//! open while the same file shows from the same commit, however the diff loads again.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::sync::Arc;

use client_core::file_text::FileText;
use client_core::{OpenFile, Opening};
use gpui_kit::{AppContext as _, Context, SharedString, Task};
use protocol::model::{CommitId, PageLine};

use super::FileDiff;
use super::sides::{Extent, Run, Sides};
use crate::file_view::HIGHLIGHT_UP_TO;
use crate::patch::{FilePatch, Hunk};

/// Where the file as it was is opened from: the diff's commit, and its path there.
type Base = (CommitId, SharedString);

pub(super) struct Shown {
    pub(super) file: FilePatch,
    pub(super) sides: Arc<Sides>,
    /// `None` for a file the change adds, which wasn't there, or from a worker too old to name its commit.
    base: Option<Base>,
    /// The file as it was, opened or being opened, and the task that hears when it has more to show.
    old: Option<(OpenFile, Task<()>)>,
    /// The sides were made once the file as it was had been opened, or with nothing to open.
    settled: bool,
    /// The pages of the file as it was that have been coloured, by number, which the sides have too when they have it
    /// in part, and are made again with, and those being coloured.
    runs: BTreeMap<u32, Arc<Run>>,
    colouring: BTreeSet<u32>,
}

/// The sides being made, so only the newest are shown, and a diff loaded again with what they're being made from
/// doesn't make them again.
#[derive(Default)]
pub(super) struct Making {
    count: u64,
    /// What the newest are being made from, until they're shown.
    from: Option<(FilePatch, Option<Base>)>,
}

impl FileDiff {
    /// Shows the file as the diff last loaded has it, or nothing once it no longer changes anything.
    pub(super) fn show_file(&mut self, cx: &mut Context<Self>) {
        let review = self.review.read(cx);
        let found = self.path.as_ref().and_then(|path| review.file(path)).filter(|file| !file.is_binary()).cloned();
        let Some(file) = found else {
            self.making = Making { count: self.making.count + 1, from: None };
            self.shown = None;
            self.lay_out_again(cx);
            return;
        };
        let (base, (client, id)) = (review.base_commit.clone().zip(file.old_path().cloned()), review.source());
        let from = Some((file.clone(), base.clone()));
        if self.making.from == from {
            return;
        }
        if let Some(shown) = self.shown.as_mut().filter(|shown| shown.file.path() == file.path() && shown.base == base)
        {
            // One that couldn't be opened is opened again, and taken in once it is.
            if let Some((old, _)) = &shown.old
                && old.reconnect(&client)
            {
                shown.settled = false;
            }
            if self.making.from.is_none() && shown.file == file {
                return;
            }
        } else {
            let old = base.as_ref().map(|(commit, path)| OpenFile::start(&client, &id, commit, path));
            self.shown = Some(Shown::plain(file.clone(), base.clone(), old, cx));
            self.lay_out_again(cx);
        }
        self.making = Making { count: self.making.count + 1, from };
        self.make_sides(file, cx);
    }

    /// Makes the sides off the main thread, from `file` and as much of the file as it was as is open, and shows them
    /// with it if nothing has been shown since.
    fn make_sides(&mut self, file: FilePatch, cx: &mut Context<Self>) {
        let Some(shown) = self.shown.as_ref() else { return };
        let old = shown.old.as_ref().map(|(old, _)| old.read(Opening::clone));
        let settled = !matches!(old, Some(Opening::Opening));
        let (make, language) = (self.making.count, look::language(file.path()).to_string());
        cx.spawn(async move |this, cx| {
            let making = cx.background_spawn(async move {
                let lines: usize = file.hunks().iter().flat_map(Hunk::lines).map(|line| line.text.len()).sum();
                let hunks = (lines <= HIGHLIGHT_UP_TO).then_some(language.as_str());
                let sides = match &old {
                    Some(Opening::Text(text)) if is_whole(text) => {
                        Sides::with_base(&file, &whole(text), Some(&language))
                    }
                    Some(Opening::Text(text)) => Sides::in_part(&file, text.lines(), hunks),
                    _ => Sides::from_patch(&file, hunks),
                };
                (file, sides)
            });
            let (file, sides) = making.await;
            let _ = this.update(cx, |t, cx| {
                if make == t.making.count {
                    t.making.from = None;
                    t.made(file, sides, settled, cx);
                }
            });
        })
        .detach();
    }

    /// Shows sides that were made, with the patch they were made from, and with the pages of the file as it was
    /// coloured since, when they have it in part.
    fn made(&mut self, file: FilePatch, mut sides: Sides, settled: bool, cx: &mut Context<Self>) {
        let Some(shown) = self.shown.as_mut() else { return };
        if matches!(sides.extent, Extent::InPart(_)) {
            for run in shown.runs.values() {
                sides.add(&file, run);
            }
        }
        (shown.file, shown.sides, shown.settled) = (file, Arc::new(sides), settled);
        self.lay_out_again(cx);
        self.old_changed(cx);
    }

    /// Takes in what's new of the file as it was: once it's open, the sides with it, and then each page read of it,
    /// coloured off the main thread.
    fn old_changed(&mut self, cx: &mut Context<Self>) {
        let Some(shown) = self.shown.as_mut() else { return };
        let Some(opening) = shown.old.as_ref().map(|(old, _)| old.read(Opening::clone)) else { return };
        let text = match opening {
            Opening::Opening => return,
            Opening::Text(text) if !shown.settled && is_whole(&text) => {
                // Made again, whole, as long as nothing else is being made, which takes this in too.
                if self.making.from.is_none() {
                    let file = shown.file.clone();
                    self.making =
                        Making { count: self.making.count + 1, from: Some((file.clone(), shown.base.clone())) };
                    self.make_sides(file, cx);
                }
                return;
            }
            Opening::Text(text) => text,
            Opening::NotText | Opening::Unsupported | Opening::Failed => {
                shown.settled = true;
                return;
            }
        };
        let learns = !shown.settled && self.making.from.is_none();
        if learns {
            // The patch is coloured already, so the sides only learn how long the file is.
            Arc::make_mut(&mut shown.sides).extent = Extent::InPart(text.lines());
            shown.settled = true;
        }
        // Pages read while the sides are being made are coloured too, for the sides to be made with.
        let colours = !shown.settled || matches!(shown.sides.extent, Extent::InPart(_));
        if learns {
            self.lay_out_again(cx);
        }
        if colours {
            self.colour_pages(&text, cx);
        }
    }

    /// Colours the pages of the file as it was that haven't been, off the main thread, and adds them to the sides.
    fn colour_pages(&mut self, text: &FileText, cx: &mut Context<Self>) {
        let Some(shown) = self.shown.as_mut() else { return };
        let new: Vec<(u32, Range<u32>, Vec<PageLine>)> = text
            .pages()
            .filter(|(index, ..)| !shown.runs.contains_key(index) && !shown.colouring.contains(index))
            .map(|(index, lines, page)| (index, lines, page.to_vec()))
            .collect();
        if new.is_empty() {
            return;
        }
        shown.colouring.extend(new.iter().map(|(index, ..)| *index));
        let (base, language) = (shown.base.clone(), look::language(shown.file.path()).to_string());
        cx.spawn(async move |this, cx| {
            let colouring = cx.background_spawn(async move {
                let coloured = |(index, lines, page): (u32, Range<u32>, Vec<PageLine>)| {
                    let bytes: usize = page.iter().map(|line: &PageLine| line.text.len()).sum();
                    (index, Run::from_page(&lines, &page, (bytes <= HIGHLIGHT_UP_TO).then_some(&*language)))
                };
                new.into_iter().map(coloured).collect::<Vec<_>>()
            });
            let runs = colouring.await;
            let _ = this.update(cx, |t, cx| t.coloured(base.as_ref(), runs, cx));
        })
        .detach();
    }

    /// Adds pages of the file as it was that were coloured, if it still shows.
    fn coloured(&mut self, base: Option<&Base>, runs: Vec<(u32, Run)>, cx: &mut Context<Self>) {
        let Some(shown) = self.shown.as_mut().filter(|shown| shown.base.as_ref() == base) else { return };
        let in_part = matches!(shown.sides.extent, Extent::InPart(_));
        for (index, run) in runs {
            shown.colouring.remove(&index);
            if in_part {
                Arc::make_mut(&mut shown.sides).add(&shown.file, &run);
            }
            shown.runs.insert(index, Arc::new(run));
        }
        self.lay_out_again(cx);
    }

    /// Asks for the pages of the file as it was that `missing`, lines that are to show, are on, when the sides have
    /// it in part. They show once they've come and been coloured.
    pub(super) fn read_lines(&mut self, missing: &[Range<u32>]) {
        let Some(shown) = self.shown.as_ref().filter(|shown| matches!(shown.sides.extent, Extent::InPart(_))) else {
            return;
        };
        if let Some((old, _)) = &shown.old
            && !missing.is_empty()
        {
            old.want(missing);
        }
    }

    /// Whether the file shows as it will, in colour, and with the file as it was if that could be opened, so what git
    /// left out can be opened.
    #[cfg(feature = "testing")]
    pub fn is_ready(&self) -> bool {
        self.making.from.is_none() && self.shown.as_ref().is_some_and(|shown| shown.settled)
    }

    /// Whether the file as it was has been opened.
    #[cfg(feature = "testing")]
    pub fn has_read_the_base(&self) -> bool {
        let old = self.shown.as_ref().and_then(|shown| shown.old.as_ref());
        old.is_some_and(|(old, _)| old.read(|opening| matches!(opening, Opening::Text(_))))
    }
}

impl Shown {
    /// The patch alone, plain, as it shows before anything's been made, hearing when `old`, the file as it was, has
    /// more to show.
    fn plain(file: FilePatch, base: Option<Base>, old: Option<OpenFile>, cx: &mut Context<FileDiff>) -> Self {
        let old = old.map(|old| {
            let changed = old.changed().clone();
            let hearing = cx.spawn(async move |this, cx| {
                while changed.recv_async().await.is_ok() {
                    if this.update(cx, FileDiff::old_changed).is_err() {
                        return;
                    }
                }
            });
            (old, hearing)
        });
        let settled = old.is_none();
        let sides = Arc::new(Sides::plain(&file));
        Self { file, sides, base, old, settled, runs: BTreeMap::new(), colouring: BTreeSet::new() }
    }
}

/// Whether every line of the file as it was is held as the file has it, so the sides can be made whole with it.
fn is_whole(text: &FileText) -> bool {
    text.is_whole() && !text.has_cut_lines()
}

/// All of a file whose every page is held, a line apart.
fn whole(text: &FileText) -> String {
    text.pages().flat_map(|(_, _, lines)| lines).fold(String::new(), |mut whole, line| {
        whole.push_str(&line.text);
        whole.push('\n');
        whole
    })
}
