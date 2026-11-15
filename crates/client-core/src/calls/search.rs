//! Searching the workspace, by what's in files or by their names, and what the search found. Claude's `Grep` lists
//! matching files by default, or each matching line as `path:number:text`, and its `Glob` lists paths. A typed search
//! says what it found, file by file.

use protocol::capability::calls::CallOutput;
use protocol::capability::calls::output::{self, FoundLine};
use serde_json::Value;

use super::{count, str_field};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Search<'a> {
    pub pattern: &'a str,
    /// Where it looked, when it was told.
    pub path: Option<&'a str>,
    pub looks: Looks,
}

/// What a search matches its pattern against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Looks {
    /// What's in each file, as `Grep` does.
    InFiles,
    /// Each file's name, as `Glob` does.
    ForNames,
}

/// What a search found.
#[derive(Debug, PartialEq, Eq)]
pub enum Found<'a> {
    /// Matching lines, by file.
    Lines(Vec<FileLines<'a>>),
    /// Files, without their lines.
    Files(Vec<&'a str>),
    Nothing,
    /// How many matches, in how many files, without the matches themselves.
    Count {
        matches: u64,
        files: u64,
    },
    /// Something else, such as matches with lines of context around them, kept as it came so nothing is lost.
    Raw(&'a str),
}

#[derive(Debug, PartialEq, Eq)]
pub struct FileLines<'a> {
    pub path: &'a str,
    pub lines: Vec<(u32, &'a str)>,
}

impl<'a> Search<'a> {
    pub(super) fn read(input: &'a Value, looks: Looks) -> Option<Self> {
        Some(Self { pattern: str_field(input, "pattern")?, path: str_field(input, "path"), looks })
    }

    /// What its line says it did.
    pub fn title(&self) -> &'static str {
        self.looks.title()
    }
}

impl Looks {
    /// What a search that looks this way is called.
    pub(crate) fn title(self) -> &'static str {
        match self {
            Looks::InFiles => "Search",
            Looks::ForNames => "Find files",
        }
    }
}

impl<'a> Found<'a> {
    /// What a search sent back says it found: typed, as `output` says it, or as its `text` says it, for one recorded
    /// as Claude's words, or what a command that only searches printed.
    pub fn of(text: &'a str, output: Option<&'a CallOutput>) -> Self {
        let Some(output) = output else { return Self::read(text) };
        match output {
            CallOutput::Found(output::Found::Lines { lines, .. }) => Self::lines(lines),
            CallOutput::Found(output::Found::Files(paths)) | CallOutput::Paths(paths) if paths.paths.is_empty() => {
                Found::Nothing
            }
            CallOutput::Found(output::Found::Files(paths)) | CallOutput::Paths(paths) => {
                Found::Files(paths.paths.iter().map(String::as_str).collect())
            }
            CallOutput::Found(output::Found::Count { matches: 0, .. }) => Found::Nothing,
            CallOutput::Found(output::Found::Count { matches, files }) => {
                Found::Count { matches: *matches, files: *files }
            }
            CallOutput::Command(_) => Self::printed(text),
            // Words, as matches with lines around them are recorded, or what this build doesn't know.
            CallOutput::Found(output::Found::Unknown)
            | CallOutput::Lines(_)
            | CallOutput::Image { .. }
            | CallOutput::Document { .. }
            | CallOutput::Changed { .. }
            | CallOutput::Page { .. }
            | CallOutput::WebResults { .. }
            | CallOutput::Report { .. }
            | CallOutput::Mcp { .. }
            | CallOutput::Words { .. }
            | CallOutput::Encrypted
            | CallOutput::Backgrounded { .. }
            | CallOutput::Unknown => {
                if text.trim().is_empty() {
                    Found::Nothing
                } else {
                    Found::Raw(text)
                }
            }
        }
    }

    /// Matching lines, as a typed search says them.
    fn lines(lines: &'a [FoundLine]) -> Self {
        Self::grouped(lines.iter().map(|line| (line.path.as_str(), line.line, line.text.as_str())))
    }

    /// Matching lines, each its file's path, its number and its text, grouped by file in the order they came, or
    /// nothing.
    fn grouped(matches: impl IntoIterator<Item = (&'a str, u32, &'a str)>) -> Self {
        let mut files: Vec<FileLines> = Vec::new();
        for (path, number, line) in matches {
            match files.last_mut() {
                Some(file) if file.path == path => file.lines.push((number, line)),
                _ => files.push(FileLines { path, lines: vec![(number, line)] }),
            }
        }
        if files.is_empty() { Found::Nothing } else { Found::Lines(files) }
    }

