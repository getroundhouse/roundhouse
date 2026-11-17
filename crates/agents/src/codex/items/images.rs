//! Codex's looks at an image, and the images it makes. Codex names each image only by where the file is, and sends
//! none of its bytes, so the adapter reads the file and keeps it as an attachment of the session, as Claude's adapter
//! keeps an image a tool sends back. The chat then draws what the model saw or made, after the file is gone too.

use std::path::Path;

use protocol::attachment::{Attachment, AttachmentBytes, MediaType, NewAttachment, describe_size, file_name};
use protocol::capability::calls::{
    Call, CallOutput,
    call::{GenerateImage, ViewImage},
};
use protocol::model::AttachmentId;
use serde::Deserialize;
use serde_json::Value;
use tokio::io::AsyncReadExt as _;

use super::native::Status;

/// Far bigger than a screenshot or an image Codex makes, as the worker reads an image an agent's text links to.
const MAX_BYTES: u64 = 32 * 1024 * 1024;

/// What came back for a call about an image, and the image to keep beside the event that says so.
pub(super) struct Kept {
    pub(super) output: CallOutput,
    pub(super) images: Vec<NewAttachment>,
}

/// A look at the image at `path`, which Codex has read for the model by the time it says so.
pub(super) fn look(path: String) -> Call {
    Call::ViewImage(ViewImage { path })
}

/// What came back from a look at the image at `path`: the image, or why it wasn't kept.
pub(super) async fn looked_at(path: &str) -> Kept {
    Kept::of(path, None).await
}

/// An image Codex made, as its `imageGeneration` item says it, as it starts and once it's made or its making failed.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Made {
    pub(super) id: String,
    /// `in_progress` as it starts, then `completed` or `failed`.
    status: String,
    /// What it was asked to make, which Codex says only once it's done.
    #[serde(default)]
    revised_prompt: Option<String>,
    /// Where Codex saved what it made, in a folder of its own.
    #[serde(default)]
    saved_path: Option<String>,
    /// The images it was made from, which Codex says only once it's done.
    #[serde(default)]
    input_images: Vec<InputImage>,
    #[serde(default)]
    failure: Option<Value>,
}

#[derive(Deserialize)]
struct InputImage {
    path: String,
}

impl Made {
    /// The call, as far as Codex has said it: as it starts, Codex hasn't said what it was asked to make, so its prompt
    /// is empty, and what came back says it.
    pub(super) fn call(&self) -> Call {
        let from = self.input_images.iter().map(|image| image.path.clone()).collect();
        Call::GenerateImage(GenerateImage { prompt: self.revised_prompt.clone().unwrap_or_default(), from })
    }

    /// How it ended, once it has: Codex completes the item whether the image was made or not, and says which.
    pub(super) fn ended(&self) -> Status {
        if self.status == "failed" || self.failure.is_some() { Status::Failed } else { Status::Completed }
    }

    /// What came back: the image, with what it was asked to make, or what's known of why it wasn't made or kept.
    pub(super) async fn kept(self) -> Kept {
        if matches!(self.ended(), Status::Failed) {
            let limited = self.failure.as_ref().is_some_and(|failure| failure["type"] == "usageLimitExceeded");
            // Codex tells only the model why else it failed.
            let text = if limited { "Codex's limit on making images was reached" } else { "" };
            return Kept { output: CallOutput::Words { text: text.into() }, images: Vec::new() };
        }
        match &self.saved_path {
            Some(path) => Kept::of(path, self.revised_prompt).await,
            None => Kept::unkept("Codex didn't say where it saved the image"),
        }
    }
}

impl Kept {
    /// The image at `path`, made from `prompt` if it's one the agent made, or why it wasn't kept.
    async fn of(path: &str, prompt: Option<String>) -> Self {
        match image(path).await {
            Ok(kept) => {
                Self { output: CallOutput::Image { image: kept.attachment.clone(), prompt }, images: vec![kept] }
            }
            Err(why) => Self::unkept(&why),
        }
    }

    /// Nothing kept, and why, which is what the chat shows of it.
    fn unkept(why: &str) -> Self {
        Self { output: CallOutput::Words { text: format!("The image wasn't kept: {why}") }, images: Vec::new() }
    }
}

/// The image at `path`, under a new ID, if its bytes are an image the transcript draws, of no more than
/// [`MAX_BYTES`]. Codex names it in full, so a path that isn't is refused rather than read from wherever the adapter
/// runs.
async fn image(path: &str) -> Result<NewAttachment, String> {
    if !Path::new(path).is_absolute() {
        return Err(format!("{path} isn't a full path"));
    }
    let mut bytes = Vec::new();
    let read = async { tokio::fs::File::open(path).await?.take(MAX_BYTES + 1).read_to_end(&mut bytes).await };
    read.await.map_err(|e| format!("couldn't read {path}: {e}"))?;
    let size_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if size_bytes > MAX_BYTES {
        return Err(format!("{path} is bigger than {}", describe_size(MAX_BYTES)));
    }
    // Known by its bytes, as a prompt's file is, rather than by its name.
    let media_type = MediaType::sniff(&bytes);
    if !media_type.is_image() || media_type.check(&bytes).is_err() {
        return Err(format!("{path} isn't an image the chat draws"));
    }
    let id = AttachmentId::from(uuid::Uuid::new_v4().to_string());
    let attachment = Attachment { id, name: file_name(path), media_type, size_bytes };
    Ok(NewAttachment { attachment, data: AttachmentBytes::new(bytes) })
}
