//! Reading the file the view shows, and making what it shows of it ready off the main thread: its lines, plain and then
//! in colour, its image, or its video's still or what it says of itself. A file read again that hasn't changed is left
//! as it shows, colours and all. What a read gives, and why a file isn't shown, are the same for a side of a changed
//! image, which reads its files the same way.

use std::hash::{DefaultHasher, Hash as _, Hasher as _};

use client_core::ClientError;
use gpui_kit::{AppContext as _, AsyncApp, Context, SharedString, WeakEntity};
use protocol::attachment::{MediaType, describe_size};
use protocol::client::ErrorCode;
use protocol::model::FileContents;
use transcript::{VideoPlayer, VideoSource};

use super::lines::Lines;
use super::markdown::is_markdown;
use super::video::FileVideo;
use super::{Contents, FileView};
use crate::file_image::FileImage;

/// Above this, a file is highlighted only after it's shown plain, since highlighting it takes a noticeable moment.
const QUICK_BYTES: usize = 64 * 1024;
/// Above this, a file is left plain, since highlighting it would take seconds.
pub(crate) const HIGHLIGHT_UP_TO: usize = 1024 * 1024;

/// What a read gives.
pub(crate) enum Read {
    Text(String),
    Image(MediaType, Vec<u8>),
    /// A video of this many bytes, none of which are read until it's shown.
    Video(MediaType, u64),
    /// A line shown in its place, saying why.
    Instead(Unshown),
    /// Lost with the connection, so what's shown stays, and the file is read again on the next one.
    Lost,
}

/// Why a file is shown as a line saying so, rather than as itself.
pub(crate) enum Unshown {
    Empty,
    Binary,
    TooBig(u64),
    /// It was there when the files were listed, and isn't now.
    Gone,
    Failed(String),
    /// It says it's an image, and it can't be decoded.
    Unreadable,
    /// It's text, where an image was expected.
    Text,
    /// It's a video, where an image was expected.
    Video,
}

impl Unshown {
    pub(crate) fn said(&self) -> String {
        match self {
            Unshown::Empty => "This file is empty".to_string(),
            Unshown::Binary => "This file isn't text, so it can't be shown".to_string(),
            Unshown::TooBig(bytes) => format!("This file is too big to show, at {}", describe_size(*bytes)),
            Unshown::Gone => "This file isn't there now".to_string(),
            Unshown::Failed(message) => format!("Couldn't read the file: {message}"),
            Unshown::Unreadable => "This image couldn't be read".to_string(),
            Unshown::Text => "This file is text, not an image".to_string(),
            Unshown::Video => "This file is a video, not an image".to_string(),
        }
    }
}

impl FileView {
    pub(super) fn read(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.path.clone() else { return };
        self.reads += 1;
        let (read, client, id, shown) = (self.reads, self.client.clone(), self.workspace_id.clone(), self.shown);
        cx.spawn(async move |this, cx| {
            let (file, reading, workspace) = (path.to_string(), client.clone(), id.clone());
            let contents = cx.background_spawn(async move { reading.workspace_file_read(&workspace, &file) }).await;
            match what_it_gives(contents) {
                Read::Lost => {}
                Read::Instead(why) => {
                    let _ = this.update(cx, |t, cx| t.set_contents(read, Contents::Instead(why), None, cx));
                }
                Read::Image(media_type, data) => show_image(&this, cx, read, shown, media_type, data).await,
                Read::Video(media_type, bytes) => {
                    let source = VideoSource::Worktree { client, workspace: id, path: path.to_string() };
                    show_video(&this, cx, read, shown, media_type, bytes, source).await;
                }
                Read::Text(text) => show_text(&this, cx, read, shown, &path, text).await,
            }
        })
        .detach();
    }
}

pub(crate) fn what_it_gives(contents: Result<FileContents, ClientError>) -> Read {
    match contents {
        Err(ClientError::Disconnected) => Read::Lost,
        Ok(FileContents::Text { text }) if text.is_empty() => Read::Instead(Unshown::Empty),
        Ok(FileContents::Text { text }) => Read::Text(text),
        Ok(FileContents::Image { media_type, data }) => Read::Image(media_type, data.into_vec()),
        Ok(FileContents::Video { media_type, bytes }) => Read::Video(media_type, bytes),
        Ok(FileContents::Binary | FileContents::Unknown) => Read::Instead(Unshown::Binary),
        Ok(FileContents::TooBig { bytes }) => Read::Instead(Unshown::TooBig(bytes)),
        Err(ClientError::Api(e)) if e.code == ErrorCode::NotFound => Read::Instead(Unshown::Gone),
        Err(e) => Read::Instead(Unshown::Failed(client_core::refusals::reason(&e))),
    }
}

