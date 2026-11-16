//! Vector QR code tile for a pairing link, painted in one GPUI canvas element with no bitmap blur.

use gpui_kit::{
    AnyElement, App, Bounds, IntoElement as _, ParentElement as _, Styled as _, canvas, div, fill, point, px, size,
};
use look::{Corner, Shaped as _};
use qrcode::QrCode;

#[derive(Clone)]
pub(super) struct QrMatrix {
    width: usize,
    dark: Vec<bool>,
}

impl QrMatrix {
    pub(super) fn of(text: &str) -> Option<Self> {
        let code = QrCode::new(text.as_bytes()).ok()?;
        let width = code.width();
        let dark = code.into_colors().into_iter().map(|c| c == qrcode::Color::Dark).collect();
        Some(Self { width, dark })
    }
}

const TILE_SIDE: f32 = 108.0;
const QUIET_ZONE: f32 = 8.0;

pub(super) fn render(matrix: &QrMatrix, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let (light, dark_ink) = (colors.main, colors.pane);
    let matrix = matrix.clone();
    let inner = TILE_SIDE - QUIET_ZONE * 2.0;
    let painted = canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            if matrix.width == 0 {
                return;
            }
            let step = f32::from(bounds.size.width) / to_f32(matrix.width);
            for row in 0..matrix.width {
                for col in 0..matrix.width {
                    if matrix.dark[row * matrix.width + col] {
                        let origin =
                            point(bounds.origin.x + px(to_f32(col) * step), bounds.origin.y + px(to_f32(row) * step));
                        let quad_size = size(px(step), px(step));
                        window.paint_quad(fill(Bounds::new(origin, quad_size), dark_ink));
                    }
                }
            }
        },
    )
    .size(px(inner));

    div()
        .flex_none()
        .size(px(TILE_SIDE))
        .p(px(QUIET_ZONE))
        .bg(light)
        .corners(Corner::Control)
        .child(painted)
        .into_any_element()
}

#[expect(clippy::cast_precision_loss, reason = "QR version widths are at most 177 modules")]
fn to_f32(modules: usize) -> f32 {
    modules as f32
}
