//! The app's theme, over the kit's dark one.

use std::rc::Rc;

use gpui_kit::component::{Theme, ThemeConfig, ThemeMode};
use gpui_kit::{App, Window};

const THEME: &str = include_str!("theme.json");

/// Installs the app's theme as the kit's dark theme, and switches to it. Going through the kit, rather than setting
/// colours on the theme directly, is what lets its other layers, such as rich text, pick the colours up too.
pub fn apply(window: Option<&mut Window>, cx: &mut App) {
    if !cx.has_global::<Theme>() {
        Theme::change(ThemeMode::Dark, None, cx);
    }
    Theme::global_mut(cx).dark_theme = Rc::new(config());
    Theme::change(ThemeMode::Dark, window, cx);
}

#[expect(clippy::expect_used, reason = "the theme is built in, and a test checks that it parses")]
fn config() -> ThemeConfig {
    serde_json::from_str(THEME).expect("the built-in theme parses")
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_built_in_theme_parses() {
        let config = super::config();
        assert_eq!(config.name.as_ref(), "Roundhouse Dark");
    }
}
