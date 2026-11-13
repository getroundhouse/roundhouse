//! Calls that read, write or edit one file, and what Claude sends back for a read: each line after its number.

use serde_json::Value;

use super::str_field;

pub struct ReadFile<'a> {
    pub path: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteFile<'a> {
    pub path: &'a str,
    /// All of what the file holds afterwards.
    pub content: &'a str,
}

/// One edit or several, to one file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditFile<'a> {
    pub path: &'a str,
    /// What each edit replaced, and with what, in order.
    pub edits: Vec<(&'a str, &'a str)>,
}

impl<'a> ReadFile<'a> {
    pub(super) fn read(input: &'a Value) -> Option<Self> {
        Some(Self { path: str_field(input, "file_path")? })
    }
}

impl<'a> WriteFile<'a> {
    pub(super) fn read(input: &'a Value) -> Option<Self> {
        Some(Self { path: str_field(input, "file_path")?, content: str_field(input, "content")? })
    }
}

impl<'a> EditFile<'a> {
    /// `Edit` has one edit, and `MultiEdit` a list of them.
    pub(super) fn read(input: &'a Value) -> Option<Self> {
        let path = str_field(input, "file_path")?;
        let pair = |edit: &'a Value| Some((str_field(edit, "old_string")?, str_field(edit, "new_string")?));
        let edits = match input.get("edits").and_then(Value::as_array) {
            Some(edits) => edits.iter().map(pair).collect::<Option<Vec<_>>>()?,
            None => vec![pair(input)?],
        };
        Some(Self { path, edits })
    }
}

/// The last part of a path, which is what a line has room for.
pub fn file_name(path: &str) -> &str {
    path.rsplit('/').find(|part| !part.is_empty()).unwrap_or(path)
}

/// The lines of a read, each with its number in the file. Claude puts each line's number in front of it, then a tab,
/// or an arrow in older versions. Whatever follows the numbered lines, such as a note to the agent, isn't the file.
pub fn numbered_lines(text: &str) -> Vec<(u32, &str)> {
    text.lines().map_while(numbered).collect()
}

fn numbered(line: &str) -> Option<(u32, &str)> {
    let line = line.trim_start_matches(' ');
    let digits = line.len() - line.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    let number = line[..digits].parse().ok()?;
    let rest = &line[digits..];
    let text = rest.strip_prefix('\t').or_else(|| rest.strip_prefix('→'))?;
    Some((number, text))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{EditFile, file_name, numbered_lines};

    #[test]
    fn a_read_s_lines_keep_their_numbers_and_leave_out_what_follows() {
        let text =
            "    11\timpl Aspect {\n    12\t    /// Next.\n    13\t\n\n<system-reminder>Not the file</system-reminder>";

        assert_eq!(numbered_lines(text), [(11, "impl Aspect {"), (12, "    /// Next."), (13, "")]);
        assert_eq!(numbered_lines("     1→old style"), [(1, "old style")]);
        assert!(numbered_lines("File does not exist.").is_empty());
    }

    #[test]
    fn several_edits_to_one_file_are_kept_in_order() {
        let input = json!({ "file_path": "/w/a.rs", "edits": [
            { "old_string": "a", "new_string": "b" },
            { "old_string": "c", "new_string": "d" },
        ]});

        let edit = EditFile::read(&input).unwrap();

        assert_eq!(edit.edits, [("a", "b"), ("c", "d")]);
        assert!(EditFile::read(&json!({ "file_path": "/w/a.rs", "old_string": "a" })).is_none());
    }

    #[test]
    fn a_file_is_named_by_its_last_part() {
        assert_eq!(file_name("/workspaces/aspen/src/signal.rs"), "signal.rs");
        assert_eq!(file_name("/workspaces/aspen/src/"), "src");
    }
}