    /// What a command that only searches printed: its matching lines, as ripgrep prints them, or else what it printed,
    /// as it came.
    fn printed(text: &'a str) -> Self {
        let lines = said(text);
        if lines.is_empty() {
            return Found::Nothing;
        }
        let matched: Option<Vec<_>> = lines.iter().map(|line| matched_line(line)).collect();
        matched.map_or(Found::Raw(text), Self::grouped)
    }

    /// What a search said it found, in words, as Claude's searches say it: its matching lines as ripgrep prints them,
    /// files under a count of them or as their whole paths, or that it found none.
    fn read(text: &'a str) -> Self {
        let lines = said(text);
        if lines.iter().all(|line| line.starts_with("No ") && line.ends_with(" found")) {
            return Found::Nothing;
        }
        let printed = Self::printed(text);
        if !matches!(printed, Found::Raw(_)) {
            return printed;
        }
        // `Grep` lists files under `Found 3 files`, and `Glob` lists their whole paths. Either may end with a note in
        // brackets, such as that the list was cut short.
        let listed: Vec<&str> = lines.into_iter().filter(|line| !line.starts_with('(')).collect();
        match listed.split_first() {
            Some((first, files)) if is_count_of_files(first) => Found::Files(files.to_vec()),
            Some(_) if listed.iter().all(|line| line.starts_with('/')) => Found::Files(listed),
            _ => printed,
        }
    }

    /// How much it found, in a few words.
    pub fn summary(&self) -> String {
        match self {
            Found::Lines(files) => matches_in(files.iter().map(|file| file.lines.len()).sum(), files.len()),
            Found::Files(files) => count(files.len(), "file", "files"),
            Found::Nothing => "Nothing found".to_string(),
            Found::Count { matches, files } => {
                let size = |n: &u64| usize::try_from(*n).unwrap_or(usize::MAX);
                matches_in(size(matches), size(files))
            }
            Found::Raw(text) => count(text.lines().count(), "line", "lines"),
        }
    }
}

/// How many matches it found in how many files, such as `3 matches in 2 files`.
fn matches_in(matches: usize, files: usize) -> String {
    format!("{} in {}", count(matches, "match", "matches"), count(files, "file", "files"))
}

/// The lines of `text` with anything on them, without what trails them.
fn said(text: &str) -> Vec<&str> {
    text.lines().map(str::trim_end).filter(|line| !line.is_empty()).collect()
}

/// `Found 3 files`, or `Found 1 file`.
fn is_count_of_files(line: &str) -> bool {
    let words: Vec<&str> = line.split(' ').collect();
    matches!(words.as_slice(), ["Found", n, "file" | "files"] if n.parse::<u32>().is_ok())
}

/// `path:number:text`, as ripgrep prints a match.
fn matched_line(line: &str) -> Option<(&str, u32, &str)> {
    let (path, rest) = line.split_once(':')?;
    let (number, text) = rest.split_once(':')?;
    Some((path, number.parse().ok()?, text))
}

#[cfg(test)]
mod tests {
    use protocol::capability::calls::CallOutput;
    use protocol::capability::calls::output::{self, Ended, FoundLine, Paths};
    use protocol::fixtures;

    use super::super::typed;
    use super::{FileLines, Found};

    /// Whether what a typed search found, read from what came back for it as a client is given it, is `expected`, and
    /// says `summary` on its line, where that's given.
    fn found(output: &CallOutput, expected: &Found, summary: Option<&str>) {
        let text = typed::output(output);
        let found = Found::of(&text, Some(output));
        assert_eq!(&found, expected, "{output:?}");
        if let Some(summary) = summary {
            assert_eq!(found.summary(), summary);
        }
    }

    #[test]
    fn a_typed_search_says_what_it_found_file_by_file() {
        let line = |path: &str, line, text: &str| FoundLine { path: path.into(), line, text: text.into() };
        let lines =
            vec![line("src/a.rs", 3, "fn a() {"), line("src/a.rs", 9, "fn b() {"), line("src/c.rs", 1, "fn c() {")];
        let grouped = Found::Lines(vec![
            FileLines { path: "src/a.rs", lines: vec![(3, "fn a() {"), (9, "fn b() {")] },
            FileLines { path: "src/c.rs", lines: vec![(1, "fn c() {")] },
        ]);

        found(
            &CallOutput::Found(output::Found::Lines { lines, truncated: false }),
            &grouped,
            Some("3 matches in 2 files"),
        );
        found(&CallOutput::Found(output::Found::Lines { lines: Vec::new(), truncated: false }), &Found::Nothing, None);
    }

