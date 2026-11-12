//! Reading git's patch: each file's headers, then its hunks, each counted out by the line numbers its `@@` line gives.
//! A hunk's lines are taken by those counts, so a removed line that reads like a header, such as `--- a/x` from
//! removing `-- a/x`, stays a line. What git says that the diff doesn't use, such as a file's mode, is passed over.

use gpui_kit::SharedString;

use super::paths::{Prefixes, decode, git_paths, header_path};
use super::{FilePatch, FileStatus, Hunk, LineKind, PatchLine};

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("line {line}: {problem}")]
pub struct PatchError {
    /// Counted from 1.
    pub line: usize,
    pub problem: Problem,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Problem {
    #[error("a hunk's `@@` line can't be read")]
    HunkHeader,
    #[error("a hunk comes before its file's `---` and `+++` lines")]
    HunkWithoutFile,
    #[error("a hunk ends before the lines it counts")]
    ShortHunk,
    #[error("a hunk starts before the one above it ends")]
    Unordered,
    #[error("a file has no path on either side")]
    NoSides,
}

/// Every file `patch` changes, in the order it gives them.
pub fn parse(patch: &str) -> Result<Vec<FilePatch>, PatchError> {
    let lines: Vec<&str> = patch.lines().collect();
    let mut files = Vec::new();
    let mut file: Option<Building> = None;
    // In a merge's diff against both its parents, which has no side to call old, so it's passed over to the next file.
    let mut merged = false;
    let mut ix = 0;
    while ix < lines.len() {
        let line = lines[ix];
        let fail = |problem| PatchError { line: ix + 1, problem };
        if let Some(header) = line.strip_prefix("diff --git ") {
            files.extend(file.take().map(Building::finish).transpose().map_err(fail)?);
            file = Some(Building::from_git(header));
            merged = false;
        } else if line.starts_with("diff --cc ") || line.starts_with("diff --combined ") {
            files.extend(file.take().map(Building::finish).transpose().map_err(fail)?);
            merged = true;
        } else if merged {
        } else if let Some(old) = line.strip_prefix("--- ")
            && let Some(new) = lines.get(ix + 1).and_then(|next| next.strip_prefix("+++ "))
        {
            // A plain unified diff starts each file here, with no `diff` line before it.
            if file.as_ref().is_none_or(|file| file.headers) {
                files.extend(file.take().map(Building::finish).transpose().map_err(fail)?);
                file = Some(Building::default());
            }
            if let Some(file) = &mut file {
                file.headers(old, new);
            }
            ix += 1;
        } else if line.starts_with("@@") {
            let file = file.as_mut().filter(|file| file.headers).ok_or(fail(Problem::HunkWithoutFile))?;
            let (hunk, next) = hunk(&lines, ix)?;
            if file.hunks.last().is_some_and(|last| hunk.old_start < last.old_end() || hunk.new_start < last.new_end())
            {
                return Err(fail(Problem::Unordered));
            }
            file.hunks.push(hunk);
            ix = next;
            continue;
        } else if let Some(file) = &mut file {
            file.extended(line);
        }
        ix += 1;
    }
    files.extend(file.map(Building::finish).transpose().map_err(|problem| PatchError { line: ix, problem })?);
    Ok(files)
}

/// Whether a file was moved, or copied, to where it is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Moved {
    #[default]
    No,
    Renamed,
    Copied,
}

/// A file as its headers are read.
#[derive(Default)]
struct Building {
    old: Option<String>,
    new: Option<String>,
    prefixes: Option<Prefixes>,
    moved: Moved,
    binary: bool,
    /// Its `---` and `+++` lines have been read, which its hunks come after.
    headers: bool,
    hunks: Vec<Hunk>,
}

impl Building {
    fn from_git(header: &str) -> Self {
        let paths = git_paths(header);
        let prefixes = paths.as_ref().map(|(_, _, prefixes)| *prefixes);
        let (old, new) = paths.map(|(old, new, _)| (Some(old), Some(new))).unwrap_or_default();
        Self { old, new, prefixes, ..Self::default() }
    }

    /// Takes the paths from the `---` and `+++` lines, which say which side is missing. Without a `diff --git` line
    /// to say what the prefixes are, they're git's own when both paths have them.
    fn headers(&mut self, old: &str, new: &str) {
        let (old_prefix, new_prefix) = self.prefixes.unwrap_or_else(|| {
            let conventional = |path: &str, prefix| path == "/dev/null" || path.starts_with(prefix);
            if conventional(old, "a/") && conventional(new, "b/") { ("a/", "b/") } else { ("", "") }
        });
        self.old = header_path(old, old_prefix);
        self.new = header_path(new, new_prefix);
        self.headers = true;
    }

