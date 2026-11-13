use gpui_kit::SharedString;
use std::ops::Range;
use std::sync::Arc;

/// One file the diff changes. It has at least one side, and its hunks are in order down the file, apart from each
/// other.
#[derive(Clone, Debug, PartialEq)]
pub struct FilePatch {
    /// Where it is now, or was for one the change removes.
    pub(super) path: SharedString,
    pub(super) old_path: Option<SharedString>,
    pub(super) new_path: Option<SharedString>,
    pub(super) status: FileStatus,
    pub(super) binary: bool,
    /// Shared, so a copy of the file for drawing its row costs nothing, however much it changes.
    pub(super) hunks: Arc<[Hunk]>,
    pub(super) added: usize,
    pub(super) removed: usize,
}

/// How a file changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileStatus {
    Added,
    Removed,
    Modified,
    Renamed,
    Copied,
}

/// A run of the file around a change, as git gives it.
#[derive(Clone, Debug, PartialEq)]
pub struct Hunk {
    pub(super) old_start: u32,
    pub(super) new_start: u32,
    pub(super) lines: Vec<PatchLine>,
}

/// A line of a hunk: one both sides have, or one only the old or the new side has.
#[derive(Clone, Debug, PartialEq)]
pub struct PatchLine {
    pub kind: LineKind,
    /// Without its line ending, and with its tabs as they are.
    pub text: SharedString,
    /// It's the last line of its side, and the file doesn't end with a line ending.
    pub no_newline: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Same,
    Removed,
    Added,
}

/// Lines both sides have that git left out of a file's diff, before a hunk or after the last, by their numbers on the
/// old side, and where they start on the new.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unchanged {
    pub old: Range<u32>,
    pub new_start: u32,
}

impl Unchanged {
    /// The number on the new side of `old`, one of these lines on the old side.
    pub fn to_new(&self, old: u32) -> u32 {
        old - self.old.start + self.new_start
    }

    /// The number on the old side of `new`, if it's one of these lines on the new side.
    pub fn to_old(&self, new: u32) -> Option<u32> {
        let old = (new + self.old.start).checked_sub(self.new_start)?;
        self.old.contains(&old).then_some(old)
    }
}

impl FilePatch {
    /// The lines before each hunk, in order, and then those after the last, up to before line `end` of the old side.
    pub fn unchanged(&self, end: u32) -> Vec<Unchanged> {
        let (mut old, mut new_start) = (1, 1);
        let mut unchanged = Vec::with_capacity(self.hunks.len() + 1);
        for hunk in self.hunks.iter() {
            unchanged.push(Unchanged { old: old..hunk.old_start(), new_start });
            (old, new_start) = (hunk.old_end(), hunk.new_end());
        }
        unchanged.push(Unchanged { old: old..end.max(old), new_start });
        unchanged
    }

    /// Where the file is now, or was for one the change removes. It's what the file is known by.
    pub fn path(&self) -> &SharedString {
        &self.path
    }

    /// Where it was before the change, or `None` for one the change adds.
    pub fn old_path(&self) -> Option<&SharedString> {
        self.old_path.as_ref()
    }

    /// Where it is after the change, or `None` for one the change removes.
    pub fn new_path(&self) -> Option<&SharedString> {
        self.new_path.as_ref()
    }

    pub fn status(&self) -> FileStatus {
        self.status
    }

    /// Whether git says only that it differs, since it isn't text.
    pub fn is_binary(&self) -> bool {
        self.binary
    }

    pub fn hunks(&self) -> &[Hunk] {
        &self.hunks
    }

    /// How many lines it adds.
    pub fn added(&self) -> usize {
        self.added
    }

    /// How many lines it removes.
    pub fn removed(&self) -> usize {
        self.removed
    }
}

impl Hunk {
    /// The first line it covers on the old side, or for one with no lines there, the line after where they'd be.
    pub fn old_start(&self) -> u32 {
        self.old_start
    }

    /// As [`Self::old_start`], on the new side.
    pub fn new_start(&self) -> u32 {
        self.new_start
    }

    /// The line after the last it covers on the old side.
    pub fn old_end(&self) -> u32 {
        self.old_start + self.count(LineKind::Removed)
    }

    /// The line after the last it covers on the new side.
    pub fn new_end(&self) -> u32 {
        self.new_start + self.count(LineKind::Added)
    }

    pub fn lines(&self) -> &[PatchLine] {
        &self.lines
    }

    /// How many lines it has on the side that has `kind`'s lines, with those both sides have.
    pub(super) fn count(&self, kind: LineKind) -> u32 {
        let lines = self.lines.iter().filter(|line| line.kind == kind || line.kind == LineKind::Same).count();
        u32::try_from(lines).unwrap_or(u32::MAX)
    }
}
