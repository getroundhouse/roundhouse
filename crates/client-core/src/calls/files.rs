//! Calls that read, write or edit one file, and what Claude sends back for a read: each line after its number.

use protocol::capability::calls::CallOutput;

pub struct ReadFile<'a> {
    pub path: &'a str,
    /// The lines the call asked for, where it named them, rather than reading from the top.
    pub lines: Option<&'a protocol::capability::calls::call::LineRange>,
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

/// The last part of a path, which is what a line has room for.
pub fn file_name(path: &str) -> &str {
    path.rsplit('/').find(|part| !part.is_empty()).unwrap_or(path)
}

/// The lines a read sent back, each with its number in the file, as `output` says them. None for anything else, such as
/// what a command that reads printed, which has no numbers.
pub fn lines(output: &CallOutput) -> Vec<(u32, &str)> {
    let CallOutput::Lines(lines) = output else { return Vec::new() };
    (lines.first..).zip(lines.lines.iter().map(String::as_str)).collect()
}

#[cfg(test)]
mod tests {
    use protocol::capability::calls::call::{Edit, Replace, Replaces};
    use protocol::capability::calls::output::Lines;
    use protocol::capability::calls::{Call, CallOutput};

    use super::super::{Recorded, ToolCall};
    use super::{file_name, lines};

    /// A read's lines are numbered from where the read started, and anything else has no numbers to read.
    #[test]
    fn a_read_s_lines_are_numbered_from_where_it_started() {
        let read = CallOutput::Lines(Lines { first: 28, lines: vec!["a".into(), "b".into()], total: None });
        let printed = CallOutput::Words { text: "    28\ta".into() };

        assert_eq!(lines(&read), [(28, "a"), (29, "b")]);
        assert!(lines(&printed).is_empty(), "only a read's lines have numbers");
    }

    #[test]
    fn several_edits_to_one_file_are_kept_in_order() {
        let replace = |old: &str, new: &str| Replace { old: old.into(), new: new.into(), replaces: Replaces::One };
        let call = Call::Edit(Edit { path: "/w/a.rs".into(), edits: vec![replace("a", "b"), replace("c", "d")] });

        let ToolCall::Edit(edit) = ToolCall::read(Recorded::Typed(&call)) else { panic!("not drawn as an edit") };

        assert_eq!(edit.edits, [("a", "b"), ("c", "d")]);
    }

    #[test]
    fn a_file_is_named_by_its_last_part() {
        assert_eq!(file_name("/workspaces/aspen/src/signal.rs"), "signal.rs");
        assert_eq!(file_name("/workspaces/aspen/src/"), "src");
    }
}
