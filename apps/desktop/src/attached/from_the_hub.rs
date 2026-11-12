//! Reading back the files the hub kept with what's typed in a place, for a chat's composer or the new-workspace dialog
//! to put back.

use client_core::{ClientError, FileToSend};
use gpui_kit::{AppContext as _, Context};
use protocol::attachment::Attachment;
use protocol::model::AttachmentId;

/// A file read back from the hub, or why it couldn't be.
pub(crate) type ReadBack = (Attachment, Result<Vec<u8>, ClientError>);

/// Reads back `files` with `read`, off the main thread, and hands what came back to `then`. A read isn't a command
/// quitting waits for, as it can be a lot of bytes, and the hub keeps them anyway.
pub(crate) fn read_back<T: 'static>(
    files: Vec<Attachment>,
    read: impl Fn(&AttachmentId) -> Result<Vec<u8>, ClientError> + Send + 'static,
    then: impl FnOnce(&mut T, Vec<ReadBack>, &mut Context<T>) + 'static,
    cx: &mut Context<T>,
) {
    cx.spawn(async move |this, cx| {
        let reading = cx.background_spawn(async move {
            let read = files.into_iter().map(|file| {
                let data = read(&file.id);
                (file, data)
            });
            read.collect::<Vec<ReadBack>>()
        });
        let read = reading.await;
        let _ = this.update(cx, |t, cx| then(t, read, cx));
    })
    .detach();
}

/// What came back, once it's every file still `waiting`, in its order: the files to put back, and why each of the
/// rest couldn't be read. `None` for files no longer waited for, such as ones a change made elsewhere stopped naming,
/// and for a read the connection cut off, since they're read again on the next.
pub(crate) fn came_back(waiting: &[Attachment], read: Vec<ReadBack>) -> Option<(Vec<FileToSend>, Vec<String>)> {
    let still_waiting = waiting.iter().map(|file| &file.id).eq(read.iter().map(|(file, _)| &file.id));
    let cut_off = |(_, data): &ReadBack| matches!(data, Err(ClientError::Io(_) | ClientError::Disconnected));
    if !still_waiting || read.iter().any(cut_off) {
        return None;
    }
    let (mut files, mut problems) = (Vec::new(), Vec::new());
    for (attachment, data) in read {
        match data {
            Ok(data) => files.push(FileToSend { attachment, data }),
            Err(e) => problems.push(format!("couldn't bring back {}: {e}", attachment.name)),
        }
    }
    Some((files, problems))
}
