//! Images the agent's text points to, such as `![The new layout](docs/shot.png)`. One in the workspace comes from the
//! hub, which has the worker read it from the worktree, by a path in full or from the worktree, and one written into
//! the text as a `data:` URL is its own bytes. Nothing else is drawn, such as an image on the web, since what an agent
//! writes is untrusted. Each image is decoded off the main thread before it's drawn, and then every row waiting for it
//! is measured again, since the list keeps each row's height, and the image is only its own height once it's decoded.
//!
//! These are kept apart from the pictures a prompt carries, which the transcript keeps by attachment, because the text
//! view asks for an image from any thread, by its address, so what answers has to be shared across threads.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use client_core::{Client, ClientError};
use gpui_kit::{App, AppContext as _, Image, ImageCacheError, ImageSource, RenderImage, SharedUri, WeakEntity, Window};
use parking_lot::Mutex;
use protocol::attachment::{AttachmentBytes, MediaType};
use protocol::client::ErrorCode;
use protocol::model::{Seq, WorkspaceId};

use crate::attachments::image_format;
use crate::transcript::Transcript;

/// How long an image that couldn't be read for a reason that can pass, such as one not written yet, waits before it's
/// asked for again, the next time it's drawn.
const RETRY: Duration = Duration::from_secs(5);

/// The images one transcript's text points to, by where the text says each is. Cheap to clone.
#[derive(Clone)]
pub(crate) struct LinkedImages(Arc<Shared>);

struct Shared {
    workspace: WorkspaceId,
    /// Replaced when the transcript reconnects.
    client: Mutex<Client>,
    images: Mutex<HashMap<String, Linked>>,
    transcript: WeakEntity<Transcript>,
}

enum Linked {
    /// With the events whose rows are waiting for it.
    Loading(Vec<Seq>),
    Ready(Arc<RenderImage>),
    /// Refused, or not an image, so it's not asked for again on this connection.
    Unreadable,
    /// Not read at the time, such as when it wasn't there yet.
    Failed(Instant),
}

/// Where an image the text names is, if it's somewhere that's drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Target {
    /// In the workspace, as a path in full or from the worktree. The worker decides whether it's in the worktree.
    Workspace(String),
    /// Written into the text.
    Embedded(Vec<u8>),
}

impl LinkedImages {
    pub(crate) fn new(client: Client, workspace: WorkspaceId, transcript: WeakEntity<Transcript>) -> Self {
        let images = Mutex::new(HashMap::new());
        Self(Arc::new(Shared { workspace, client: Mutex::new(client), images, transcript }))
    }

    /// Carries on on a new connection, and asks again for what couldn't be read, in case the connection was why.
    pub(crate) fn reconnect(&self, client: &Client) {
        *self.0.client.lock() = client.clone();
        self.0.images.lock().retain(|_, linked| matches!(linked, Linked::Loading(_) | Linked::Ready(_)));
    }

    /// What a text view in the row of the event at `seq` draws for the image at `uri`.
    pub(crate) fn resolver(&self, seq: Seq) -> impl Fn(&SharedUri) -> ImageSource + Send + Sync + 'static {
        let images = self.clone();
        move |uri| {
            let (images, uri) = (images.clone(), uri.to_string());
            ImageSource::Custom(Arc::new(move |_: &mut Window, cx: &mut App| images.image(&uri, seq, cx)))
        }
    }

    /// The image at `uri` if it's here, an error if it can't be shown, or `None` while it loads, which it starts doing
    /// the first time it's asked for.
    fn image(&self, uri: &str, seq: Seq, cx: &mut App) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        let unreadable = || Err(ImageCacheError::Asset(format!("{uri} can't be shown").into()));
        let mut images = self.0.images.lock();
        match images.get_mut(uri) {
            Some(Linked::Ready(image)) => return Some(Ok(image.clone())),
            Some(Linked::Unreadable) => return Some(unreadable()),
            Some(Linked::Failed(at)) if at.elapsed() < RETRY => return Some(unreadable()),
            Some(Linked::Loading(waiting)) => {
                if !waiting.contains(&seq) {
                    waiting.push(seq);
                }
                return None;
            }
            Some(Linked::Failed(_)) | None => {}
        }
        let Some(target) = target(uri) else {
            images.insert(uri.to_string(), Linked::Unreadable);
            return Some(unreadable());
        };
        images.insert(uri.to_string(), Linked::Loading(vec![seq]));
        drop(images);
        self.load(uri.to_string(), target, cx);
        None
    }

    fn load(&self, uri: String, target: Target, cx: &mut App) {
        let (images, client, workspace) = (self.clone(), self.0.client.lock().clone(), self.0.workspace.clone());
        let renderer = cx.svg_renderer();
        let loading = cx.background_spawn(async move {
            let read = match target {
                Target::Workspace(path) => client.workspace_image_read(&workspace, &path),
                Target::Embedded(bytes) => Ok((MediaType::sniff(&bytes), bytes)),
            };
            match read {
                Ok((media_type, bytes)) => image_format(media_type)
                    .and_then(|format| Image::from_bytes(format, bytes).to_image_data(renderer).ok())
                    .map_or(Linked::Unreadable, Linked::Ready),
                Err(e) if lasting(&e) => Linked::Unreadable,
                Err(_) => Linked::Failed(Instant::now()),
            }
        });
        cx.spawn(async move |cx| {
            let linked = loading.await;
            let waiting = match images.0.images.lock().insert(uri, linked) {
                Some(Linked::Loading(waiting)) => waiting,
                _ => Vec::new(),
            };
            let _ = images.0.transcript.update(cx, |transcript, cx| {
                for seq in waiting {
                    transcript.remeasure(seq, cx);
                }
            });
        })
        .detach();
    }
}

