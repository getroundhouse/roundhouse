//! How Codex's items read as typed calls and what they sent back.

use protocol::capability::calls::CallOutput;
use protocol::capability::calls::output::McpContent;
use serde_json::json;

use super::translate::mcp_result;

/// As an MCP server can send it, which reads back as none, so it's kept as none.
#[test]
fn a_null_structured_result_is_none() {
    let sent = mcp_result(&json!({"content": [{"type": "text", "text": "ok"}], "structuredContent": null}));
    let given = mcp_result(&json!({"content": [], "structuredContent": {"rows": 2}}));

    let ok = vec![McpContent::Text { text: "ok".into() }];
    assert_eq!(sent, CallOutput::Mcp { content: ok, structured: None });
    assert_eq!(given, CallOutput::Mcp { content: Vec::new(), structured: Some(json!({"rows": 2})) });
}

/// As Codex's turn's diff has them, from 0.170.0: a file changed, one made, one moved and one deleted.
#[test]
fn a_turns_diff_reads_as_each_file_it_changed() {
    use protocol::capability::calls::output::FileOutcome;
    use protocol::event::{FileChange, Hunk, HunkLine, LineMark};

    let diff = "diff --git a/a.txt b/a.txt\nindex 814f4a4..08a39ce 100644\n--- a/a.txt\n+++ b/a.txt\n@@ -1,2 +1,2 @@\n-one\n+uno\n two\ndiff --git a/hello.txt b/hello.txt\nnew file mode 100644\nindex 0000000..ce01362\n--- /dev/null\n+++ b/hello.txt\n@@ -0,0 +1 @@\n+hello\ndiff --git a/old.txt b/new.txt\nsimilarity index 100%\nrename from old.txt\nrename to new.txt\ndiff --git a/gone.txt b/gone.txt\ndeleted file mode 100644\nindex ce01362..0000000\n--- a/gone.txt\n+++ /dev/null\n@@ -1 +0,0 @@\n-bye\n";

    let line = |mark, text: &str| HunkLine { mark, text: text.into() };
    let changed = vec![Hunk {
        old_start: 1,
        new_start: 1,
        lines: vec![line(LineMark::Removed, "one"), line(LineMark::Added, "uno"), line(LineMark::Same, "two")],
    }];
    let outcome = |path: &str, change, moved_to: Option<&str>| FileOutcome {
        path: path.into(),
        change,
        moved_to: moved_to.map(str::to_string),
        diagnostics: Vec::new(),
    };
    assert_eq!(
        super::turn_changes(diff),
        [
            outcome("a.txt", Some(FileChange::Changed { hunks: changed }), None),
            outcome("hello.txt", Some(FileChange::Created), None),
            outcome("old.txt", Some(FileChange::Changed { hunks: Vec::new() }), Some("new.txt")),
            outcome("gone.txt", None, None),
        ]
    );
    assert_eq!(super::turn_changes(""), [], "a turn that has changed nothing");
}
