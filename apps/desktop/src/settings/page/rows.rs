//! A setting's row: what it's called and what it does on the left, its control on the right, and a line under it that
//! sets it apart from the next.

use gpui_kit::component::button::Button;
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::{Anchor, AnyElement, App, IntoElement, ParentElement as _, Styled as _, div};
use look::{Hand as _, Off as _, Typeset as _};

use super::{Control, Setting};

pub(super) fn row(setting: &Setting, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    // Narrower than what it says, if need be, so a long line wraps rather than pushing the control off the page.
    let said = v_flex()
        .flex_1()
        .min_w_0()
        .gap_1()
        .child(div().emphasis().child(setting.name))
        .child(div().text_color(colors.text_muted).child(setting.does));
    let control = match &setting.control {
        Control::Switch { is_on, set } => {
            // One that can't be read or changed just now shows off and can't be flipped.
            let on = is_on(cx);
            let set = set.clone();
            Switch::new(setting.name)
                .accessibility_label(setting.name)
                .checked(on.unwrap_or(false))
                .off_while(on.is_none())
                .on_click(move |on: &bool, _, cx: &mut App| set(*on, cx))
                .into_any_element()
        }
        Control::Choice { id, chosen, menu } => {
            let menu = menu.clone();
            let chosen = chosen(cx);
            Button::new(*id)
                .accessibility_label(format!("{}: {chosen}", setting.name))
                .hand()
                .outline()
                .small()
                .dropdown_caret(true)
                .label(chosen)
                .dropdown_menu_with_anchor(Anchor::TopRight, move |popup, window, cx| menu(popup, window, cx))
                .into_any_element()
        }
    };
    h_flex()
        .w_full()
        .items_center()
        .justify_between()
        .gap_6()
        .pt_1()
        .pb_5()
        .border_b_1()
        .border_color(colors.edge)
        .typeset(look::Type::Body)
        .child(said)
        // As wide as it needs, however much there is to say beside it.
        .child(div().flex_none().child(control))
        .into_any_element()
}
