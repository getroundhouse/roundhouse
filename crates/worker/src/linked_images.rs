//! The images the agent's text points to in the worktree, such as `![The new layout](docs/shot.png)`. The worker reads
//! each as the finished message passes through it, and sends it to the hub beside the event, so the hub keeps the image
//! as it was when the agent wrote about it. When the agent changes a file and points to it again, the later text gets
//! the file as it is then, and every client draws what each text showed, whatever happens to the worktree after.

use std::collections::HashMap;
use std::path::Path;

use protocol::attachment::{Attachment, AttachmentBytes, NewAttachment};
use protocol::event::{ContentBlock, EventBody, LinkedImage};
use protocol::model::AttachmentId;
use tracing::debug;

mod addresses;

/// The most of what one event's texts point to that's kept. An event and what it carries travel to the hub in one
/// frame, as base64, which [`protocol::frame::MAX_FRAME_BYTES`] caps, so this leaves room for the event itself.
const MAX_BYTES_PER_EVENT: u64 = 128 * 1024 * 1024;

/// `body`, with each image its texts point to named in its text's block, and those images, to send beside it. Files
/// are read off the async threads, and only for an event whose text might point to an image.
pub(crate) async fn keep_in(worktree: &Path, body: EventBody) -> (EventBody, Vec<NewAttachment>) {
    if !texts(&body).any(addresses::might_have) {
        return (body, Vec::new());
    }
    let (worktree, unchanged) = (worktree.to_path_buf(), body.clone());
    let keeping = tokio::task::spawn_blocking(move || {
        let mut body = body;
        let kept = keep(&worktree, &mut body, MAX_BYTES_PER_EVENT);
        (body, kept)
    });
    // Only a panic ends it early, and the event still goes, without its images.
    keeping.await.unwrap_or((unchanged, Vec::new()))
}

/// Reads each image the texts in `body` point to from the worktree at `worktree`, and names it in its text's block.
/// One that isn't there, or isn't an image inside the worktree, is left out, and so is one past the `left` bytes the
/// event can carry.
fn keep(worktree: &Path, body: &mut EventBody, mut left: u64) -> Vec<NewAttachment> {
    let mut kept = Vec::new();
    // Each address is read once for the event, however many of its texts point to it.
    let mut read: HashMap<String, Option<Attachment>> = HashMap::new();
    for (text, images) in texts_mut(body) {
        for address in addresses::of(text) {
            let attachment = read.entry(address.clone()).or_insert_with(|| {
                let image = read_one(worktree, &address, &mut left)?;
                let attachment = image.attachment.clone();
                kept.push(image);
                Some(attachment)
            });
            if let Some(attachment) = attachment {
                images.push(LinkedImage { address, attachment: attachment.clone() });
            }
        }
    }
    kept
}

/// The image at `address`, under a new ID, if it's an image inside the worktree that fits in what's `left`.
fn read_one(worktree: &Path, address: &str, left: &mut u64) -> Option<NewAttachment> {
    let path = addresses::path(address)?;
    let (media_type, bytes) = crate::images::read(worktree, &path)
        .inspect_err(|e| debug!(path, "not keeping an image the agent's text points to: {e}"))
        .ok()?;
    let size_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let Some(after) = left.checked_sub(size_bytes) else {
        debug!(path, "not keeping an image the agent's text points to, past what one event can carry");
        return None;
    };
    *left = after;
    let id = AttachmentId::from(uuid::Uuid::new_v4().to_string());
    let name = Path::new(&path).file_name().map_or_else(|| path.clone(), |name| name.to_string_lossy().into_owned());
    let attachment = Attachment { id, name, media_type, size_bytes };
    Some(NewAttachment { attachment, data: AttachmentBytes::new(bytes) })
}

/// The text of each block of the agent's own words in `body`.
fn texts(body: &EventBody) -> impl Iterator<Item = &str> {
    let (EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. }) = body else {
        return [].iter().filter_map(text_of);
    };
    blocks.iter().filter_map(text_of)
}

fn text_of(block: &ContentBlock) -> Option<&str> {
    match block {
        ContentBlock::Text { text, .. } => Some(text),
        ContentBlock::Thinking { .. }
        | ContentBlock::ToolUse { .. }
        | ContentBlock::ToolResult { .. }
        | ContentBlock::Image { .. }
        | ContentBlock::Unknown => None,
    }
}

/// Each block of text in `body`, with the images it names.
fn texts_mut(body: &mut EventBody) -> impl Iterator<Item = (&str, &mut Vec<LinkedImage>)> {
    let (EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. }) = body else {
        return [].iter_mut().filter_map(text_and_images);
    };
    blocks.iter_mut().filter_map(text_and_images)
}

fn text_and_images(block: &mut ContentBlock) -> Option<(&str, &mut Vec<LinkedImage>)> {
    match block {
        ContentBlock::Text { text, images } => Some((text.as_str(), images)),
        ContentBlock::Thinking { .. }
        | ContentBlock::ToolUse { .. }
        | ContentBlock::ToolResult { .. }
        | ContentBlock::Image { .. }
        | ContentBlock::Unknown => None,
    }
}

#[cfg(test)]
mod tests;
