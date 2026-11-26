use gpui_kit::component::menu::PopupMenuStyle;
use gpui_kit::component::{Side, Size};

use crate::colors::Colors;
use crate::control_sizes::ControlSize;
use crate::raised::shadow;
use crate::shape::{Corner, EDGE};
use crate::space::Space;
use crate::theme::UNDER_POINTER;

/// The size of control each row is, as tall as a dialog's buttons, so a menu has room to read and to aim at.
pub(super) const ROW: ControlSize = ControlSize::Large;
/// The room between the menu's edge and its rows.
pub(super) const AROUND: Space = Space::XSmall;
/// The menu's corners, which its rows' follow, a step in.
pub(super) const CORNER: Corner = Corner::Card;
/// A row's corners, so they sit inside the menu's evenly.
pub(super) const ROW_CORNER: Corner = Corner::Control;

/// Every menu's style: rows as tall as a large control, a large icon in the muted text's colour, the row under the
/// pointer washed as a borderless button is, and the surface of what opens over the rest.
pub(crate) fn style(colors: &Colors) -> PopupMenuStyle {
    PopupMenuStyle {
        item_height: Some(ROW.height()),
        item_padding: Some(Space::Medium.size()),
        item_gap: Some(Space::Small.size()),
        item_radius: Some(ROW_CORNER.radius()),
        padding: Some(AROUND.size()),
        row_gap: Some(gpui_kit::px(0.)),
        icon_size: Some(Size::Size(ROW.icon_with_words().size())),
        icon_color: Some(colors.text_muted),
        hover: Some(UNDER_POINTER.of(colors.text)),
        separator_width: Some(EDGE),
        separator_margin: Some(Space::XSmall.size()),
        separator_overhang: Some(gpui_kit::px(0.)),
        // At the row's end, so a menu where only some rows can be chosen, such as the hub switcher, doesn't push them
        // all over to keep room for one, and a check takes room only in the row that has it.
        check_side: Some(Side::Right),
        radius: Some(CORNER.radius()),
        border: Some(colors.edge),
        shadow: Some(shadow(colors)),
    }
}
