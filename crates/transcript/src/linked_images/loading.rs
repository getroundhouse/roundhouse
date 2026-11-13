use super::drawn;
use crate::attachments::image_format;
use crate::transcript::Transcript;
use client_core::Client;
use gpui_kit::component::text::MarkdownExtensions;
use gpui_kit::{App, AppContext as _, Image, ImageCacheError, ImageSource, RenderImage, SharedUri, WeakEntity, Window};
use parking_lot::Mutex;
use protocol::attachment::{AttachmentBytes, MediaType};
use protocol::event::LinkedImage;
use protocol::model::{AttachmentId, Seq, SessionId};
use std::collections::HashMap;
use std::sync::Arc;

/// How a text drawn from the kit's blocks draws the images it points to: what the kit draws each image [`to_read`](drawn::to_read)
/// took with, and the resolver the kit finds the rest with, which is all of them in a text parsed without it.
pub(crate) struct Images {
    pub(crate) plugins: Arc<MarkdownExtensions>,
    pub(crate) resolver: Box<dyn Fn(&SharedUri) -> ImageSource + Send + Sync>,
}

/// The images one transcript's text points to, by where each comes from. Cheap to clone.
#[derive(Clone)]
pub(crate) struct LinkedImages(pub(super) Arc<Shared>);

pub(super) struct Shared {
    pub(super) session: SessionId,
    /// Replaced when the transcript reconnects.
    pub(super) client: Mutex<Client>,
    pub(super) images: Mutex<HashMap<Source, Linked>>,
    pub(super) transcript: WeakEntity<Transcript>,
}

pub(super) enum Linked {
    /// With the events whose rows are waiting for it.
    Loading(Vec<Seq>),
    Ready(Arc<RenderImage>),
    /// The hub couldn't give it, or it isn't an image, so it's not asked for again on this connection.
    Unreadable,
}

/// Where an image a text points to comes from, if it's drawn.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum Source {
    /// The copy the hub kept when the text came.
    Kept(AttachmentId),
    /// Written into the text, by its whole address.
    Embedded(String),
}

impl LinkedImages {
    pub(crate) fn new(client: Client, session: SessionId, transcript: WeakEntity<Transcript>) -> Self {
        let images = Mutex::new(HashMap::new());
        Self(Arc::new(Shared { session, client: Mutex::new(client), images, transcript }))
    }

    /// Carries on on a new connection, and asks again for what couldn't be read, in case the connection was why.
    pub(crate) fn reconnect(&self, client: &Client) {
        *self.0.client.lock() = client.clone();
        self.0.images.lock().retain(|_, linked| !matches!(linked, Linked::Unreadable));
    }

    /// How a text drawn from the kit's blocks, which names `kept`, in the row of the event at `seq`, draws the images
    /// it points to.
    pub(crate) fn images(&self, seq: Seq, kept: &[LinkedImage]) -> Images {
        Images { plugins: Arc::new(drawn::to_draw(self, seq, kept)), resolver: Box::new(self.resolver(seq, kept)) }
    }

    /// The image at `uri` in a text that names `kept`, in the row of the event at `seq`, once it's here, or `None`
    /// while it loads, if it can't be shown, or if it isn't one that's drawn.
    pub(super) fn ready(&self, uri: &str, kept: &[LinkedImage], seq: Seq, cx: &mut App) -> Option<Arc<RenderImage>> {
        self.image(&source(uri, kept)?, seq, cx)?.ok()
    }

