//! Searching the workspace, by what's in files or by their names, and what the search found. Claude's `Grep` lists
//! matching files by default, or each matching line as `path:number:text`, and its `Glob` lists paths.

use serde_json::Value;

use super::{count, str_field};

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
        match self.looks {
            Looks::InFiles => "Search",
            Looks::ForNames => "Find files",
        }
    }
}

impl<'a> Found<'a> {
    pub fn read(text: &'a str) -> Self {
        let lines: Vec<&str> = text.lines().map(str::trim_end).filter(|line| !line.is_empty()).collect();
        if lines.is_empty() || lines.iter().all(|line| line.starts_with("No ") && line.ends_with(" found")) {
            return Found::Nothing;
        }
        let matched: Vec<(&str, u32, &str)> = lines.iter().filter_map(|line| matched_line(line)).collect();
        if !matched.is_empty() && matched.len() == lines.len() {
            let mut files: Vec<FileLines> = Vec::new();
            for (path, number, line) in matched {
                match files.last_mut() {
                    Some(file) if file.path == path => file.lines.push((number, line)),
                    _ => files.push(FileLines { path, lines: vec![(number, line)] }),
                }
            }
            return Found::Lines(files);
        }
        // `Grep` lists files under `Found 3 files`, and `Glob` lists their whole paths. Either may end with a note in
        // brackets, such as that the list was cut short.
        let listed: Vec<&str> = lines.iter().copied().filter(|line| !line.starts_with('(')).collect();
        match listed.split_first() {
            Some((first, files)) if is_count_of_files(first) => Found::Files(files.to_vec()),
            Some(_) if listed.iter().all(|line| line.starts_with('/')) => Found::Files(listed),
            _ => Found::Raw(text),
        }
    }

    /// How much it found, in a few words.
    pub fn summary(&self) -> String {
        match self {
            Found::Lines(files) => {
                let lines = files.iter().map(|file| file.lines.len()).sum();
                format!("{} in {}", count(lines, "match", "matches"), count(files.len(), "file", "files"))
            }
            Found::Files(files) => count(files.len(), "file", "files"),
            Found::Nothing => "Nothing found".to_string(),
            Found::Raw(text) => count(text.lines().count(), "line", "lines"),
        }
    }
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
    use super::{FileLines, Found};

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
