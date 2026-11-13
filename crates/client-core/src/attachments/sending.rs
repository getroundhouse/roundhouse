use crate::client::{ClientError, new_attachment_id};
use protocol::attachment::{Attachment, MAX_FILE_BYTES, MediaType, describe_size, file_name};
use std::path::Path;

/// A file to send with a prompt, under the ID it's shown by until it's recorded and after. Sent again under the same
/// ID, it isn't added twice.
#[derive(Clone)]
pub struct FileToSend {
    pub attachment: Attachment,
    pub data: Vec<u8>,
}

/// Only the length of its bytes, which would otherwise be megabytes of numbers.
impl std::fmt::Debug for FileToSend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileToSend").field("attachment", &self.attachment).field("bytes", &self.data.len()).finish()
    }
}

impl FileToSend {
    /// `data` as `media_type`, under a new ID, if a session would take it.
    pub fn checked(name: &str, media_type: MediaType, data: Vec<u8>) -> std::result::Result<Self, NotAttached> {
        let name = file_name(name);
        if let Err(problem) = media_type.check(&data) {
            return Err(NotAttached { name, why: problem.to_string() });
        }
        let size_bytes = u64::try_from(data.len()).unwrap_or(u64::MAX);
        Ok(Self { attachment: Attachment { id: new_attachment_id(), name, media_type, size_bytes }, data })
    }

    /// A file from disk, as the kind its bytes say it is. Its size is checked before it's read, so choosing something
    /// huge doesn't read it all first.
    pub fn read(path: &Path) -> std::result::Result<Self, NotAttached> {
        let name = file_name(&path.to_string_lossy());
        let refused = |why: String| NotAttached { name: name.clone(), why };
        let metadata = std::fs::metadata(path).map_err(|e| refused(e.to_string()))?;
        if metadata.is_dir() {
            return Err(refused("a folder can't be attached".to_string()));
        }
        if metadata.len() > MAX_FILE_BYTES {
            return Err(refused(format!("a file can be at most {}", describe_size(MAX_FILE_BYTES))));
        }
        let data = std::fs::read(path).map_err(|e| refused(e.to_string()))?;
        Self::checked(&name, MediaType::sniff(&data), data)
    }

    /// Gives the hub this file with `add`, as its kind, whatever it's for. A hub refuses a kind it doesn't know, such as
    /// one newer than it, as it refuses a request it doesn't know, so the file is tried again as one that goes as it
    /// is, which every hub that takes files takes. Refused again, it's the request that's refused.
    pub(crate) fn give<T>(&self, add: impl Fn(MediaType) -> Result<T, ClientError>) -> Result<T, ClientError> {
        let kind = self.attachment.media_type;
        match add(kind) {
            Err(e) if e.is_unsupported() && kind != MediaType::Other => add(MediaType::Other),
            added => added,
        }
    }
}

/// Why a file can't go, written for the person who chose it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotAttached {
    pub name: String,
    pub why: String,
}

impl std::fmt::Display for NotAttached {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "couldn't attach {}: {}", self.name, self.why)
    }
}

impl std::error::Error for NotAttached {}
