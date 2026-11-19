use protocol::attachment::{AttachmentBytes, AttachmentFile, MediaType};
use protocol::prompt::Prompt;
use serde_json::{Value, json};

/// The most image bytes one prompt gives in itself, which keeps its line to the CLI to about 43 MB as base64. An image
/// past that is opened from the worktree.
pub(crate) const INLINE_IMAGE_BYTES: u64 = 32 * 1024 * 1024;
/// About 8,000 tokens. A longer text file is opened from the worktree, where the agent can search it rather than
/// take all of it in.
const INLINE_TEXT_BYTES: u64 = 32 * 1024;
const CLOSING_TAG: &str = "</attachment>";

/// Builds content blocks for `prompt`'s attached files, formatting inline images with `image_block`. Reads small text
/// and image files from disk, so callers invoke this off async runtime threads.
pub(crate) fn file_content_blocks(
    prompt: &Prompt<AttachmentFile>,
    image_block: impl Fn(MediaType, &AttachmentBytes) -> Value,
) -> Vec<Value> {
    let mut images_left = INLINE_IMAGE_BYTES;
    prompt.files.iter().flat_map(|file| blocks(file, &mut images_left, &image_block)).collect()
}

/// A file that can't be read back from the worktree, or a kind this build doesn't know, goes by its path alone, and
/// so does a text file that would end its own tag.
fn blocks(
    file: &AttachmentFile,
    images_left: &mut u64,
    image_block: &impl Fn(MediaType, &AttachmentBytes) -> Value,
) -> Vec<Value> {
    let attachment = &file.attachment;
    let tag = |inside: Option<&str>| {
        let (name, path) = (escape(&attachment.name), escape(&file.path));
        let text = match inside {
            Some(text) => format!("<attachment name=\"{name}\" path=\"{path}\">\n{text}\n{CLOSING_TAG}"),
            None => format!("<attachment name=\"{name}\" path=\"{path}\" />"),
        };
        json!({ "type": "text", "text": text })
    };
    let size = attachment.size_bytes;
    if attachment.media_type.is_bitmap()
        && size <= *images_left
        && let Ok(bytes) = std::fs::read(&file.path)
    {
        *images_left -= size;
        return vec![image_block(attachment.media_type, &AttachmentBytes::new(bytes)), tag(None)];
    }
    if attachment.media_type.is_text()
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
