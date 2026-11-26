use client_core::{Client, Hub};
use protocol::client::attachments::AttachmentReadRange;
use protocol::model::{AttachmentId, MAX_RANGE_BYTES, SessionId, WorkspaceId};
use std::io;
use std::num::NonZeroU32;
use video::{About, Fetch, OpenError, Part, Parts, WebmVideo};

/// How much of a video is asked for at a time: a quarter of what one reply can carry, so the first frame comes after
/// a quarter of a second's reading of a screen's recording, rather than a second's.
const PART_BYTES: NonZeroU32 = NonZeroU32::new(MAX_RANGE_BYTES / 4).unwrap();
/// How many parts are kept, as many as a WebM's reader goes back and forth over as it opens one, its start and its
/// index at the end, and a few seconds of frames.
const KEPT: usize = 16;
/// More than one, so VP9 decodes on several threads, and few, since a chat can play several at once.
const THREADS: NonZeroU32 = NonZeroU32::new(2).unwrap();

/// Where a video's bytes come from, a part at a time, which the hub gives.
#[derive(Clone)]
pub enum VideoSource {
    /// The copy the hub kept of one an agent's text pointed to, as the chat it's in has it.
    Kept { hub: Hub, session: SessionId, id: AttachmentId },
    /// One in a workspace's worktree, as it is now.
    Worktree { client: Client, workspace: WorkspaceId, path: String },
}

impl VideoSource {
    /// The same video, read on `client`'s connection, if it's read on one. One a chat's text pointed to is read from
    /// its hub, whichever connection there is.
    pub(crate) fn on(&self, client: Client) -> Self {
        match self.clone() {
            kept @ VideoSource::Kept { .. } => kept,
            VideoSource::Worktree { workspace, path, .. } => VideoSource::Worktree { client, workspace, path },
        }
    }

    /// Opens it, reading what it has to of it from the hub, so it's for a thread that can wait.
    pub(super) fn open(&self) -> Result<WebmVideo<Parts<VideoSource>>, OpenError> {
        let parts = self.parts().map_err(|e| OpenError::Read(e.into()))?;
        WebmVideo::open(parts, THREADS)
    }

    /// What it says of itself, if it's an MP4 or a QuickTime file, which the app doesn't play. It waits on the hub, so
    /// it's for a background thread.
    pub fn about(&self) -> io::Result<About> {
        video::about(&mut self.parts()?)
    }

    /// Writes the whole of it to `to`, a part at a time, failing if it changes as it's read.
    pub fn copy_to(&self, to: &mut impl io::Write) -> io::Result<u64> {
        io::copy(&mut self.parts()?, to)
    }

    /// It, read a part at a time.
    fn parts(&self) -> io::Result<Parts<VideoSource>> {
        Parts::new(self.clone(), PART_BYTES, KEPT)
    }
}

impl Fetch for VideoSource {
    fn fetch(&mut self, start: u64, length: u32) -> io::Result<Part> {
        let read = match self {
            VideoSource::Kept { hub, session, id } => {
                hub.call(AttachmentReadRange { session_id: session.clone(), id: id.clone(), start, length })
            }
            VideoSource::Worktree { client, workspace, path } => {
                client.workspace_file_read_range(workspace, path, start, length)
            }
        };
        let range = read.map_err(io::Error::other)?;
        Ok(Part { data: range.data.into_vec(), file_bytes: range.file_bytes })
    }
}
