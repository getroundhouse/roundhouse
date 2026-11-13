//! The app's theme, over the kit's dark one, made from the app's colours, so the kit's own parts, such as its
//! buttons, menus and rich text, draw with the same ones.

use std::rc::Rc;

use gpui_kit::component::{Theme, ThemeConfig, ThemeMode};
use gpui_kit::{App, Hsla, Rgba};
use serde_json::{Map, Value, json};

use crate::colors::Colors;
use crate::typography::{MONO, MONO_SIZE, REM};

/// Installs the app's colours, and its theme as the kit's dark theme, and switches to it. Going through the kit,
/// rather than setting colours on the theme directly, is what lets its other layers, such as rich text, pick the
/// colours up too.
pub fn apply(cx: &mut App) {
    let colors = Colors::dark();
    let config = Rc::new(config(&colors));
    cx.set_global(colors);
    if !cx.has_global::<Theme>() {
        Theme::change(ThemeMode::Dark, None, cx);
    }
    Theme::global_mut(cx).dark_theme = config.clone();
    Theme::change(ThemeMode::Dark, None, cx);
    // The kit can't tell a code font chosen to be the same as its own default from no choice, and swaps either for
    // one it finds installed. It finds that once for the whole process, from whichever app asks first, and in a test
    // that can be one that lists no fonts, which would leave every later picture with code in the system font. The
    // theme's is on every Mac, so it goes back as chosen.
    if let Some(mono) = config.mono_font_family.clone() {
        Theme::global_mut(cx).mono_font_family = mono;
        Theme::sync_base(cx);
    }
}

/// The theme, in the form the kit reads a theme's file in, since that's the only way to give it some of its colours,
/// such as `base.red`.
#[expect(clippy::expect_used, reason = "the theme is made here, and a test checks that the kit reads all of it")]
fn config(colors: &Colors) -> ThemeConfig {
    let kit: Map<String, Value> =
        kit_colors(colors).into_iter().map(|(key, value)| (key.to_string(), Value::String(value))).collect();
    serde_json::from_value(json!({
        "name": "Roundhouse Dark",
        "mode": "dark",
        "font.size": f32::from(REM),
        "mono_font.family": MONO,
        "mono_font.size": f32::from(MONO_SIZE),
        "radius": 6,
        "radius.lg": 10,
        "shadow": false,
        "colors": kit,
    }))
    .expect("the kit reads the theme")
}

/// The kit's name for each of its colours, and the app's colour it takes: the role whose value it had, and of those,
/// the one nearest what the kit uses it for. A few are drawn only by the kit's own parts, such as the text on its
/// danger button, and have no role, so they're given here.
fn kit_colors(c: &Colors) -> Vec<(&'static str, String)> {
    [surfaces(c), controls(c), meanings(c)].concat()
}

fn surfaces(c: &Colors) -> Vec<(&'static str, String)> {
    vec![
        ("background", hex(c.pane)),
        ("foreground", hex(c.text)),
        ("border", hex(c.edge)),
        ("window.border", hex(c.edge)),
        ("muted.background", hex(c.inset)),
        ("muted.foreground", hex(c.text_muted)),
        ("secondary.background", hex(c.raised)),
        ("secondary.foreground", hex(c.text)),
        ("secondary.hover.background", hex(c.raised_hover)),
        ("secondary.active.background", hex(c.raised_pressed)),
        ("accent.background", hex(c.menu_hover)),
        ("accent.foreground", hex(c.text)),
        ("overlay", hex(c.dim)),
        ("popover.background", hex(c.raised)),
        ("popover.foreground", hex(c.text)),
        ("sidebar.background", hex(c.sidebar)),
        ("sidebar.foreground", hex(c.text)),
        ("sidebar.border", hex(c.edge)),
        ("sidebar.accent.background", hex(c.edge)),
        ("sidebar.accent.foreground", "#f6f3ef".into()),
        ("sidebar.primary.background", hex(c.main)),
        ("sidebar.primary.foreground", hex(c.on_main)),
        ("tab.background", hex(c.pane)),
        ("tab.active.background", hex(c.raised)),
        ("tab.foreground", hex(c.text_muted)),
        ("tab.active.foreground", hex(c.text)),
        ("tab_bar.background", hex(c.pane)),
        ("title_bar.background", hex(c.sidebar)),
        ("title_bar.border", hex(c.edge)),
        ("list.background", hex(c.pane)),
        ("list.hover.background", hex(c.row_hover)),
        ("list.active.background", hex(c.edge)),
        ("list.active.border", "#00000000".into()),
        ("scrollbar.background", "#00000000".into()),
        ("scrollbar.thumb.background", hex(c.scroll_thumb)),
        ("scrollbar.thumb.hover.background", hex(c.scroll_thumb_hover)),
        ("skeleton.background", hex(c.raised)),
    ]
}

