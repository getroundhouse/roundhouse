//! What an edit or a write changed, from the `tool_use_result` Claude puts beside a tool's result. Claude sends the
//! change as a structured patch, with the file's own line numbers, and says whether a write made the file.

use protocol::event::{FileChange, Hunk, HunkLine, LineMark};
use serde_json::Value;

use super::str_of;

/// The most lines of a change carried on a result. Each event is stored in the log and sent to every client that
/// shows it, and a change bigger than this is too long to read in a transcript anyway. It's left to the result's
/// text and the call, and the raw line still has it.
const MAX_LINES: usize = 4000;

/// The change in a line's `tool_use_result`, if it says what changed in a file.
pub(super) fn read(line: &Value) -> Option<FileChange> {
    let result = line.get("tool_use_result")?;
    let patch = result.get("structuredPatch")?.as_array()?;
    // A new file's patch is empty, since all of it is in the call.
    if str_of(result, "type") == Some("create") {
        return Some(FileChange::Created);
    }
    let hunks: Vec<Hunk> = patch.iter().map(hunk).collect::<Option<_>>()?;
    let lines: usize = hunks.iter().map(|hunk| hunk.lines.len()).sum();
    (lines <= MAX_LINES).then_some(FileChange::Changed { hunks })
}

fn hunk(value: &Value) -> Option<Hunk> {
    let start = |key| value.get(key).and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok());
    let lines = value.get("lines")?.as_array()?.iter().filter_map(Value::as_str).filter_map(hunk_line).collect();
    Some(Hunk { old_start: start("oldStart")?, new_start: start("newStart")?, lines })
}

/// A line of a unified diff, by its first character. Anything else, such as `\ No newline at end of file`, is about
/// the diff rather than the file, so it's left out.
fn hunk_line(line: &str) -> Option<HunkLine> {
    let mut chars = line.chars();
    let mark = match chars.next()? {
        ' ' => LineMark::Same,
        '-' => LineMark::Removed,
        '+' => LineMark::Added,
        _ => return None,
    };
    Some(HunkLine { mark, text: chars.as_str().to_string() })
}

#[cfg(test)]
mod tests {
    use protocol::event::{FileChange, Hunk, HunkLine, LineMark};
    use serde_json::json;

    use super::read;

    fn marked(mark: LineMark, text: &str) -> HunkLine {
        HunkLine { mark, text: text.to_string() }
    }

    #[test]
    fn an_edit_s_patch_keeps_the_file_s_line_numbers() {
        let line = json!({ "type": "user", "tool_use_result": {
            "filePath": "/w/src/signal.rs",
            "structuredPatch": [{ "oldStart": 27, "oldLines": 3, "newStart": 28, "newLines": 3, "lines": [
                "             Self::Clear => \"clear\",",
                "-            Self::Caution => \"caution\",",
                "+            Self::Caution => \"approach with caution\",",
                "\\ No newline at end of file",
            ]}],
        }});

        let change = read(&line).unwrap();

        assert_eq!(
            change,
            FileChange::Changed {
                hunks: vec![Hunk {
                    old_start: 27,
                    new_start: 28,
                    lines: vec![
                        marked(LineMark::Same, "            Self::Clear => \"clear\","),
                        marked(LineMark::Removed, "            Self::Caution => \"caution\","),
                        marked(LineMark::Added, "            Self::Caution => \"approach with caution\","),
                    ],
                }],
            }
        );
    }

    #[test]
    fn a_write_says_whether_it_made_the_file() {
        let created = json!({ "tool_use_result": { "type": "create", "content": "a\n", "structuredPatch": [] } });
        let replaced = json!({ "tool_use_result": { "type": "update", "content": "a\n", "structuredPatch": [] } });

        assert_eq!(read(&created), Some(FileChange::Created));
        assert_eq!(read(&replaced), Some(FileChange::Changed { hunks: Vec::new() }));
    }

    #[test]
    fn a_result_that_isn_t_about_a_file_has_no_change() {
        let command = json!({ "tool_use_result": { "stdout": "ok", "stderr": "", "interrupted": false } });
        let broken = json!({ "tool_use_result": { "structuredPatch": [{ "lines": ["+a"] }] } });

        assert_eq!(read(&command), None);
        assert_eq!(read(&broken), None, "a hunk that doesn't say where it is can't be placed");
        assert_eq!(read(&json!({ "type": "user" })), None);
    }

    #[test]
    fn a_huge_change_is_left_to_the_result_s_text() {
        let lines: Vec<String> = (0..5000).map(|i| format!("+line {i}")).collect();
        let line = json!({ "tool_use_result": { "structuredPatch": [
            { "oldStart": 1, "newStart": 1, "lines": lines },
        ]}});

        assert_eq!(read(&line), None);
    }
}
