//! What a tool result says, as text. Its content is a string, or a list of blocks: text, images, documents such as a
//! PDF the agent read, references to the tools a tool search loaded, and any kind a newer CLI sends. A block of a kind
//! this build doesn't know is shown as its JSON, so none is left out only for being new. An image's bytes become an
//! attachment, which `images` takes, and a document's bytes can't be read as text.

use std::fmt::Write as _;

use protocol::short_text::start_of;
use serde_json::Value;

use super::{each_string, str_of, string_of};

/// The most of any one string shown in a block's JSON. Enough for any name, path or paragraph in one, while bytes a
/// block carries as base64 don't put megabytes into the event and every view of it.
const LONGEST_STRING_CHARS: usize = 2_000;

/// What `content` says, each block that says anything a line apart, in order.
pub(super) fn read(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => {
            blocks.iter().filter_map(said).filter(|text| !text.is_empty()).collect::<Vec<_>>().join("\n")
        }
        _ => String::new(),
    }
}

/// What one block says, if anything. A block of a known kind without the field that kind says it in is shown as its
/// JSON, as a kind this build doesn't know is.
fn said(block: &Value) -> Option<String> {
    let known = match str_of(block, "type") {
        Some("image") => return None,
        // A PDF the agent read comes after a text block naming the file.
        Some("document") if is_bytes(block) => return string_of(block, "title"),
        Some("text") => string_of(block, "text"),
        // A tool that a tool search loaded.
        Some("tool_reference") => string_of(block, "tool_name"),
        _ => None,
    };
    Some(known.unwrap_or_else(|| as_json(block)))
}

/// Whether `block` holds bytes as base64, as an image or a PDF does, rather than words.
fn is_bytes(block: &Value) -> bool {
    block.pointer("/source/type").and_then(Value::as_str) == Some("base64")
}

/// `block` as its JSON, with each long string cut short.
fn as_json(block: &Value) -> String {
    let mut block = block.clone();
    each_string(&mut block, &mut cut);
    block.to_string()
}

/// `text` cut to [`LONGEST_STRING_CHARS`], saying how much was left out.
fn cut(text: &mut String) {
    let (start, cut) = start_of(text, LONGEST_STRING_CHARS);
    if !cut {
        return;
    }
    let at = start.len();
    let more = text[at..].chars().count();
    text.truncate(at);
    let unit = if more == 1 { "character" } else { "characters" };
    // Writing to a string can't fail.
    let _ = write!(text, "… and {more} more {unit}");
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{LONGEST_STRING_CHARS, read};

    #[test]
    fn each_block_that_says_something_is_a_line_in_order() {
        let pdf = json!({ "type": "base64", "media_type": "application/pdf", "data": "JVBERi0x" });
        let content = json!([
            { "type": "text", "text": "PDF file read: timetable.pdf" },
            { "type": "document", "source": pdf },
            { "type": "document", "source": pdf, "title": "Timetable" },
            { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "iVBORw0KGgo=" } },
            { "type": "text", "text": "" },
            { "type": "tool_reference", "tool_name": "TaskStop" },
            { "type": "tool_reference", "tool_name": "Monitor" },
        ]);

        assert_eq!(read(Some(&content)), "PDF file read: timetable.pdf\nTimetable\nTaskStop\nMonitor");
    }

    #[test]
    fn a_block_it_can_t_read_in_words_is_shown_as_its_json() {
        let blocks = vec![
            json!({ "type": "search_result", "title": "Signals", "content": [] }),
            json!({ "type": "tool_reference" }),
            json!({ "type": "document", "source": { "type": "text", "data": "Platform 2" } }),
        ];

        let text = read(Some(&Value::from(blocks.clone())));

        let shown: Vec<Value> = text.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
        assert_eq!(shown, blocks);
    }

    #[test]
    fn only_a_string_in_a_block_s_json_is_cut_and_only_past_the_limit() {
        let at_the_limit = "é".repeat(LONGEST_STRING_CHARS);
        let past_it = format!("{at_the_limit}é");
        let content = json!([
            { "type": "text", "text": past_it },
            { "type": "attachment", "name": at_the_limit, "data": past_it },
        ]);

        let text = read(Some(&content));

        let (said, json) = text.split_once('\n').unwrap();
        assert_eq!(said, past_it, "a text block is shown whole");
        let shown: Value = serde_json::from_str(json).unwrap();
        assert_eq!(shown["name"], at_the_limit);
        assert_eq!(shown["data"], format!("{at_the_limit}… and 1 more character"));
    }
}
