use super::{drawings, drawn};
use crate::attachments::image_format;
use crate::diagrams::{self, Palette};
use crate::transcript::Transcript;
use crate::videos::{Still, VideoPlayer, VideoSource, still};
use client_core::{Hub, Round};
use gpui_kit::component::text::MarkdownExtensions;
use gpui_kit::{
    App, AppContext as _, Entity, Image, ImageCacheError, ImageFormat, ImageSource, RenderImage, SharedUri, WeakEntity,
    Window,
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
    /// What the images are read from.
    pub(super) hub: Hub,
    /// The hub's round when what couldn't be read was last let go of, so it's asked for again once on each new one.
    pub(super) round: Mutex<Round>,
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
    /// A video, which plays where it's drawn alone, and whose still is drawn anywhere else, with how many pixels it
    /// covers.
    Video(Entity<VideoPlayer>, Drawn),
    /// The hub couldn't give it, or it isn't an image, so it's not asked for again until a new connection has caught
    /// up.
    Unreadable,
}

/// An image as it's drawn, with how many pixels it covers, as [`drawable`] and [`pixels`] say.
pub(super) type Drawn = (Arc<RenderImage>, (u32, u32));

/// Where an image a text points to comes from, if it's drawn.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum Source {
    /// The copy the hub kept when the text came.
    Kept(AttachmentId),
    /// The copy the hub kept of a video, which is read a part at a time as it plays, rather than whole.
    Video(AttachmentId),
    /// Written into the text, by its whole address.
    Embedded(String),
    /// A diagram written into the text, by what it says, drawn in a palette, as `crate::diagrams` draws it.
    Diagram(Arc<str>, Palette),
}

