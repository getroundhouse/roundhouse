//! A worktree's files, as a client browses them: what's in it or in one of its folders, and what one file holds.

use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::attachment::{AttachmentBytes, MediaType};
use crate::client::{ApiError, ErrorCode};

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
    /// A video, by what it starts with, as [`MediaType::sniff`] decides, with none of its bytes, which a player reads
    /// as it needs them, with `WorkspaceFileReadRange`.
    Video { media_type: MediaType, bytes: u64 },
    /// It's something other than text or an image, by a NUL byte near its start, as git decides.
    Binary,
    /// It's bigger than a client is sent.
    TooBig { bytes: u64 },
    #[serde(other)]
    Unknown,
}

/// The most bytes one read of part of a file sends, a few seconds of a screen's recording, so a player starts soon
/// after it asks, and one reply stays far under what a frame can carry.
pub const MAX_RANGE_BYTES: u32 = 4 * 1024 * 1024;

/// Which part of a file a read asks for: `length` bytes from `start`, never none and never more than
/// [`MAX_RANGE_BYTES`], as each request that reads part of a file says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ByteRange {
    start: u64,
    length: u32,
}

impl ByteRange {
    /// Refused with `InvalidRequest` when `length` is 0 or more than [`MAX_RANGE_BYTES`].
    pub fn new(start: u64, length: u32) -> Result<Self, ApiError> {
        if length == 0 || length > MAX_RANGE_BYTES {
            let message = format!("a read of part of a file takes 1 to {MAX_RANGE_BYTES} bytes, not {length}");
            return Err(ApiError::new(ErrorCode::InvalidRequest, message));
        }
        Ok(Self { start, length })
    }

    pub fn start(self) -> u64 {
        self.start
    }

    pub fn length(self) -> u32 {
        self.length
    }

    /// The bytes it asks for of a file of `file_bytes` bytes: none past the file's end.
    pub fn within(self, file_bytes: u64) -> Range<u64> {
        self.start.min(file_bytes)..self.start.saturating_add(u64::from(self.length)).min(file_bytes)
    }
}

/// Part of a file, read from `start`, such as a video's, which a player reads as it needs it rather than whole.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FileRange {
    pub start: u64,
    /// As many bytes as were asked for, or fewer where the file ends, and none from past its end.
    pub data: AttachmentBytes,
    /// How many bytes the whole file has, as it was read, so a reader knows where it ends, and that it changed when
    /// this does.
    pub file_bytes: u64,
}

/// A file as a commit held it, opened to be read a page of lines at a time, so one too big to send whole can still be
/// read, such as the lines a diff leaves out of it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FileOpened {
    Text(OpenedText),
    /// It's an image, or something else that isn't text, by what it starts with, which `WorkspaceFileReadAt` reads
    /// whole.
    NotText,
    #[serde(other)]
    Unknown,
}

/// A text file that's been opened, and any of its pages sent with it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OpenedText {
    pub bytes: u64,
    /// How many lines it has, a last one without a line ending included.
    pub lines: u32,
    /// How many lines a page has, which `WorkspaceFilePages` is asked for its pages with. Page `n` is lines
    /// `n * page_lines + 1` to `(n + 1) * page_lines`, numbered from 1, and the last page has what's left.
    pub page_lines: u32,
    /// When the file is small enough to be sent at once, its pages, with no line cut short, as many as fit in what's
    /// sent at once, which is all of them unless bytes that aren't UTF-8 make it bigger. Otherwise none.
    #[serde(default)]
    pub pages: Vec<FilePage>,
}

/// Pages of a file's lines, in the order the file has them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct FilePages {
    pub pages: Vec<FilePage>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FilePage {
    /// Which page it is, from 0.
    pub index: u32,
    pub lines: Vec<PageLine>,
}

/// One line of a file, without its line ending.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PageLine {
    /// Bytes that aren't UTF-8 are replaced.
    pub text: String,
    /// How many bytes the whole line has in the file, when it's too long to be sent whole, and `text` is its start, cut
    /// where a character ends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cut: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::{ByteRange, FileContents, MAX_RANGE_BYTES};
    use crate::attachment::{AttachmentBytes, MediaType};
    use crate::client::ErrorCode;

    /// A read of part of a file asks for at least a byte and at most a reply's worth, and gets none from past the end.
    #[test]
    fn a_read_of_part_of_a_file_gets_what_the_file_has_of_it() {
        let refused = |length| ByteRange::new(0, length).unwrap_err().code;

        assert_eq!(ByteRange::new(4, 8).unwrap().within(100), 4..12);
        assert_eq!(ByteRange::new(96, 8).unwrap().within(100), 96..100, "up to the end");
        assert_eq!(ByteRange::new(200, 8).unwrap().within(100), 100..100, "past the end");
        assert_eq!(ByteRange::new(u64::MAX, MAX_RANGE_BYTES).unwrap().within(u64::MAX), u64::MAX..u64::MAX);
        assert_eq!(refused(0), ErrorCode::InvalidRequest);
        assert_eq!(refused(MAX_RANGE_BYTES + 1), ErrorCode::InvalidRequest);
    }

    /// A newer hub can say something this build doesn't know, which has to read as something it can show.
    #[test]
    fn contents_of_a_kind_this_build_doesn_t_know_read_as_unknown() {
        let text: FileContents = serde_json::from_str(r#"{"kind":"text","text":"one"}"#).unwrap();
        let newer: FileContents = serde_json::from_str(r#"{"kind":"audio","media_type":"audio/ogg"}"#).unwrap();

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