fn controls(c: &Colors) -> Vec<(&'static str, String)> {
    vec![
        ("primary.background", hex(c.main)),
        ("primary.foreground", hex(c.on_main)),
        ("primary.hover.background", hex(c.main_hover)),
        ("primary.active.background", hex(c.main_pressed)),
        ("button.background", hex(c.button)),
        ("button.foreground", hex(c.text)),
        ("button.hover.background", hex(c.raised_hover)),
        ("button.active.background", hex(c.raised_pressed)),
        ("button.primary.background", hex(c.main)),
        ("button.primary.foreground", hex(c.on_main)),
        ("button.primary.hover.background", hex(c.main_hover)),
        ("button.primary.active.background", hex(c.main_pressed)),
        ("input.border", hex(c.control_edge)),
        ("ring", hex(c.brand)),
        ("caret", hex(c.brand)),
        // The kit holds a selection to at most 30%, so it takes the brand's copper at that, as `selection` is.
        ("selection.background", hex(c.brand)),
        ("link", hex(c.link)),
        ("link.hover", hex(c.link_hover)),
        ("link.active", hex(c.link_hover)),
    ]
}

/// The kit's colours for what happened. It has fewer meanings than the app, so its green and yellow take the roles
/// they had the value of, `added` and `modified`.
fn meanings(c: &Colors) -> Vec<(&'static str, String)> {
    vec![
        ("danger.background", hex(c.error)),
        ("danger.foreground", "#fff1ef".into()),
        ("danger.hover.background", "#f2827a".into()),
        ("danger.active.background", "#dc5f55".into()),
        ("warning.background", hex(c.warning)),
        ("warning.foreground", hex(c.on_main)),
        ("success.background", hex(c.added)),
        ("success.foreground", "#0f1a12".into()),
        ("info.background", "#6aa6f8".into()),
        ("info.foreground", "#0d1420".into()),
        ("base.red", hex(c.error)),
        ("base.green", hex(c.added)),
        ("base.yellow", hex(c.modified)),
        ("base.blue", "#6aa6f8".into()),
        ("base.magenta", "#d58fd8".into()),
        ("base.cyan", "#5fc9d6".into()),
    ]
}

/// `#rrggbb`, or `#rrggbbaa` for a colour that isn't opaque, as the kit reads a colour, and back to the same bytes
/// the colour was made from, as a test checks.
pub(crate) fn hex(color: Hsla) -> String {
    let Rgba { r, g, b, a } = color.into();
    let [r, g, b, a] = [r, g, b, a].map(byte);
    if a == u8::MAX { format!("#{r:02x}{g:02x}{b:02x}") } else { format!("#{r:02x}{g:02x}{b:02x}{a:02x}") }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a channel is between 0 and 1, so it rounds to a byte"
)]
fn byte(channel: f32) -> u8 {
    (channel.clamp(0., 1.) * 255.).round() as u8
}

#[cfg(test)]
mod tests;