impl LinkedImages<Seq> {
    /// The images the texts of the transcript `transcript` point to, each of whose rows is measured again when one it
    /// waited for comes.
    pub(crate) fn of_transcript(hub: Hub, session: SessionId, transcript: WeakEntity<Transcript>) -> Self {
        Self::new(hub, session, move |waiting, cx| {
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
        hub: Hub,
        session: SessionId,
        arrived: impl Fn(Vec<W>, &mut App) + Send + Sync + 'static,
    ) -> Self {
        let (images, round) = (Mutex::new(HashMap::new()), Mutex::new(hub.round()));
        Self(Arc::new(Shared { session, hub, round, images, arrived: Box::new(arrived) }))
    }

    /// Once a new connection has caught up, as the hub's round says, asks again for what couldn't be read, in case the
    /// connection was why, and stops each video playing, since what it was reading went with the last. A diagram that
    /// couldn't be drawn isn't drawn again, since nothing it's drawn from has changed. Its owner calls this whenever
    /// it's told something changed.
    pub(crate) fn caught_up_again(&self, cx: &mut App) {
        let round = self.0.hub.round();
        if std::mem::replace(&mut *self.0.round.lock(), round) == round {
            return;
        }
        let mut players = Vec::new();
        self.0.images.lock().retain(|source, linked| match linked {
            Linked::Video(player, _) => {
                players.push(player.clone());
                true
            }
            Linked::Unreadable => matches!(source, Source::Diagram(..)),
            Linked::Ready(..) | Linked::Loading(_) => true,
        });
        for player in players {
            player.update(cx, VideoPlayer::stop_for_new_connection);
        }
    }

    /// How a text drawn from the kit's blocks, which names `kept`, for `waiter`, draws the images it points to, and
    /// whatever `and` adds to how it's drawn, such as the issues it names.
    pub(crate) fn images(
        &self,
        waiter: W,
        kept: &[LinkedImage],
        and: impl FnOnce(MarkdownExtensions) -> MarkdownExtensions,
    ) -> Images {
        Images {
            plugins: Arc::new(and(drawn::to_draw(self, waiter, kept))),
            resolver: Box::new(self.resolver(waiter, kept)),
        }
    }

    /// The image at `uri` in a text that names `kept`, for `waiter`, and how many pixels it covers as it's drawn, once
    /// it's here, or `None` while it loads, if it can't be shown, or if it isn't one that's drawn.
    pub(super) fn ready(&self, uri: &str, kept: &[LinkedImage], waiter: W, cx: &mut App) -> Option<Drawn> {
        self.drawn(&source(uri, kept)?, waiter, cx)?.ok()
    }

    /// The image from `source`, for `waiter`, and how many pixels it covers as it's drawn, once it's here, an error if
    /// it can't be shown, or `None` while it loads, which it starts doing the first time it's asked for.
    pub(super) fn drawn(&self, source: &Source, waiter: W, cx: &mut App) -> Option<Result<Drawn, ImageCacheError>> {
        if let Err(e) = self.image(source, waiter, cx)? {
            return Some(Err(e));
        }
        match self.0.images.lock().get(source) {
            Some(Linked::Ready(image, covers)) => Some(Ok((image.clone(), *covers))),
            Some(Linked::Video(_, drawn)) => Some(Ok(drawn.clone())),
            _ => None,
        }
    }

    /// What plays the video at `uri` in a text that names `kept`, once its still is here, or `None` while it loads,
    /// if it can't be played, or if it isn't a video.
    pub(super) fn player(&self, uri: &str, kept: &[LinkedImage]) -> Option<Entity<VideoPlayer>> {
        match self.0.images.lock().get(&source(uri, kept)?) {
            Some(Linked::Video(player, _)) => Some(player.clone()),
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
            Some(Linked::Ready(image, _) | Linked::Video(_, (image, _))) => return Some(Ok(image.clone())),
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
        let (images, hub, session) = (self.clone(), self.0.hub.clone(), self.0.session.clone());
        let round = hub.round();
        let (renderer, reading) = (cx.svg_renderer(), source.clone());
        let loading = cx.background_spawn(async move {
            let bytes = match reading {
                Source::Kept(id) => hub.read_attachment(&session, &id).ok(),
                Source::Video(id) => {
                    let video = VideoSource::Kept { hub, session, id };
                    return still(&video).map(|still| Read::Video(video, still));
                }
                Source::Embedded(uri) => embedded(&uri),
                Source::Diagram(text, palette) => diagrams::svg(&text, &palette),
            };
            let bytes = bytes?;
            let format = image_format(MediaType::sniff(&bytes))?;
            let Drawable { bytes, says } = drawable(format, bytes)?;
            let image = Image::from_bytes(format, bytes).to_image_data(renderer).ok()?;
            let covers = says.or_else(|| pixels(&image))?;
            Some(Read::Image(image, covers))
        });
        cx.spawn(async move |cx| {
            let read = loading.await;
            let linked = cx.update(|cx| match read {
                Some(Read::Image(image, covers)) => Some(Linked::Ready(image, covers)),
                Some(Read::Video(video, still)) => {
                    let drawn = (still.image.clone(), still.covers);
                    Some(Linked::Video(cx.new(|cx| VideoPlayer::new(video, still, cx)), drawn))
                }
                // Read on a connection before the one that has caught up now, so it's asked for again as it's next
                // drawn, rather than kept as unreadable.
                None if images.0.hub.round() != round => None,
                None => Some(Linked::Unreadable),
            });
            let mut held = images.0.images.lock();
            let was = match linked {
                Some(linked) => held.insert(source, linked),
                None => held.remove(&source),
            };
            drop(held);
            let waiting = match was {
                Some(Linked::Loading(waiting)) => waiting,
                _ => Vec::new(),
            };
            cx.update(|cx| (images.0.arrived)(waiting, cx));
        })
        .detach();
    }
}

/// What's read off the main thread of what a text points to, for the main thread to keep: an image, with how many
/// pixels it covers, or a video's still, with where the rest of it comes from.
enum Read {
    Image(Arc<RenderImage>, (u32, u32)),
    Video(VideoSource, Still),
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

/// Where the image or the video at `uri` comes from, from a Markdown image's address and what its text's block names,
/// or `None` if it isn't drawn: on the web, elsewhere on the machine, in the workspace without a copy the hub kept, or
/// a copy of a kind this build doesn't know, which a newer hub may keep, and which isn't read to find out what it is.
fn source(uri: &str, kept: &[LinkedImage]) -> Option<Source> {
    if uri.starts_with("data:") {
        return Some(Source::Embedded(uri.to_string()));
    }
    let image = kept.iter().find(|image| image.address == uri)?;
    let (id, media_type) = (image.attachment.id.clone(), image.attachment.media_type);
    if media_type.is_video() { Some(Source::Video(id)) } else { media_type.is_image().then_some(Source::Kept(id)) }
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

    /// A copy of a kind this build doesn't know isn't read, since a newer hub may keep something big, such as a video
    /// it has a new kind for.
    #[test]
    fn a_copy_of_a_kind_this_build_doesn_t_know_isn_t_read() {
        let attachment = Attachment {
            id: AttachmentId::from("a1".to_string()),
            name: "clip.mkv".into(),
            media_type: MediaType::Unknown,
            size_bytes: 3,
        };
        let kept = [LinkedImage { address: "clip.mkv".into(), attachment }];

        assert_eq!(source("clip.mkv", &kept), None);
    }

    #[test]
    fn an_image_written_into_the_text_is_its_own_bytes_and_one_on_the_web_isnt_drawn() {
        assert_eq!(embedded("data:image/png;base64,R0lGODlh"), Some(b"GIF89a".to_vec()));
        assert_eq!(embedded("data:image/svg+xml,<svg/>"), None);
        assert_eq!(source("https://example.com/shot.png", &[]), None);
        assert_eq!(source("", &[]), None);
    }
}
