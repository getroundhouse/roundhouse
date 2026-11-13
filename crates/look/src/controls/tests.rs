use gpui_kit::component::StyleSized as _;
use gpui_kit::component::button::Button;
use gpui_kit::{DefiniteLength, Length, Styled as _, div, px};

use super::{ControlSize, Sizing as _};
use crate::Typeset as _;
use crate::spec_table;

#[test]
fn the_specs_table_has_every_control_size_in_order_as_the_app_sizes_it() {
    let table: Vec<[String; 4]> = spec_table::rows("**Every size of a button, by its name.**");

    let sized: Vec<[String; 4]> = ControlSize::ALL.iter().map(|size| size.written()).collect();
    assert_eq!(table, sized, "docs/spec/look.md's table of control sizes, against crates/look/src/controls.rs");
}

#[test]
fn a_button_with_words_is_as_tall_as_its_size_with_its_room_at_each_end() {
    for &size in ControlSize::ALL {
        let mut sized = Button::new("words").sized(size);
        let style = sized.style();

        assert_eq!(style.size.height, Some(Length::from(size.height())), "{size:?}");
        assert_eq!(style.size.width, None, "{size:?}: as wide as its words");
        let room = Some(DefiniteLength::from(size.padding()));
        assert_eq!([style.padding.left, style.padding.right], [room; 2], "{size:?}");
    }
}

/// The kit sets a button's words from its size, inside the button, so this holds the kit's size to the kind of text
/// the spec says.
#[test]
fn a_buttons_words_are_the_kind_of_text_its_size_says() {
    for &size in ControlSize::ALL {
        let mut kit = div().button_text_size(size.kit_words());
        let mut kind = div().typeset(size.words());

        assert_eq!(kit.style().text.font_size, kind.style().text.font_size, "{size:?}");
    }
}

#[test]
fn a_button_thats_only_an_icon_is_a_square_of_its_size_with_no_room_taken() {
    for &size in ControlSize::ALL {
        let mut sized = Button::new("icon").sized_square(size);
        let style = sized.style();

        let side = Some(Length::from(size.height()));
        assert_eq!([style.size.width, style.size.height], [side; 2], "{size:?}");
        let none = Some(DefiniteLength::from(px(0.)));
        let padding = &style.padding;
        assert_eq!([padding.top, padding.right, padding.bottom, padding.left], [none; 4], "{size:?}");
    }
}