/// Whether reading an image failed in a way asking again won't change: the hub refused it, or doesn't know how.
fn lasting(e: &ClientError) -> bool {
    matches!(e, ClientError::Api(e) if matches!(e.code, ErrorCode::InvalidRequest | ErrorCode::Unsupported))
}

/// Where `uri` points, from a Markdown image's address, or `None` if it's somewhere that isn't drawn. A path can be a
/// `file://` URL, and either can be percent-encoded, as an address with a space in it has to be.
fn target(uri: &str) -> Option<Target> {
    if let Some(data) = uri.strip_prefix("data:") {
        let (meta, base64) = data.split_once(',')?;
        let bytes = meta.ends_with(";base64").then(|| AttachmentBytes::from_base64(base64)).flatten()?;
        return Some(Target::Embedded(bytes.into_vec()));
    }
    let path = uri.strip_prefix("file://").unwrap_or(uri);
    if path.is_empty() || path.contains("://") {
        return None;
    }
    let decoded = percent_encoding::percent_decode_str(path).decode_utf8();
    Some(Target::Workspace(decoded.map_or_else(|_| path.to_string(), std::borrow::Cow::into_owned)))
}

#[cfg(test)]
mod tests {
    use client_core::ClientError;
    use protocol::client::{ApiError, ErrorCode};

    use super::{Target, lasting, target};

    #[test]
    fn an_image_in_the_workspace_is_named_by_its_path_however_it_was_written() {
        let workspace = |path: &str| Some(Target::Workspace(path.into()));
        assert_eq!(target("docs/shot.png"), workspace("docs/shot.png"));
        assert_eq!(target("/work/repo/shot.png"), workspace("/work/repo/shot.png"));
        assert_eq!(target("file:///work/repo/a%20shot.png"), workspace("/work/repo/a shot.png"));
        assert_eq!(target("100%.png"), workspace("100%.png"), "a % that escapes nothing stays");
        assert_eq!(target("%FF.png"), workspace("%FF.png"), "so does one that isn't text once it's decoded");
    }

    #[test]
    fn an_image_written_into_the_text_is_its_own_bytes_and_one_on_the_web_isnt_drawn() {
        assert_eq!(target("data:image/png;base64,R0lGODlh"), Some(Target::Embedded(b"GIF89a".to_vec())));
        assert_eq!(target("data:image/svg+xml,<svg/>"), None);
        assert_eq!(target("https://example.com/shot.png"), None);
        assert_eq!(target(""), None);
    }

    #[test]
    fn an_image_the_hub_refused_isnt_asked_for_again_but_one_that_wasnt_there_is() {
        let api = |code| ClientError::Api(ApiError::new(code, "no"));
        assert!(lasting(&api(ErrorCode::InvalidRequest)));
        assert!(lasting(&api(ErrorCode::Unsupported)));
        assert!(!lasting(&api(ErrorCode::NotFound)));
        assert!(!lasting(&ClientError::Disconnected));
    }
}
