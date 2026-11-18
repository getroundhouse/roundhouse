use std::rc::Rc;

use gpui_kit::component::tooltip::TooltipStyle;
use gpui_kit::{IntoElement as _, ParentElement as _, Pixels, Styled as _, div, px};

use crate::colors::Colors;
use crate::keys::{KeyKind, key};
use crate::raised::shadow;
use crate::shape::{Corner, Shaped as _};
use crate::space::Space;
use crate::typography::{Type, Typeset as _};

/// The widest a tooltip is, past which its words wrap, so one that says more than a few words reads as a line or two
/// rather than running across the window.
pub(super) const WIDEST: Pixels = px(320.);
/// Its corners, a control's, since it's small, while a card or a menu has a card's.
pub(super) const CORNER: Corner = Corner::Control;

/// Every tooltip's style: what opens over the rest, raised, with the one edge and the one shadow, a control's corners
/// and a detail's text, and at most [`WIDEST`] wide, with its key a plain `look` key in the muted text's colour, which
/// the kit gives what it draws a key in.
pub(crate) fn style(colors: &Colors) -> TooltipStyle {
    let mut shaped = div()
        .bg(colors.raised)
        .border_color(colors.edge)
        .text_color(colors.text)
        .shadow(shadow(colors))
        .corners(CORNER)
        .typeset(Type::Detail)
        .px(Space::Small)
        .py(Space::XSmall)
        .max_w(WIDEST);
    TooltipStyle {
        style: Some(gpui_kit::Styled::style(&mut shaped).clone()),
        key: Some(Rc::new(|stroke, _, cx| {
            let binding = stroke.unparse(cx.platform_info().os);
            div().child(key(KeyKind::Plain, binding)).into_any_element()
        })),
    }
}
