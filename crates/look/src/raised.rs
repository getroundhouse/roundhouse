//! What opens over the rest, such as a card or what a hover shows: raised over the pane, with a shadow under it.

use gpui_kit::{App, BoxShadow, Div, Pixels, Styled as _, div, point, px};

use crate::colors::{Colors, colors};
use crate::shape::{Corner, EDGE, Shaped as _};

/// The shadow, as `docs/spec/look.md` gives it: a soft one well below, and a tighter one just under the edge.
const SHADOW: [Layer; 2] =
    [Layer { below: px(8.), blur: px(12.), strength: 0.225 }, Layer { below: px(2.), blur: px(3.), strength: 0.15 }];

/// One of the shadow's layers: how far below the surface it falls, how far it blurs, as GPUI measures a blur, and how
/// much of the shadow's colour it takes.
struct Layer {
    below: Pixels,
    blur: Pixels,
    strength: f32,
}

/// An empty surface for what opens over the rest, in the raised colour with the one thin edge, a card's corners and
/// the shadow under it. Whoever fills it says how wide it is and how much room its content has.
pub fn raised(cx: &App) -> Div {
    let colors = colors(cx);
    div()
        .corners(Corner::Card)
        .border(EDGE)
        .border_color(colors.edge)
        .bg(colors.raised)
        .text_color(colors.text)
        .shadow(shadow(colors))
}

fn shadow(colors: &Colors) -> Vec<BoxShadow> {
    let layer = |layer: &Layer| BoxShadow {
        color: colors.shadow.opacity(layer.strength),
        offset: point(px(0.), layer.below),
        blur_radius: layer.blur,
        spread_radius: px(0.),
        inset: false,
    };
    SHADOW.iter().map(layer).collect()
}
