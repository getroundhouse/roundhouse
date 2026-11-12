//! A prompt as the CLI takes it: one `user` line, with each attached file in it as far as the CLI can take it.
//!
//! Every file is in the worktree already, and the agent is told where in a tag that names it, so it can open the file
//! with its tools, in this turn or a later one. An image also goes in as a base64 image block, just before its tag, up
//! to what one prompt takes, and the CLI shrinks one bigger than its API takes. A short text file goes in whole,
//! inside its tag.
//! Anything else, such as a PDF, an archive or a long log, the agent opens from where it is. Files come before what
//! was typed, which is where Anthropic's guidance puts images for the best answers. `docs/claude-cli.md` has what was
//! checked.

use protocol::attachment::{AttachmentBytes, AttachmentFile, MediaType};
use protocol::prompt::Prompt;
use serde_json::{Value, json};

/// The most image bytes one prompt gives in itself, which keeps its line to the CLI to about 43 MB as base64. An image
/// past that is opened from the worktree.
const INLINE_IMAGE_BYTES: u64 = 32 * 1024 * 1024;
/// About 8,000 tokens. A longer text file is opened from the worktree, where the agent can search it rather than
/// take all of it in.
const INLINE_TEXT_BYTES: u64 = 32 * 1024;
const CLOSING_TAG: &str = "</attachment>";

/// Reads the files it gives in full, so it's called off the async threads.
pub(super) fn user_message(prompt: &Prompt<AttachmentFile>) -> Value {
    let mut images_left = INLINE_IMAGE_BYTES;
    let content = prompt.files.iter().flat_map(|file| blocks(file, &mut images_left)).collect();
    user_line_saying(content, prompt)
}

/// A `user` line of `content`'s blocks, made from the prompt's files, and after them what the prompt says, unless it
/// says nothing and they're something.
pub(in crate::claude) fn user_line_saying<F>(mut content: Vec<Value>, prompt: &Prompt<F>) -> Value {
    if prompt.says_anything() || content.is_empty() {
        content.push(json!({"type": "text", "text": prompt.text}));
    }
    user_line(content)
}

/// A `user` line of `content`'s blocks, as the CLI reads one on stdin.
fn user_line(content: Vec<Value>) -> Value {
    json!({
        "type": "user",
        "session_id": "",
        "parent_tool_use_id": null,
        "message": {"role": "user", "content": Value::Array(content)},
    })
}

/// An image given whole, as a block of a `user` line.
pub(in crate::claude) fn image_block(media_type: MediaType, data: &AttachmentBytes) -> Value {
    json!({"type": "image", "source": {"type": "base64", "media_type": media_type.mime(), "data": data.to_base64()}})
}

/// A file that can't be read back from the worktree, or a kind this build doesn't know, goes by its path alone, and
/// so does a text file that would end its own tag.
fn blocks(file: &AttachmentFile, images_left: &mut u64) -> Vec<Value> {
    let attachment = &file.attachment;
    let tag = |inside: Option<&str>| {
        let (name, path) = (escape(&attachment.name), escape(&file.path));
        let text = match inside {
            Some(text) => format!("<attachment name=\"{name}\" path=\"{path}\">\n{text}\n{CLOSING_TAG}"),
            None => format!("<attachment name=\"{name}\" path=\"{path}\" />"),
        };
        json!({"type": "text", "text": text})
    };
    let size = attachment.size_bytes;
    if attachment.media_type.is_image() && size <= *images_left {
        if let Ok(bytes) = std::fs::read(&file.path) {
            *images_left -= size;
            return vec![image_block(attachment.media_type, &AttachmentBytes::new(bytes)), tag(None)];
        }
    } else if attachment.media_type == MediaType::Text
        && size <= INLINE_TEXT_BYTES
        && let Ok(bytes) = std::fs::read(&file.path)
    {
        let text = String::from_utf8_lossy(&bytes);
        if !text.contains(CLOSING_TAG) {
            return vec![tag(Some(&text))];
        }
    }
    vec![tag(None)]
}

/// So a path can't end the quotes it's in. A name has no quotes in it, as `file_name` keeps it.
fn escape(text: &str) -> String {
    text.replace('"', "&quot;")
}

#[cfg(test)]
mod tests;
