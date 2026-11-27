use gpui_kit::component::{Icon, IconName, Sizable as _};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render, Styled as _, TestAppContext,
    Window, div,
};

use super::{IconSize, IconSizing as _};
use crate::ControlSize;
use crate::spec_table;
use crate::traits::Reach;
use crate::typography::{Type, rem};

#[test]
fn the_specs_table_has_every_icon_size_in_order_as_the_app_sizes_it() {
    let table: Vec<[String; 3]> = spec_table::rows("**Every size of an icon, by its name.**");

    let sized: Vec<[String; 3]> = IconSize::ALL.iter().map(|size| size.written()).collect();
    assert_eq!(table, sized, "docs/spec/look.md's table of icon sizes, against crates/look/src/icon_sizes/sizing.rs");
}

/// An icon set among text is as big as a detail among a detail's or code's, and as big as body text among it, at each
/// reach, at the app's own scale.
#[test]
fn an_icon_among_text_is_as_big_as_the_text() {
    for reach in Reach::ALL {
        let text = |kind: Type| kind.size_at(reach, rem(reach));

        assert_eq!(IconSize::Small.size_at(reach), text(Type::Detail), "{reach:?}");
        assert_eq!(IconSize::Regular.size_at(reach), text(Type::Body), "{reach:?}");
    }
}

/// One icon, in a box that's as big as what it draws, so the box's bounds are the icon's.
struct Drawn(Icon);

impl Render for Drawn {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().flex().items_start().child(div().flex_none().debug_selector(|| "icon".into()).child(self.0.clone()))
    }
}

/// How wide and tall `icon` is, as it's drawn.
fn drawn(cx: &mut TestAppContext, icon: Icon) -> [Pixels; 2] {
    let (_, cx) = cx.add_window_view(move |_, _| Drawn(icon));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let bounds = cx.debug_bounds("icon").unwrap();
    [bounds.size.width, bounds.size.height]
}

#[gpui_kit::test]
fn an_icon_is_drawn_as_big_as_its_size(cx: &mut TestAppContext) {
    for &size in IconSize::ALL {
        let icon = Icon::new(IconName::Check).sized(size);

        assert_eq!(drawn(cx, icon), [size.size(); 2], "{size:?}");
    }
}

/// The kit's Copy button takes only the kit's size, and sizes its icon by it, inside it, where a style from outside
/// doesn't reach, so this holds the icon the kit draws at that size to a small square's.
#[gpui_kit::test]
fn a_copy_buttons_icon_is_a_small_squares(cx: &mut TestAppContext) {
    let icon = Icon::new(IconName::Copy).with_size(crate::copy::KIT_SIZE);

    assert_eq!(drawn(cx, icon), [ControlSize::Small.icon().size(); 2]);
}
