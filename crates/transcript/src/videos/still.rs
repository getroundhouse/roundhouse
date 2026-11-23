use gpui_kit::RenderImage;
use image::{Frame, RgbaImage};
use std::sync::Arc;
use std::time::Duration;
use video::{Rows, Shown};

use super::VideoSource;

/// A video's first frame, which is drawn until it's played, how many pixels that covers, and how long the file says
/// the video is.
#[derive(Clone)]
pub struct Still {
    pub image: Arc<RenderImage>,
    pub covers: (u32, u32),
    pub length: Option<Duration>,
}

/// The still of the video from `source`, or `None` if it isn't one the app plays, or it couldn't be read. It waits on
/// the hub, so it's for a background thread.
pub fn still(source: &VideoSource) -> Option<Still> {
    let mut video = source.open().ok()?;
    let length = video.duration();
    let frame = video.next_frame().ok()??;
    let (width, height) = (u32::try_from(frame.picture.width).ok()?, u32::try_from(frame.picture.height).ok()?);
    let mut bgra = Vec::new();
    frame.picture.to_bgra(&mut Rows::default(), &mut bgra);
    let image = render_image(Shown { seek: 0, time: frame.time, width, height, bgra })?;
    Some(Still { image: Arc::new(image), covers: (width, height), length })
}

/// A frame as GPUI draws it, or `None` for one with no pixels. GPUI takes an image's pixels as blue, green, red and
/// alpha, which is how a frame has them, though the image crate's type for them says red first.
pub(super) fn render_image(shown: Shown) -> Option<RenderImage> {
    let pixels =
        RgbaImage::from_raw(shown.width, shown.height, shown.bgra).filter(|_| shown.width > 0 && shown.height > 0)?;
    Some(RenderImage::new(vec![Frame::new(pixels)]))
}
