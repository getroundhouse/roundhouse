use crate::model::AttachmentId;
use serde::{Deserialize, Serialize};

/// The most a prompt can carry, as many files as most people would pick by hand.
pub const MAX_PER_PROMPT: usize = 10;

/// The largest file a session takes. A file travels to the hub, and on to the worker, in one frame, which has to stay
/// under [`crate::frame::MAX_FRAME_BYTES`] once it's base64.
pub const MAX_FILE_BYTES: u64 = 100 * 1024 * 1024;

/// What kind of file an attachment is. The kinds named are the ones an agent can be given in a prompt, or the
/// transcript draws, and anything else is `Other`, which an agent opens from the worktree. Each is sent as its MIME
/// type.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MediaType {
    #[serde(rename = "image/png")]
    Png,
    #[serde(rename = "image/jpeg")]
    Jpeg,
    #[serde(rename = "image/gif")]
    Gif,
    #[serde(rename = "image/webp")]
    Webp,
    /// A drawing, which is text, so an agent is given it as text, and the transcript draws it where the agent's text
    /// points to it, as it does a bitmap.
    #[serde(rename = "image/svg+xml")]
    Svg,
    /// UTF-8 text, such as a log or a source file.
    #[serde(rename = "text/plain")]
    Text,
    #[serde(rename = "application/octet-stream")]
    Other,
    #[serde(other)]
    Unknown,
}

impl MediaType {
    pub const fn mime(self) -> &'static str {
        match self {
            MediaType::Png => "image/png",
            MediaType::Jpeg => "image/jpeg",
            MediaType::Gif => "image/gif",
            MediaType::Webp => "image/webp",
            MediaType::Svg => "image/svg+xml",
            MediaType::Text => "text/plain",
            MediaType::Other | MediaType::Unknown => "application/octet-stream",
        }
    }

    /// An image the transcript draws, which is what the hub keeps.
    pub const fn is_image(self) -> bool {
        match self {
            MediaType::Png | MediaType::Jpeg | MediaType::Gif | MediaType::Webp | MediaType::Svg => true,
            MediaType::Text | MediaType::Other | MediaType::Unknown => false,
        }
    }

    /// An image made of pixels, which an agent's model takes as an image, and a file's tab and a prompt draw as one.
    /// A drawing isn't, since a model takes it as the text it is.
    pub const fn is_bitmap(self) -> bool {
        match self {
            MediaType::Png | MediaType::Jpeg | MediaType::Gif | MediaType::Webp => true,
            MediaType::Svg | MediaType::Text | MediaType::Other | MediaType::Unknown => false,
        }
    }

    /// Text, which an agent can be given as it's written: plain text, or a drawing.
    pub const fn is_text(self) -> bool {
        match self {
            MediaType::Text | MediaType::Svg => true,
            MediaType::Png
            | MediaType::Jpeg
            | MediaType::Gif
            | MediaType::Webp
            | MediaType::Other
            | MediaType::Unknown => false,
        }
    }

    /// What `bytes` are, from what they start with rather than from a file's name, which can say anything. Text is
    /// UTF-8 with no NUL in it, and a drawing is text whose first element is `<svg`.
    pub fn sniff(bytes: &[u8]) -> MediaType {
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            MediaType::Png
        } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            MediaType::Jpeg
        } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            MediaType::Gif
        } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
            MediaType::Webp
        } else if !bytes.contains(&0)
            && let Ok(text) = std::str::from_utf8(bytes)
        {
            if starts_svg(text) { MediaType::Svg } else { MediaType::Text }
        } else {
            MediaType::Other
        }
    }

    /// Whether a session takes `bytes` as a file of this kind. The hub refuses anything this refuses, and a client
    /// asks first so it can say so before sending.
    pub fn check(self, bytes: &[u8]) -> Result<(), AttachmentProblem> {
        if self == MediaType::Unknown {
            return Err(AttachmentProblem::Unsupported);
        }
        if bytes.is_empty() {
            return Err(AttachmentProblem::Empty);
        }
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_FILE_BYTES {
            return Err(AttachmentProblem::TooLarge { limit_bytes: MAX_FILE_BYTES });
        }
        let found = Self::sniff(bytes);
        // A client from before drawings says one is text, which it is.
        let drawing_as_text = self == MediaType::Text && found == MediaType::Svg;
        if self == MediaType::Other || found == self || drawing_as_text {
            Ok(())
        } else {
            Err(AttachmentProblem::NotWhatItSays { said: self, found })
        }
    }
}

