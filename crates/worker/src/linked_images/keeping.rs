use super::addresses;
use protocol::attachment::{Attachment, AttachmentBytes, NewAttachment};
use protocol::event::{ContentBlock, EventBody, LinkedImage};
use protocol::model::AttachmentId;
use std::collections::HashMap;
use std::path::Path;
use tracing::debug;

/// The most of what one event's texts point to that's kept, or one proposal's. Each, and what it carries, travels to
/// the hub in one frame, as base64, which [`protocol::frame::MAX_FRAME_BYTES`] caps, so this leaves room for the event
/// or the proposal itself.
const MAX_BYTES_PER_FRAME: u64 = 128 * 1024 * 1024;

/// `body`, with each image its texts point to named in its text's block, and those images, to send beside it. Files
/// are read off the async threads, and only for an event whose text might point to an image.
pub(crate) async fn keep_in(worktree: &Path, body: EventBody) -> (EventBody, Vec<NewAttachment>) {
    if !texts(&body).any(addresses::might_have) {
        return (body, Vec::new());
    }
    let (worktree, unchanged) = (worktree.to_path_buf(), body.clone());
    let keeping = tokio::task::spawn_blocking(move || {
        let mut body = body;
        let kept = keep(&worktree, &mut body, MAX_BYTES_PER_FRAME);
        (body, kept)
    });
    // Only a panic ends it early, and the event still goes, without its images.
    keeping.await.unwrap_or((unchanged, Vec::new()))
}

/// The images `texts`, which are what an agent wrote for a proposal, point to in the worktree at `worktree`, each named
/// once, by its address, and their bytes, to send beside what names them. Files are read off the async threads.
pub(crate) async fn keep_for(worktree: &Path, texts: Vec<String>) -> (Vec<LinkedImage>, Vec<NewAttachment>) {
    if !texts.iter().any(|text| addresses::might_have(text)) {
        return (Vec::new(), Vec::new());
    }
    let worktree = worktree.to_path_buf();
    let reading = tokio::task::spawn_blocking(move || {
        let mut reading = Reading::new(&worktree, MAX_BYTES_PER_FRAME);
        let mut named: Vec<LinkedImage> = Vec::new();
        for image in texts.iter().flat_map(|text| reading.images_in(text)) {
            if !named.iter().any(|known| known.address == image.address) {
                named.push(image);
            }
        }
        (named, reading.kept)
    });
    // Only a panic ends it early, and the proposal is still made, without its images.
    reading.await.unwrap_or_default()
}

/// Reads each image the texts in `body` point to from the worktree at `worktree`, and names it in its text's block.
/// One that isn't there, or isn't an image inside the worktree, is left out, and so is one past the `left` bytes the
/// event can carry.
pub(super) fn keep(worktree: &Path, body: &mut EventBody, left: u64) -> Vec<NewAttachment> {
    let mut reading = Reading::new(worktree, left);
    for (text, images) in texts_mut(body) {
        images.extend(reading.images_in(text));
    }
    reading.kept
}

/// What's been read from a worktree of what some texts point to, which travels in one frame.
struct Reading<'a> {
    worktree: &'a Path,
    /// How many more bytes can be read.
    left: u64,
    /// Each address is read once, however many of the texts point to it.
    read: HashMap<String, Option<Attachment>>,
    kept: Vec<NewAttachment>,
}

impl<'a> Reading<'a> {
    fn new(worktree: &'a Path, left: u64) -> Self {
        Self { worktree, left, read: HashMap::new(), kept: Vec::new() }
    }

    /// Each image `text` points to, in order, once for each place it does, reading any not read yet.
    fn images_in(&mut self, text: &str) -> Vec<LinkedImage> {
        let mut images = Vec::new();
        for address in addresses::of(text) {
            let attachment = self.read.entry(address.clone()).or_insert_with(|| {
                let image = read_one(self.worktree, &address, &mut self.left)?;
                let attachment = image.attachment.clone();
                self.kept.push(image);
                Some(attachment)
            });
            if let Some(attachment) = attachment {
                images.push(LinkedImage { address, attachment: attachment.clone() });
            }
        }
        images
    }
}

/// The image or video at `address`, under a new ID, if it's one inside the worktree that fits in what's `left`.
fn read_one(worktree: &Path, address: &str, left: &mut u64) -> Option<NewAttachment> {
    let path = addresses::path(address)?;
    // What's left of what one event can carry, which a file bigger than is read no further than that.
    let (media_type, bytes) = crate::images::read(worktree, &path, *left)
        .inspect_err(|e| debug!(path, "not keeping what the agent's text points to: {e}"))
        .ok()?;
    let size_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    *left = left.saturating_sub(size_bytes);
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
        | ContentBlock::Image { .. }
        | ContentBlock::Call { .. }
        | ContentBlock::CallResult { .. }
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
        | ContentBlock::Image { .. }
        | ContentBlock::Call { .. }
        | ContentBlock::CallResult { .. }
        | ContentBlock::Unknown => None,
    }
}
