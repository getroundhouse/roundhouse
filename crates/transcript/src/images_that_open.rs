//! The chat's images that a click opens over the window, for a closer look at one drawn small, or fitted to its
//! column or its line: one a prompt carried, one a tool sent back, or one the agent's text points to. What draws one
//! is [`opening`], and one drawn whole in the chat is [`whole`]. Opened, it's in the kit's dialog, as big as it is, up
//! to what fits in the window at its own shape, so the chat doesn't move under it however far from the bottom its row
//! is. Escape or a click closes it, on the image or around it.

use gpui_kit::component::dialog::DialogClose;
use gpui_kit::component::{ActiveTheme as _, TITLE_BAR_HEIGHT, WindowExt as _, h_flex, window_paddings};
use gpui_kit::{
    AnyElement, App, ClickEvent, Div, ImageSource, Img, InteractiveElement as _, IntoElement as _, ObjectFit,
    ParentElement as _, Pixels, Size, Stateful, StatefulInteractiveElement as _, Styled as _, StyledImage as _,
    TestSupportExt as _, Window, div, img, px, size,
};
use look::{Hand as _, IMAGE_EDGE, fit, pixels};

/// `frame`, for an image that a click on it opens over the window, with the hand over it. `covers` is the image's
/// pixels, turned as its orientation says. What it draws the image at, and the image, are the caller's.
pub(crate) fn opening(frame: Stateful<Div>, image: impl Into<ImageSource>, covers: (u32, u32)) -> Stateful<Div> {
    let image = image.into();
    frame.hand().on_click(move |_: &ClickEvent, window, cx| open(image.clone(), covers, window, cx))
}

/// `image`, which `covers` this many pixels, whole in `frame`, in the chat: as wide as it is, up to what it's in, at
/// its own shape, and [`opening`], so one made smaller to fit is as big as it is opened, if the window has room.
pub(crate) fn whole(frame: Stateful<Div>, image: impl Into<ImageSource>, covers: (u32, u32)) -> AnyElement {
    let image = image.into();
    let (width, height) = (pixels(covers.0), pixels(covers.1));
    let shown = opening(frame, image.clone(), covers)
        .w_full()
        .max_w(px(width))
        .aspect_ratio(width / height)
        .child(picture(image).size_full().object_fit(ObjectFit::Contain))
        .test_support();
    // In a row, since in a column the layout takes its height from the column's width rather than its own, which
    // leaves a gap under an image narrower than the column.
    h_flex().w_full().child(shown).into_any_element()
}

/// `image`, to draw. It's known by a name, which is what GPUI keeps an animated one's place by from one frame to the
/// next, so a GIF moves.
pub(crate) fn picture(image: impl Into<ImageSource>) -> Stateful<Img> {
    img(image).id("image")
}

fn open(image: ImageSource, covers: (u32, u32), window: &mut Window, cx: &mut App) {
    window.open_dialog(cx, move |dialog, window, cx| {
        // Drawn again as the window is resized, so it always fits.
        let shown = fitted(covers, room(window));
        // The dialog's own corners, which its edge is drawn on, less the edge, so the image's meet them.
        let corners = cx.theme().radius_lg - px(IMAGE_EDGE);
        let opened = div()
            .id("opened-picture")
            .w(shown.width)
            .h(shown.height)
            .hand()
            .child(picture(image.clone()).size_full().rounded(corners).object_fit(ObjectFit::Contain))
            .test_support();
        // The dialog's own padding and the least height it gives, which a short image would leave a strip under, are
        // let go, so the dialog is the image and its edge.
        dialog
            .p_0()
            .min_h(px(0.))
            .w(shown.width + px(2. * IMAGE_EDGE))
            .bg(look::colors(cx).inset)
            .centered()
            .close_button(false)
            .child(DialogClose::new().child(opened))
    });
}

/// The room in the window for an opened image: all of it but the window's own frame, where it draws one, and as much
/// around it as the strip along the top that the kit's dialogs leave to the window's buttons and to dragging it.
fn room(window: &Window) -> Size<Pixels> {
    let frame = window_paddings(window);
    let clear = TITLE_BAR_HEIGHT * 2.;
    let viewport = window.viewport_size();
    size(viewport.width - frame.left - frame.right - clear, viewport.height - frame.top - frame.bottom - clear)
}

/// How big an image `covers` pixels is drawn, inside its edge, to fit in `room`.
fn fitted((width, height): (u32, u32), room: Size<Pixels>) -> Size<Pixels> {
    let scale = fit((width, height), room);
    size(px(pixels(width) * scale), px(pixels(height) * scale))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Room 932 by 732, which leaves 930 by 730 for an image inside its edge.
    fn in_the_room(width: u32, height: u32) -> Size<Pixels> {
        fitted((width, height), size(px(932.), px(732.)))
    }

    #[test]
    fn an_image_that_fits_is_as_big_as_it_is() {
        assert_eq!(in_the_room(400, 300), size(px(400.), px(300.)));
        assert_eq!(in_the_room(930, 730), size(px(930.), px(730.)), "to the edge of the room");
    }

    #[test]
    fn one_too_big_shrinks_to_fit_at_its_own_shape() {
        assert_eq!(in_the_room(9300, 100), size(px(930.), px(10.)), "too wide");
        assert_eq!(in_the_room(100, 7300), size(px(10.), px(730.)), "too tall");
        assert_eq!(in_the_room(1860, 2920), size(px(465.), px(730.)), "both, by whichever is further over");
    }
}