/// Decodes the image off the main thread, unless it's the one shown, read again.
async fn show_image(
    this: &WeakEntity<FileView>,
    cx: &mut AsyncApp,
    read: u64,
    shown: Option<u64>,
    media_type: MediaType,
    data: Vec<u8>,
) {
    let decoded = cx.background_spawn(async move {
        let hash = hash(&data);
        (shown != Some(hash)).then(|| (hash, FileImage::read(media_type, &data, hash)))
    });
    let Some((hash, image)) = decoded.await else { return };
    let contents = image.map_or(Contents::Instead(Unshown::Unreadable), Contents::Image);
    let _ = this.update(cx, |t, cx| t.set_contents(read, contents, Some(hash), cx));
}

/// Reads a WebM's still, to play it in the tab, or what another video says of itself, off the main thread, unless it's
/// the one shown, read again. Only its kind and its size are read until then, so a video read again whose kind and size
/// are the same is taken to be the same, and keeps playing where it is.
async fn show_video(
    this: &WeakEntity<FileView>,
    cx: &mut AsyncApp,
    read: u64,
    shown: Option<u64>,
    media_type: MediaType,
    bytes: u64,
    source: VideoSource,
) {
    let hash = hash(format!("{media_type:?} {bytes}").as_bytes());
    if shown == Some(hash) {
        return;
    }
    let reading = source.clone();
    let still =
        cx.background_spawn(
            async move { if media_type == MediaType::Webm { transcript::still(&reading) } else { None } },
        );
    let contents = if let Some(still) = still.await {
        let player = cx.new(|cx| VideoPlayer::new(source, still, cx));
        cx.update(|cx| FileVideo::playing(media_type, bytes, player, cx))
    } else {
        // An MP4 or a QuickTime file, or a WebM the app can't play, such as one in AV1, which the system's player may.
        let about = cx.background_spawn(async move { source.about().unwrap_or_default() }).await;
        FileVideo::elsewhere(media_type, bytes, about)
    };
    let _ = this.update(cx, |t, cx| t.set_contents(read, Contents::Video(contents), Some(hash), cx));
}

async fn show_text(
    this: &WeakEntity<FileView>,
    cx: &mut AsyncApp,
    read: u64,
    shown: Option<u64>,
    path: &SharedString,
    text: String,
) {
    let (hash, text) = cx.background_spawn(async move { (hash(text.as_bytes()), text) }).await;
    // Read again and the same as it was, so what's shown, colours and all, still is.
    if shown == Some(hash) {
        return;
    }
    if is_markdown(path) {
        let (name, rendered) = (path.clone(), SharedString::from(text.clone()));
        let _ = this.update(cx, |t, cx| {
            if t.reads == read {
                t.document.update(cx, |document, cx| document.show(name, rendered, cx));
            }
        });
    }
    let language = (text.len() <= HIGHLIGHT_UP_TO).then(|| look::language(path).to_string());
    // A big file is shown plain first, the first time, rather than left loading while it's highlighted. Read again,
    // what's shown stays, in its colours, until the new lines are.
    let Some(language) = language else {
        let lines = cx.background_spawn(async move { Lines::new(&text, None) }).await;
        let _ = this.update(cx, |t, cx| t.set_contents(read, Contents::Lines(lines), Some(hash), cx));
        return;
    };
    if text.len() > QUICK_BYTES && shown.is_none() {
        let plain = text.clone();
        let lines = cx.background_spawn(async move { Lines::new(&plain, None) }).await;
        let current = this.update(cx, |t, cx| t.set_contents(read, Contents::Lines(lines), None, cx));
        // Not highlighted for nothing once another file, or another read, has taken its place.
        if !current.unwrap_or(false) {
            return;
        }
    }
    let lines = cx.background_spawn(async move { Lines::new(&text, Some(&language)) }).await;
    let _ = this.update(cx, |t, cx| t.set_contents(read, Contents::Lines(lines), Some(hash), cx));
}

/// A hash of a file's bytes, to tell whether it changed since it was shown without keeping a copy of it.
pub(crate) fn hash(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}
