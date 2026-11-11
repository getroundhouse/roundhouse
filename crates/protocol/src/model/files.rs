//! A worktree's files, as a client browses them: what's in it or in one of its folders, and what one file holds.

use serde::{Deserialize, Serialize};

use crate::attachment::{AttachmentBytes, MediaType};

/// Files in a worktree, tracked, new or ignored, by their paths from the worktree, and the folders whose files are only
/// listed once someone asks, since one such as `target` or `node_modules` can hold hundreds of thousands.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct WorktreeFiles {
    /// In order.
    pub paths: Vec<String>,
    /// Folders whose files aren't in `paths`, by their paths from the worktree, in order. Reading one lists what's
    /// directly in it.
    #[serde(default)]
    pub folders: Vec<String>,
    /// True when some were left out to keep the reply bounded.
    pub truncated: bool,
}

/// What a file in a worktree holds, as a client shows it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FileContents {
    /// Its text. Bytes that aren't UTF-8 are replaced, so it can still be shown.
    Text { text: String },
    /// An image a client draws, whole, by what its bytes start with rather than by its name, as
    /// [`MediaType::sniff`] decides. It's sent up to a bigger size than text, since it can't be shown in part.
    Image { media_type: MediaType, data: AttachmentBytes },
    /// It's something other than text or an image, by a NUL byte near its start, as git decides.
    Binary,
    /// It's bigger than a client is sent.
    TooBig { bytes: u64 },
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::FileContents;
    use crate::attachment::{AttachmentBytes, MediaType};

    /// A newer hub can say something this build doesn't know, which has to read as something it can show.
    #[test]
    fn contents_of_a_kind_this_build_doesn_t_know_read_as_unknown() {
        let text: FileContents = serde_json::from_str(r#"{"kind":"text","text":"one"}"#).unwrap();
        let newer: FileContents = serde_json::from_str(r#"{"kind":"video","media_type":"video/mp4"}"#).unwrap();

        assert_eq!(text, FileContents::Text { text: "one".to_string() });
        assert_eq!(newer, FileContents::Unknown);
    }

    /// An image's bytes travel as base64 beside the kind they are, which a build from before images reads as
    /// `Unknown`, and so shows as a file it can't show.
    #[test]
    fn an_image_travels_as_its_kind_and_its_bytes_in_base64() {
        let image = FileContents::Image { media_type: MediaType::Png, data: AttachmentBytes::new(b"\x89PNG".to_vec()) };

        let sent = serde_json::to_value(&image).unwrap();

        assert_eq!(sent, serde_json::json!({"kind": "image", "media_type": "image/png", "data": "iVBORw=="}));
        assert_eq!(serde_json::from_value::<FileContents>(sent).unwrap(), image);
    }
}
