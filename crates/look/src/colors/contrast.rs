use super::srgb::Srgb;

/// How many times [`least_reaching`] halves what it searches, which finds where a colour reaches a contrast far more
/// finely than a byte of colour can show it.
const HALVINGS: u32 = 40;

/// How readable text is on what's behind it, as APCA's Lc measures it, in its version 0.0.98G-4g, without the sign
/// that says which of the two is lighter. It's about 90 for body text at its best, at least 75 for body text, 60 for
/// other text that's read, 30 for any text at all and 15 for what isn't text. It's used rather than WCAG 2's ratio,
/// which says two dark colours are further apart than they look, so it can't judge a dark theme.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub(super) struct Contrast(pub(super) f64);

impl Contrast {
    /// Of `text` on `page`, whichever is the lighter.
    pub(super) fn of(text: Srgb, page: Srgb) -> Self {
        let (text, page) = (luminance(text), luminance(page));
        if (page - text).abs() < 0.0005 {
            return Contrast(0.0);
        }
        let lc = if page > text {
            let dark_on_light = (page.powf(0.56) - text.powf(0.57)) * 1.14;
            if dark_on_light < 0.1 { 0.0 } else { dark_on_light - 0.027 }
        } else {
            let light_on_dark = (page.powf(0.65) - text.powf(0.62)) * 1.14;
            if light_on_dark > -0.1 { 0.0 } else { light_on_dark + 0.027 }
        };
        Contrast((lc * 100.0).abs())
    }
}

/// The colour `along` gives at the least of `from` to `to` whose contrast on `page` reaches `contrast`, where the colours
/// it gives grow further from the page from `from` to `to`, such as text from the page's lightness toward white.
pub(super) fn least_reaching(
    contrast: Contrast,
    page: Srgb,
    (from, to): (f64, f64),
    along: impl Fn(f64) -> Srgb,
) -> Srgb {
    let (mut short, mut reaches) = (from, to);
    for _ in 0..HALVINGS {
        let middle = f64::midpoint(short, reaches);
        if Contrast::of(along(middle), page) < contrast {
            short = middle;
        } else {
            reaches = middle;
        }
    }
    along(reaches)
}

/// How much light `color` gives, as APCA measures it, with its soft clamp near black, where the eye can't tell much
/// apart.
fn luminance(color: Srgb) -> f64 {
    let [r, g, b] = color.0.map(|channel| (f64::from(channel) / 255.0).powf(2.4));
    let y = 0.212_672_9 * r + 0.715_152_2 * g + 0.072_175 * b;
    if y < 0.022 { y + (0.022 - y).powf(1.414) } else { y }
}
