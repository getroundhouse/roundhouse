use super::{drawings, drawn};
use crate::attachments::image_format;
use crate::transcript::Transcript;
use client_core::Client;
use gpui_kit::component::text::MarkdownExtensions;
use gpui_kit::{
    App, AppContext as _, Image, ImageCacheError, ImageFormat, ImageSource, RenderImage, SharedUri, WeakEntity, Window,
};
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

/// The images one chat's texts point to, by where each comes from, whether its transcript's or a proposal's. `W` is
/// what waits for one, such as the row of a transcript, which is told when it comes. Cheap to clone.
pub(crate) struct LinkedImages<W>(pub(super) Arc<Shared<W>>);

impl<W> Clone for LinkedImages<W> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

/// What waits for an image, which is told when it comes: a transcript's row, by its event's seq, or a page drawn whole.
pub(crate) trait Waiter: Copy + PartialEq + Send + Sync + 'static {}

impl<W: Copy + PartialEq + Send + Sync + 'static> Waiter for W {}

pub(super) struct Shared<W> {
    pub(super) session: SessionId,
    /// Replaced when its owner reconnects.
    pub(super) client: Mutex<Client>,
    pub(super) images: Mutex<HashMap<Source, Linked<W>>>,
    /// Tells what waited for an image that it has come, so each draws it at its size.
    pub(super) arrived: Arrived<W>,
}

/// What tells what waited for an image that it came.
type Arrived<W> = Box<dyn Fn(Vec<W>, &mut App) + Send + Sync>;

pub(super) enum Linked<W> {
    /// With what's waiting for it.
    Loading(Vec<W>),
    /// With how many pixels it covers as it's drawn, as [`drawable`] and [`pixels`] say.
    Ready(Arc<RenderImage>, (u32, u32)),
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

impl LinkedImages<Seq> {
    /// The images the texts of the transcript `transcript` point to, each of whose rows is measured again when one it
    /// waited for comes.
    pub(crate) fn of_transcript(client: Client, session: SessionId, transcript: WeakEntity<Transcript>) -> Self {
        Self::new(client, session, move |waiting, cx| {
            let _ = transcript.update(cx, |transcript, cx| {
                for seq in waiting {
                    transcript.remeasure(seq, cx);
                }
            });
        })
    }
}

impl<W: Waiter> LinkedImages<W> {
    /// The images the texts of the chat `session` point to, where `arrived` tells what waited for one that it came.
    pub(crate) fn new(
        client: Client,
        session: SessionId,
        arrived: impl Fn(Vec<W>, &mut App) + Send + Sync + 'static,
    ) -> Self {
        let images = Mutex::new(HashMap::new());
        Self(Arc::new(Shared { session, client: Mutex::new(client), images, arrived: Box::new(arrived) }))
    }

    /// Carries on on a new connection, and asks again for what couldn't be read, in case the connection was why.
    pub(crate) fn reconnect(&self, client: &Client) {
        *self.0.client.lock() = client.clone();
        self.0.images.lock().retain(|_, linked| !matches!(linked, Linked::Unreadable));
    }

    /// How a text drawn from the kit's blocks, which names `kept`, for `waiter`, draws the images it points to.
    pub(crate) fn images(&self, waiter: W, kept: &[LinkedImage]) -> Images {
        Images {
            plugins: Arc::new(drawn::to_draw(self, waiter, kept)),
            resolver: Box::new(self.resolver(waiter, kept)),
        }
    }

    /// The image at `uri` in a text that names `kept`, for `waiter`, and how many pixels it covers as it's drawn, once
    /// it's here, or `None` while it loads, if it can't be shown, or if it isn't one that's drawn.
    pub(super) fn ready(
        &self,
        uri: &str,
        kept: &[LinkedImage],
        waiter: W,
        cx: &mut App,
    ) -> Option<(Arc<RenderImage>, (u32, u32))> {
        let source = source(uri, kept)?;
        self.image(&source, waiter, cx)?.ok()?;
        match self.0.images.lock().get(&source) {
            Some(Linked::Ready(image, covers)) => Some((image.clone(), *covers)),
            _ => None,
        }
    }

    /// What the kit draws for the image at an address in a text that names `kept`, for `waiter`.
    pub(crate) fn resolver(
        &self,
        waiter: W,
        kept: &[LinkedImage],
    ) -> impl Fn(&SharedUri) -> ImageSource + Send + Sync + 'static {
        let (images, kept) = (self.clone(), kept.to_vec());
        move |uri| {
            let (images, source) = (images.clone(), source(uri, &kept));
            ImageSource::Custom(Arc::new(move |_: &mut Window, cx: &mut App| match &source {
                Some(source) => images.image(source, waiter, cx),
                None => Some(Err(ImageCacheError::Asset("not an image that's drawn".into()))),
            }))
        }
    }

    /// The image from `source` if it's here, an error if it can't be shown, or `None` while it loads, which it starts
    /// doing the first time it's asked for.
    pub(super) fn image(
        &self,
        source: &Source,
        waiter: W,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        let mut images = self.0.images.lock();
        match images.get_mut(source) {
            Some(Linked::Ready(image, _)) => return Some(Ok(image.clone())),
            Some(Linked::Unreadable) => return Some(Err(ImageCacheError::Asset("it can't be shown".into()))),
            Some(Linked::Loading(waiting)) => {
                if !waiting.contains(&waiter) {
                    waiting.push(waiter);
                }
                return None;
            }
            None => {}
        }
        images.insert(source.clone(), Linked::Loading(vec![waiter]));
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
                    let format = image_format(MediaType::sniff(&bytes))?;
                    let Drawable { bytes, says } = drawable(format, bytes)?;
                    let image = Image::from_bytes(format, bytes).to_image_data(renderer).ok()?;
                    let covers = says.or_else(|| pixels(&image))?;
                    Some(Linked::Ready(image, covers))
                })
                .unwrap_or(Linked::Unreadable)
        });
        cx.spawn(async move |cx| {
            let linked = loading.await;
            let waiting = match images.0.images.lock().insert(source, linked) {
                Some(Linked::Loading(waiting)) => waiting,
                _ => Vec::new(),
            };
            cx.update(|cx| (images.0.arrived)(waiting, cx));
        })
        .detach();
    }
}

/// An image's bytes as they're safe to draw, and the size it says it is, if it's a drawing, since GPUI keeps that to
/// itself: it draws one at twice that, to be sharp on a screen of twice the pixels, and smaller than that when it's
/// very large.
struct Drawable {
    bytes: Vec<u8>,
    says: Option<(u32, u32)>,
}

/// `bytes`, an image of `format`, as they're safe to draw. A drawing is made safe first, as `drawings` says, and is
/// `None` if it can't be.
fn drawable(format: ImageFormat, bytes: Vec<u8>) -> Option<Drawable> {
    if format != ImageFormat::Svg {
        return Some(Drawable { bytes, says: None });
    }
    let drawing = drawings::safe(&bytes)?;
    Some(Drawable { bytes: drawing.svg, says: Some(drawing.size) })
}

/// How many pixels a bitmap covers as it's drawn, which is as many as it has, or `None` for one with none, which has
/// no shape to draw it at.
fn pixels(image: &RenderImage) -> Option<(u32, u32)> {
    let pixels = image.size(0);
    let side = |pixels: i32| u32::try_from(pixels).ok().filter(|side| *side > 0);
    Some((side(pixels.width.0)?, side(pixels.height.0)?))
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