    /// What the kit draws for the image at an address in a text that names `kept`, in the row of the event at `seq`.
    pub(crate) fn resolver(
        &self,
        seq: Seq,
        kept: &[LinkedImage],
    ) -> impl Fn(&SharedUri) -> ImageSource + Send + Sync + 'static {
        let (images, kept) = (self.clone(), kept.to_vec());
        move |uri| {
            let (images, source) = (images.clone(), source(uri, &kept));
            ImageSource::Custom(Arc::new(move |_: &mut Window, cx: &mut App| match &source {
                Some(source) => images.image(source, seq, cx),
                None => Some(Err(ImageCacheError::Asset("not an image that's drawn".into()))),
            }))
        }
    }

    /// The image from `source` if it's here, an error if it can't be shown, or `None` while it loads, which it starts
    /// doing the first time it's asked for.
    pub(super) fn image(
        &self,
        source: &Source,
        seq: Seq,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        let mut images = self.0.images.lock();
        match images.get_mut(source) {
            Some(Linked::Ready(image)) => return Some(Ok(image.clone())),
            Some(Linked::Unreadable) => return Some(Err(ImageCacheError::Asset("it can't be shown".into()))),
            Some(Linked::Loading(waiting)) => {
                if !waiting.contains(&seq) {
                    waiting.push(seq);
                }
                return None;
            }
            None => {}
        }
        images.insert(source.clone(), Linked::Loading(vec![seq]));
        drop(images);
        self.load(source.clone(), cx);
        None
    }

    pub(super) fn load(&self, source: Source, cx: &mut App) {
        let (images, client, session) = (self.clone(), self.0.client.lock().clone(), self.0.session.clone());
        let (renderer, reading) = (cx.svg_renderer(), source.clone());
        let loading = cx.background_spawn(async move {
            let bytes = match reading {
                Source::Kept(id) => client.attachment_read(&session, &id).ok(),
                Source::Embedded(uri) => embedded(&uri),
            };
            bytes
                .and_then(|bytes| {
                    Image::from_bytes(image_format(MediaType::sniff(&bytes))?, bytes).to_image_data(renderer).ok()
                })
                .map_or(Linked::Unreadable, Linked::Ready)
        });
        cx.spawn(async move |cx| {
            let linked = loading.await;
            let waiting = match images.0.images.lock().insert(source, linked) {
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

/// Where the image at `uri` comes from, from a Markdown image's address and what its text's block names, or `None` if
/// it isn't drawn: on the web, elsewhere on the machine, or in the workspace without a copy the hub kept.
fn source(uri: &str, kept: &[LinkedImage]) -> Option<Source> {
    if uri.starts_with("data:") {
        return Some(Source::Embedded(uri.to_string()));
    }
    kept.iter().find(|image| image.address == uri).map(|image| Source::Kept(image.attachment.id.clone()))
}

/// The bytes a `data:` URL holds, if it's written as base64, as an image has to be.
fn embedded(uri: &str) -> Option<Vec<u8>> {
    let (meta, base64) = uri.strip_prefix("data:")?.split_once(',')?;
    meta.ends_with(";base64").then(|| AttachmentBytes::from_base64(base64)).flatten().map(AttachmentBytes::into_vec)
}

#[cfg(test)]
mod tests {
    use protocol::attachment::{Attachment, MediaType};
    use protocol::event::LinkedImage;
    use protocol::model::AttachmentId;

    use super::{Source, embedded, source};

    #[test]
    fn an_image_in_the_workspace_is_the_copy_its_text_names_by_the_address_it_gives() {
        let attachment = Attachment {
            id: AttachmentId::from("a1".to_string()),
            name: "shot.png".into(),
            media_type: MediaType::Png,
            size_bytes: 3,
        };
        let kept = [LinkedImage { address: "docs/a%20shot.png".into(), attachment }];

        assert_eq!(source("docs/a%20shot.png", &kept), Some(Source::Kept(AttachmentId::from("a1".to_string()))));
        assert_eq!(source("docs/a shot.png", &kept), None, "only by the address as the text gives it");
        assert_eq!(source("docs/other.png", &kept), None, "one with no copy isn't drawn");
    }

    #[test]
    fn an_image_written_into_the_text_is_its_own_bytes_and_one_on_the_web_isnt_drawn() {
        assert_eq!(embedded("data:image/png;base64,R0lGODlh"), Some(b"GIF89a".to_vec()));
        assert_eq!(embedded("data:image/svg+xml,<svg/>"), None);
        assert_eq!(source("https://example.com/shot.png", &[]), None);
        assert_eq!(source("", &[]), None);
    }
}
