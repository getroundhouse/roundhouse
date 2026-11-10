//! What a prompt carries: its script, and what each file or image that came with it was.

use parking_lot::Mutex;
use serde_json::Value;

/// What the newest prompt carried besides its script, as `attached` says it.
static LAST_ATTACHED: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub(crate) fn attached() -> String {
    let attached = LAST_ATTACHED.lock();
    if attached.is_empty() { "nothing attached".to_string() } else { format!("attached: {}", attached.join(" | ")) }
}

/// A prompt's script, which is its last text block, and what the blocks before it carried.
pub(crate) fn read_prompt(content: &Value) -> String {
    let blocks = content.as_array().cloned().unwrap_or_default();
    let script_at = blocks.iter().rposition(|block| {
        block["type"] == "text" && !block["text"].as_str().unwrap_or_default().starts_with("<attachment ")
    });
    let described = blocks.iter().enumerate().filter(|(i, _)| Some(*i) != script_at).map(|(_, block)| {
        if block["type"] == "image" {
            let data = block["source"]["data"].as_str().unwrap_or_default();
            let bytes = data.len() / 4 * 3 - data.chars().rev().take_while(|c| *c == '=').count();
            format!("{} {bytes} bytes", block["source"]["media_type"].as_str().unwrap_or_default())
        } else {
            describe_tag(block["text"].as_str().unwrap_or_default())
        }
    });
    *LAST_ATTACHED.lock() = described.collect();
    script_at.map(|at| blocks[at]["text"].as_str().unwrap_or_default().to_string()).unwrap_or_default()
}

/// `<attachment name="N" path="P">` with the file inside, or `<attachment name="N" path="P" />` for one to open.
fn describe_tag(text: &str) -> String {
    let mut lines = text.lines();
    let tag = lines.next().unwrap_or_default();
    let attribute = |key: &str| {
        let start = tag.find(&format!("{key}=\"")).map(|at| at + key.len() + 2)?;
        let end = tag[start..].find('"')? + start;
        Some(tag[start..end].replace("&quot;", "\""))
    };
    let name = attribute("name").unwrap_or_default();
    if !tag.ends_with("/>") {
        return format!("{name}: {}", lines.next().unwrap_or_default());
    }
    match attribute("path").map(std::fs::metadata) {
        Some(Ok(on_disk)) => format!("{name}: on disk, {} bytes", on_disk.len()),
        _ => format!("{name}: missing"),
    }
}
