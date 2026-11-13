use std::collections::BTreeMap;

use gpui_kit::TestAppContext;
use gpui_kit::component::{ActiveTheme as _, try_parse_color};

use super::{Colors, config, kit_colors};
use crate::colors::colors;
use crate::shape::Corner;

/// So a key the kit doesn't know, or a value it can't read, which it would pass over for one of its own when the theme
/// is applied, fails here.
#[test]
fn the_kit_reads_every_colour_its_given_as_its_given() {
    let colors = Colors::dark();
    let given: BTreeMap<String, String> =
        kit_colors(&colors).into_iter().map(|(key, value)| (key.to_string(), value)).collect();

    let read = serde_json::to_value(config(&colors).colors).unwrap();

    let read: BTreeMap<String, String> = read
        .as_object()
        .unwrap()
        .iter()
        .filter_map(|(key, value)| Some((key.clone(), value.as_str()?.to_string())))
        .collect();
    assert_eq!(read, given);
    for (key, value) in &given {
        assert!(try_parse_color(value).is_ok(), "{key} is {value}, which the kit can't read");
    }
}

/// What the kit draws its own parts with is the app's colour, including the selection, which the kit holds to 30%.
#[gpui_kit::test]
fn the_kits_theme_draws_with_the_apps_colours(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    cx.update(super::apply);

    cx.update(|cx| {
        let (theme, colors) = (cx.theme(), colors(cx));
        assert_eq!(theme.background, colors.pane);
        assert_eq!(theme.muted_foreground, colors.text_muted);
        assert_eq!(theme.muted, colors.inset);
        assert_eq!(theme.border, colors.edge);
        assert_eq!(theme.warning, colors.warning);
        assert_eq!(theme.popover, colors.raised);
        assert_eq!(theme.list_hover, colors.row_hover);
        assert_eq!(theme.ring, colors.brand);
        assert_eq!(theme.selection, colors.selection());
        assert_eq!(theme.radius, Corner::Control.radius());
        assert_eq!(theme.radius_lg, Corner::Card.radius());
    });
}
