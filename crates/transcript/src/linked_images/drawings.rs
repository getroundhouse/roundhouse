//! The SVG drawings a text points to, made safe to draw before GPUI sees them. GPUI draws an SVG with `usvg`'s default
//! options, which read any file an `<image>` in it names, by its path anywhere on the machine, and read on and on
//! from one such as `/dev/zero`, until the app runs out of memory. What an agent writes can't be trusted to point only
//! where it should, as `crate::linked_images` says, so each drawing is read here first, with any image it names by a
//! path or an address left out, and an image it holds as a `data:` address kept, and written out again as the SVG
//! GPUI draws. That also gives the size the drawing says it is, which GPUI keeps to itself.

use std::sync::{Arc, LazyLock};
use usvg::{ImageHrefResolver, Options, Tree, WriteOptions, fontdb};

/// A drawing as it's safe to draw: its SVG, which holds every image it shows, and the size it says it is, in pixels.
pub(super) struct Drawing {
    pub(super) svg: Vec<u8>,
    pub(super) size: (u32, u32),
}

/// The drawing `bytes` describe, safe to draw, or `None` if they aren't one, or it has no size. Slow, since its text
/// is laid out with the machine's fonts, so it's for another thread.
pub(super) fn safe(bytes: &[u8]) -> Option<Drawing> {
    let resolver = ImageHrefResolver {
        resolve_data: ImageHrefResolver::default_data_resolver(),
        // Anything but a `data:` address, such as a path, isn't read.
        resolve_string: Box::new(|_, _| None),
    };
    let options = Options { image_href_resolver: resolver, fontdb: system_fonts(), ..Options::default() };
    let tree = Tree::from_data(bytes, &options).ok()?;
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a side of a drawing, in whole pixels")]
    let whole = |side: f32| Some(side.ceil() as u32).filter(|side| *side > 0);
    let size = (whole(tree.size().width())?, whole(tree.size().height())?);
    // Its words as words, since GPUI lays them out again with the fonts it has.
    let written = tree.to_string(&WriteOptions { preserve_text: true, ..WriteOptions::default() });
    Some(Drawing { svg: written.into_bytes(), size })
}

/// The machine's fonts, found the first time they're needed, which a drawing's words are laid out with, as GPUI lays
/// them out.
pub(crate) fn system_fonts() -> Arc<fontdb::Database> {
    static FONTS: LazyLock<Arc<fontdb::Database>> = LazyLock::new(|| {
        let mut fonts = fontdb::Database::new();
        fonts.load_system_fonts();
        Arc::new(fonts)
    });
    FONTS.clone()
}

#[cfg(test)]
mod tests {
    use super::safe;
    use protocol::attachment::AttachmentBytes;

    /// An image a drawing names by its path, anywhere on the machine, isn't read, and one it holds is kept.
    #[test]
    fn a_drawing_shows_no_file_it_names_by_its_path() {
        let dir = tempfile::tempdir().unwrap();
        let elsewhere = dir.path().join("elsewhere.png");
        // A red pixel, as a PNG.
        let pixel = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==";
        std::fs::write(&elsewhere, AttachmentBytes::from_base64(pixel).unwrap().into_vec()).unwrap();
        let held = format!("data:image/png;base64,{pixel}");
        let drawing = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="60" height="30">
                 <image href="{}" width="30" height="30"/>
                 <image href="{held}" x="30" width="30" height="30"/>
               </svg>"#,
            elsewhere.display()
        );

        let safe = safe(drawing.as_bytes()).expect("a drawing");

        let written = String::from_utf8(safe.svg).unwrap();
        assert_eq!(written.matches("<image").count(), 1, "only the one it holds: {written}");
        assert!(!written.contains("elsewhere"), "{written}");
        assert_eq!(safe.size, (60, 30), "the size it says");
    }

    #[test]
    fn what_isnt_a_drawing_is_none() {
        assert!(safe(b"not a drawing").is_none());
        assert!(safe(br#"<svg xmlns="http://www.w3.org/2000/svg" width="0" height="10"/>"#).is_none(), "no size");
    }
}