    /// One of the lines between a file's `diff` line and its hunks.
    fn extended(&mut self, line: &str) {
        if let Some(path) = line.strip_prefix("rename from ") {
            (self.old, self.moved) = (Some(decode(path)), Moved::Renamed);
        } else if let Some(path) = line.strip_prefix("rename to ") {
            (self.new, self.moved) = (Some(decode(path)), Moved::Renamed);
        } else if let Some(path) = line.strip_prefix("copy from ") {
            (self.old, self.moved) = (Some(decode(path)), Moved::Copied);
        } else if let Some(path) = line.strip_prefix("copy to ") {
            (self.new, self.moved) = (Some(decode(path)), Moved::Copied);
        } else if line.starts_with("new file mode ") {
            self.old = None;
        } else if line.starts_with("deleted file mode ") {
            self.new = None;
        } else if line.starts_with("Binary files ") || line == "GIT binary patch" {
            self.binary = true;
        }
    }

    fn finish(self) -> Result<FilePatch, Problem> {
        let (old_path, new_path) = (self.old.map(SharedString::from), self.new.map(SharedString::from));
        let path = new_path.clone().or_else(|| old_path.clone()).ok_or(Problem::NoSides)?;
        let status = match (&old_path, &new_path) {
            (None, _) => FileStatus::Added,
            (_, None) => FileStatus::Removed,
            _ => match self.moved {
                Moved::No => FileStatus::Modified,
                Moved::Renamed => FileStatus::Renamed,
                Moved::Copied => FileStatus::Copied,
            },
        };
        let count = |kind| self.hunks.iter().flat_map(|hunk| &hunk.lines).filter(|line| line.kind == kind).count();
        let (added, removed) = (count(LineKind::Added), count(LineKind::Removed));
        Ok(FilePatch {
            path,
            old_path,
            new_path,
            status,
            binary: self.binary,
            hunks: self.hunks.into(),
            added,
            removed,
        })
    }
}

/// The hunk whose `@@` line is `lines[at]`, and where the next line after it is.
fn hunk(lines: &[&str], at: usize) -> Result<(Hunk, usize), PatchError> {
    let (old_start, old_count, new_start, new_count) =
        hunk_header(lines[at]).ok_or(PatchError { line: at + 1, problem: Problem::HunkHeader })?;
    let (mut old_left, mut new_left) = (old_count, new_count);
    let mut taken = Vec::new();
    let mut ix = at + 1;
    while old_left > 0 || new_left > 0 {
        let short = PatchError { line: ix + 1, problem: Problem::ShortHunk };
        let line = *lines.get(ix).ok_or(short)?;
        ix += 1;
        // An editor or a mail program can take the space off an empty line both sides have, and git takes it back.
        let (kind, text) = match line.as_bytes().first() {
            None => (LineKind::Same, ""),
            Some(b' ') => (LineKind::Same, &line[1..]),
            Some(b'-') => (LineKind::Removed, &line[1..]),
            Some(b'+') => (LineKind::Added, &line[1..]),
            Some(b'\\') => {
                mark_no_newline(&mut taken);
                continue;
            }
            Some(_) => return Err(short),
        };
        if kind != LineKind::Added {
            old_left = old_left.checked_sub(1).ok_or(short)?;
        }
        if kind != LineKind::Removed {
            new_left = new_left.checked_sub(1).ok_or(short)?;
        }
        let text = text.strip_suffix('\r').unwrap_or(text);
        taken.push(PatchLine { kind, text: text.to_string().into(), no_newline: false });
    }
    // A last line without a line ending says so on the line after it.
    while lines.get(ix).is_some_and(|line| line.starts_with('\\')) {
        mark_no_newline(&mut taken);
        ix += 1;
    }
    // A side with no lines names the line before where they'd be, so the hunk starts on the one after.
    let start = |start: u32, count: u32| start + u32::from(count == 0);
    let hunk = Hunk { old_start: start(old_start, old_count), new_start: start(new_start, new_count), lines: taken };
    Ok((hunk, ix))
}

fn mark_no_newline(taken: &mut [PatchLine]) {
    if let Some(last) = taken.last_mut() {
        last.no_newline = true;
    }
}

/// The starts and counts of `@@ -12,3 +12,4 @@`, where a count left out is one.
fn hunk_header(line: &str) -> Option<(u32, u32, u32, u32)> {
    let ranges = line.strip_prefix("@@ ")?.split(" @@").next()?;
    let (old, new) = ranges.split_once(' ')?;
    let range = |value: &str| {
        let (start, count) = value.split_once(',').unwrap_or((value, "1"));
        Some((start.parse().ok()?, count.parse().ok()?))
    };
    let ((old_start, old_count), (new_start, new_count)) =
        (range(old.strip_prefix('-')?)?, range(new.strip_prefix('+')?)?);
    Some((old_start, old_count, new_start, new_count))
}
