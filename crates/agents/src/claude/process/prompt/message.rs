use protocol::attachment::{AttachmentBytes, AttachmentFile, MediaType};
use protocol::prompt::Prompt;
use serde_json::{Value, json};

/// Reads the files it gives in full, so it's called off the async threads. A spoken prompt has how to talk before it.
pub(in super::super) fn user_message(prompt: &Prompt<AttachmentFile>) -> Value {
    let mut content: Vec<Value> =
        crate::spoken::how_to_talk(prompt).map(|told| json!({"type": "text", "text": told})).into_iter().collect();
    content.extend(crate::attachments::file_content_blocks(prompt, image_block));
    user_line_saying(content, prompt)
}

/// A `user` line of `content`'s blocks, made from the prompt's files, and after them what the prompt says, unless it
/// says nothing and they're something. A spoken prompt always says something, so how to talk before it is never all.
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
