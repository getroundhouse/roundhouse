//! Reading back the files the hub kept with a draft, for a client to put back with what's typed: a chat's from the
//! chat, and the new-workspace dialog's from where the hub keeps them apart, since no chat exists yet.

use protocol::attachment::Attachment;

use super::FilesFor;
use crate::{Client, ClientError, FileToSend};

#[cfg(test)]
mod tests;

/// The files the hub kept for a draft, as they were read back, in their order, each with its bytes or why it couldn't
/// be read.
#[derive(Debug)]
pub struct ReadBack(Vec<(Attachment, Result<Vec<u8>, ClientError>)>);

/// What came back of the files a draft was waiting for: those to put back, in their order, and each that couldn't be
/// read.
#[derive(Debug)]
pub struct CameBack {
    pub files: Vec<FileToSend>,
    pub unread: Vec<NotReadBack>,
}

/// A file the hub kept with a draft that couldn't be read back, such as one it no longer has.
#[derive(Debug)]
pub struct NotReadBack {
    pub name: String,
    pub error: ClientError,
}

impl std::fmt::Display for NotReadBack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "couldn't bring back {}: {}", self.name, self.error)
    }
}

impl Client {
    /// Reads back each of `files`, which the hub kept for `kept_for`, one at a time. Each read blocks, and can be a lot
    /// of bytes, so it belongs on a thread of its own, and isn't a command quitting waits for, since the hub keeps them
    /// anyway.
    pub fn read_back(&self, kept_for: &FilesFor, files: Vec<Attachment>) -> ReadBack {
        let read = files.into_iter().map(|file| {
            let data = match kept_for {
                FilesFor::Chat(chat) => self.attachment_read(chat, &file.id),
                FilesFor::NewWorkspace => self.new_workspace_draft_file_read(&file.id),
            };
            (file, data)
        });
        ReadBack(read.collect())
    }
}

impl ReadBack {
    /// What came back, once it's every file still `waiting`, in its order. `None` for files no longer waited for, such
    /// as ones a change made elsewhere stopped naming, and for a read the connection cut off, since they're read again
    /// on the next.
    pub fn came_back(self, waiting: &[Attachment]) -> Option<CameBack> {
        let still_waiting = waiting.iter().map(|file| &file.id).eq(self.0.iter().map(|(file, _)| &file.id));
        let cut_off = |(_, data): &(Attachment, Result<Vec<u8>, ClientError>)| {
            matches!(data, Err(ClientError::Io(_) | ClientError::Disconnected))
        };
        if !still_waiting || self.0.iter().any(cut_off) {
            return None;
        }
        let (mut files, mut unread) = (Vec::new(), Vec::new());
        for (attachment, data) in self.0 {
            match data {
                Ok(data) => files.push(FileToSend { attachment, data }),
                Err(error) => unread.push(NotReadBack { name: attachment.name, error }),
            }
        }
        Some(CameBack { files, unread })
    }
}