/// Whether `text`'s first element is an `<svg>`, after any byte-order mark, XML declaration, comment, doctype or space
/// before it.
fn starts_svg(text: &str) -> bool {
    let mut rest = text.trim_start_matches('\u{feff}');
    loop {
        rest = rest.trim_start();
        let skipped = [("<?", "?>"), ("<!--", "-->"), ("<!", ">")]
            .iter()
            .find(|(open, _)| rest.starts_with(open))
            .and_then(|(open, close)| rest[open.len()..].find(close).map(|end| open.len() + end + close.len()));
        match skipped {
            Some(end) => rest = &rest[end..],
            None => break,
        }
    }
    rest.strip_prefix("<svg")
        .is_some_and(|after| after.starts_with(|c: char| c.is_whitespace() || c == '>' || c == '/'))
}

/// Why a session won't take a file. The message is written for the person who chose it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachmentProblem {
    /// A kind from a newer build, which this one doesn't know how to hand an agent.
    Unsupported,
    Empty,
    TooLarge {
        limit_bytes: u64,
    },
    /// Its bytes are another kind, so a file named `.png` that's really a JPEG can't reach the agent labelled as a
    /// PNG. Anything can go as `Other`.
    NotWhatItSays {
        said: MediaType,
        found: MediaType,
    },
}

impl std::fmt::Display for AttachmentProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttachmentProblem::Unsupported => {
                f.write_str("the hub is older than the app, and doesn't know this kind of file")
            }
            AttachmentProblem::Empty => f.write_str("the file is empty"),
            AttachmentProblem::TooLarge { limit_bytes } => {
                write!(f, "a file can be at most {}", describe_size(*limit_bytes))
            }
            AttachmentProblem::NotWhatItSays { said, found } => {
                write!(f, "the file says it's {} but it's {}", said.mime(), found.mime())
            }
        }
    }
}

impl std::error::Error for AttachmentProblem {}

/// A size the way a person reads it, such as "256 KB" or "1.5 MB".
pub fn describe_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    if bytes >= MB {
        let tenths = (bytes * 10).div_ceil(MB);
        if tenths.is_multiple_of(10) {
            format!("{} MB", tenths / 10)
        } else {
            format!("{}.{} MB", tenths / 10, tenths % 10)
        }
    } else if bytes >= KB {
        format!("{} KB", bytes.div_ceil(KB))
    } else {
        format!("{bytes} bytes")
    }
}

/// What a prompt says it carries, or a tool's result says it sent back. An image's bytes are kept apart, and read with
/// `AttachmentRead`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Attachment {
    pub id: AttachmentId,
    /// The file's name, without where it was. People see it, and so does the agent.
    pub name: String,
    pub media_type: MediaType,
    pub size_bytes: u64,
}

crate::base64_bytes::base64_bytes!(
    /// An attachment's bytes.
    AttachmentBytes
);

impl AttachmentBytes {
    /// The base64 an agent's API takes an image as.
    pub fn to_base64(&self) -> String {
        crate::base64_bytes::encode(self.as_slice())
    }

    /// Bytes an agent sent as base64, such as an image a tool returned. `None` if it isn't base64.
    pub fn from_base64(text: &str) -> Option<Self> {
        crate::base64_bytes::decode(text).ok().map(Self::new)
    }
}

/// An attachment and its bytes. It's an image a tool sent the agent, for the hub to keep, which the worker sends beside
/// the event that names it, under a new ID, as a client gives a file it adds. Or it's an image a prompt carried, for a
/// call that reads it beside the prompt's words, such as one for a title.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct NewAttachment {
    pub attachment: Attachment,
    pub data: AttachmentBytes,
}

/// An attachment once its worker has written it into the worktree, as an agent is given it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AttachmentFile {
    pub attachment: Attachment,
    /// Where it is, in full, so the agent can open it without knowing where it's running.
    pub path: String,
}

/// The name to keep for a file: the last part of what a client sent, without control characters, and with any `"` made
/// a `'`, since the agent is given it in quotes. It's never empty, and never `.` or `..`, since the worker writes a
/// file by it, and at most 200 bytes, which leaves room for a number within the 255 a disk allows.
pub fn file_name(sent: &str) -> String {
    const MAX_BYTES: usize = 200;
    let last = sent.rsplit(['/', '\\']).next().unwrap_or_default();
    let mut name = String::new();
    for c in last.chars().filter(|c| !c.is_control()).map(|c| if c == '"' { '\'' } else { c }) {
        if name.len() + c.len_utf8() > MAX_BYTES {
            break;
        }
        name.push(c);
    }
    match name.trim() {
        "" | "." | ".." => "attachment".to_string(),
        name => name.to_string(),
    }
}