    #[test]
    fn a_typed_search_s_files_or_count_are_what_it_found() {
        let paths =
            |paths: &[&str]| Paths { paths: paths.iter().map(|path| (*path).into()).collect(), truncated: false };

        found(
            &CallOutput::Found(output::Found::Files(paths(&["a.rs", "b.rs"]))),
            &Found::Files(vec!["a.rs", "b.rs"]),
            None,
        );
        found(&CallOutput::Paths(paths(&["src/a.rs"])), &Found::Files(vec!["src/a.rs"]), Some("1 file"));
        found(&CallOutput::Paths(paths(&[])), &Found::Nothing, None);
        let counted = Found::Count { matches: 3, files: 2 };
        found(
            &CallOutput::Found(output::Found::Count { matches: 3, files: 2 }),
            &counted,
            Some("3 matches in 2 files"),
        );
        found(&CallOutput::Found(output::Found::Count { matches: 0, files: 0 }), &Found::Nothing, None);
    }

    /// Matches with lines around them are recorded as words, which are kept as they came.
    #[test]
    fn a_typed_search_said_in_words_is_kept_as_it_came() {
        let context = "src/a.rs:10:let a = 1;\nsrc/a.rs-11-let b = 2;";

        found(&CallOutput::Words { text: context.into() }, &Found::Raw(context), Some("2 lines"));
        found(&CallOutput::Words { text: "\n".into() }, &Found::Nothing, None);
    }

    /// What a command that only searches printed is its matching lines where it printed them as ripgrep does, and
    /// otherwise what it printed, as it came, even where Claude's search would have said files or nothing that way.
    #[test]
    fn what_a_command_that_only_searches_printed_is_read_as_ripgrep_prints_it() {
        let printed = |text: &str| fixtures::command_output(text, Some(0), Ended::Exited);
        let matched = vec![FileLines { path: "src/a.rs", lines: vec![(3, "fn a() {")] }];

        found(&printed("src/a.rs:3:fn a() {\n"), &Found::Lines(matched), None);
        found(&printed("/w/a.rs\n/w/b.rs\n"), &Found::Raw("/w/a.rs\n/w/b.rs\n"), Some("2 lines"));
        found(&printed("No matches found\n"), &Found::Raw("No matches found\n"), None);
        found(&printed("src/a.rs:fn a() {\n"), &Found::Raw("src/a.rs:fn a() {\n"), None);
        found(&printed(""), &Found::Nothing, None);
    }

    #[test]
    fn matching_lines_are_grouped_by_file() {
        let text =
            "/w/signal.rs:15:Self::Clear => Self::Caution,\n/w/signal.rs:16:Self::Caution\n/w/train.rs:53:if a {";

        let found = Found::read(text);

        assert_eq!(
            found,
            Found::Lines(vec![
                FileLines {
                    path: "/w/signal.rs",
                    lines: vec![(15, "Self::Clear => Self::Caution,"), (16, "Self::Caution")]
                },
                FileLines { path: "/w/train.rs", lines: vec![(53, "if a {")] },
            ])
        );
        assert_eq!(found.summary(), "3 matches in 2 files");
    }

    #[test]
    fn a_list_of_files_leaves_out_what_s_said_about_it() {
        let found = Found::read("Found 2 files\n/w/signal.rs\n/w/train.rs\n");

        assert_eq!(found, Found::Files(vec!["/w/signal.rs", "/w/train.rs"]));
        assert_eq!(found.summary(), "2 files");
    }

    #[test]
    fn a_list_of_files_can_have_relative_paths_under_its_count() {
        assert_eq!(Found::read("Found 1 file\nsrc/a b.rs"), Found::Files(vec!["src/a b.rs"]));
        assert_eq!(Found::read("/w/a.rs\n(Results are truncated.)"), Found::Files(vec!["/w/a.rs"]));
    }

    #[test]
    fn matches_with_context_are_kept_as_they_came() {
        let text = "src/a.rs:10:let a = 1;\nsrc/a.rs-11-let b = 2;\n--\nsrc/b.rs:3:let c = 3;";

        let found = Found::read(text);

        assert_eq!(found, Found::Raw(text));
        assert_eq!(found.summary(), "4 lines");
    }

    #[test]
    fn finding_nothing_says_so() {
        assert_eq!(Found::read("No matches found"), Found::Nothing);
        assert_eq!(Found::read("No files found"), Found::Nothing);
        assert_eq!(Found::read(""), Found::Nothing);
    }
}
