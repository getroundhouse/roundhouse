//! Images a tool sends back, such as a screenshot the agent reads. Each becomes an attachment of the session, which the
//! hub keeps in a file of its own, as it does a prompt's images. The line is kept without their bytes, since the CLI
//! sends a read image twice in one line, in its result and in its record of the read, as about a megabyte of base64.

use protocol::attachment::{Attachment, AttachmentBytes, MediaType, NewAttachment};
use protocol::event::ContentBlock;
use protocol::model::AttachmentId;
use serde_json::Value;

/// The CLI doesn't say what an image is called, and the call that read it names the file.
const NAME: &str = "image";

/// An image taken out of a line: what keeps it, and the base64 it came as, to find again in the line.
pub(super) struct Taken {
    pub(super) kept: NewAttachment,
    base64: String,
}

/// Fills in the images each tool result in `blocks` sent back, from `content`, the line's own list of blocks, which
/// `blocks` was read from in the same order. Returns what was taken.
pub(super) fn take(content: Option<&Value>, blocks: &mut [ContentBlock]) -> Vec<Taken> {
    let Some(Value::Array(items)) = content else { return Vec::new() };
    let mut taken = Vec::new();
    for (block, item) in blocks.iter_mut().zip(items) {
        let ContentBlock::ToolResult { images, .. } = block else { continue };
        let Some(Value::Array(parts)) = item.get("content") else { continue };
        for found in parts.iter().filter_map(image) {
            images.push(found.kept.attachment.clone());
            taken.push(found);
        }
    }
    taken
}

/// An image block, under a new ID, if its bytes are an image the transcript can draw and a session would take. Anything
/// else is left in the line.
fn image(part: &Value) -> Option<Taken> {
    if part.get("type").and_then(Value::as_str) != Some("image") {
        return None;
    }
    let base64 = part.get("source")?.get("data")?.as_str()?;
    let data = AttachmentBytes::from_base64(base64)?;
    // Known by its bytes, as a prompt's file is, rather than by the type the CLI gives it.
    let media_type = MediaType::sniff(data.as_slice());
    if !media_type.is_image() || media_type.check(data.as_slice()).is_err() {
        return None;
    }
    let id = AttachmentId::from(uuid::Uuid::new_v4().to_string());
    let size_bytes = u64::try_from(data.len()).unwrap_or(u64::MAX);
    let attachment = Attachment { id, name: NAME.to_string(), media_type, size_bytes };
    Some(Taken { kept: NewAttachment { attachment, data }, base64: base64.to_string() })
}

/// `line` again, with the base64 of each image in `taken` swapped for a [`marker`] naming the attachment that keeps it,
/// wherever it appears. Normalized again, the line has no images, so a rebuild of the views finds each among the
/// session's attachments by its marker.
pub(super) fn without(line: &Value, taken: &[Taken]) -> String {
    let mut line = line.clone();
    swap(&mut line, taken);
    line.to_string()
}

fn swap(value: &mut Value, taken: &[Taken]) {
    match value {
        Value::String(text) => {
            if let Some(found) = taken.iter().find(|found| found.base64 == *text) {
                *text = marker(&found.kept.attachment.id);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|item| swap(item, taken)),
        Value::Object(fields) => fields.values_mut().for_each(|field| swap(field, taken)),
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

/// What stands in a kept line where an image's base64 was.
pub(super) fn marker(id: &AttachmentId) -> String {
    format!("attachment:{id}")
}
