//! A panel of a page of its own, such as Git's and MCP servers', raised as Agents' are: its title and what it says under
//! that, what's in it, then its buttons, and the choices one offers. Wherever it is on the page, it's brought into sight
//! as it opens, as its end is once there's more at it, and gives Settings the focus back as it goes.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::base::ElementExt as _;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Bounds, ClickEvent, Div, FocusHandle, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, Role, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _,
    Window, div, px, size,
};
use look::{IconSizing as _, Lined as _, Typeset as _};

use super::redraw::bring_into_sight;

/// Where an open panel's focus is, and what of it is still to be brought into sight, which the page's state keeps with
/// it.
pub(super) struct Place {
    /// Holds the focus of everything in the panel, to tell whether it has it as it goes.
    focus: FocusHandle,
    /// What to bring into sight as the panel is next laid out.
    reveal: Option<Reveal>,
}

/// What of a panel is brought into sight.
enum Reveal {
    /// All of it, as it opens.
    Whole,
    /// Its end, where its buttons are, once there's more at it.
    End,
}

impl Place {
    /// Of a panel that's opening.
    pub(super) fn opening(cx: &App) -> Place {
        Place { focus: cx.focus_handle(), reveal: Some(Reveal::Whole) }
    }

    /// What the panel says at its end has changed, so its end is brought into sight as it's next laid out.
    pub(super) fn end_changed(&mut self) {
        self.reveal.get_or_insert(Reveal::End);
    }

    /// Moves the focus to the panel itself when something in it has it, as what has it goes, so keys still reach
    /// Settings.
    pub(super) fn keep_focus(&self, window: &mut Window, cx: &mut App) {
        self.give_focus(&self.focus, window, cx);
    }

    /// Moves the focus to `to` when something in the panel has it, as the panel goes, so keys still reach Settings.
    pub(super) fn give_focus(&self, to: &FocusHandle, window: &mut Window, cx: &mut App) {
        if self.focus.contains_focused(window, cx) {
            window.focus(to, cx);
        }
    }

    /// Brings what's still to be brought into sight there, on Settings' `scroll`, now that the panel is laid out at
    /// `bounds`. This is during a draw, after the page has been placed by the scroll as it was, so the scroll shows in
    /// the frame after, which is asked for once this one is done, since the window can't be marked for drawing again
    /// while it draws.
    pub(super) fn laid_out(&mut self, bounds: Bounds<Pixels>, scroll: &ScrollHandle, window: &mut Window) {
        let revealed = match self.reveal.take() {
            Some(Reveal::Whole) => bounds,
            Some(Reveal::End) => Bounds::new(bounds.bottom_left(), size(bounds.size.width, px(0.))),
            None => return,
        };
        if bring_into_sight(scroll, revealed) {
            window.on_next_frame(|window, _| window.refresh());
        }
    }
}

/// What a panel says and holds.
pub(super) struct Framed {
    pub(super) title: SharedString,
    pub(super) lead: SharedString,
    pub(super) body: Option<AnyElement>,
    /// Why the hub refused what the panel asked, in red, as an alert, so assistive apps say it as it comes.
    pub(super) refused: Option<String>,
    pub(super) buttons: Div,
}

/// A panel at `place` on the `page` of that name, as `framed` says, which tells `laid_out` where it was laid out. Tests
/// know it by the page's name and `-panel`.
pub(super) fn framed(
    page: &'static str,
    place: &Place,
    framed: Framed,
    laid_out: impl Fn(Bounds<Pixels>, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let refused = framed.refused.map(|refused| refusal(page, refused, cx));
    let card = look::card(look::CardKind::InPlace, look::CardWidth::Fills)
        .roomy()
        .heading(look::card_heading(framed.title))
        .under_heading(said_under_the_title(page, framed.lead, cx))
        .children(framed.body)
        .children(refused)
        .child(framed.buttons);
    div()
        .id(SharedString::from(format!("{page}-panel")))
        .track_focus(&place.focus)
        .relative()
        .child(card)
        .on_prepaint(laid_out)
        .test_support()
        .into_any_element()
}

/// Why the hub, or another system, refused what a panel asked, in red, as an alert, so assistive apps say it as it
/// comes. Tests find it by the page's name and `-refused`.
pub(super) fn refusal(page: &'static str, refused: String, cx: &App) -> impl IntoElement + use<> {
    let refused = SharedString::from(refused);
    let id = SharedString::from(format!("{page}-refused"));
    let said = div().id(id).role(Role::Alert).aria_label(refused.clone());
    said.typeset(look::Type::Body).text_color(look::colors(cx).error).child(refused).test_support()
}

/// What a panel says under its title, which tests read by the page's name and `-panel-says`, as assistive apps do.
fn said_under_the_title(page: &'static str, said: SharedString, cx: &App) -> impl IntoElement + use<> {
    let id = SharedString::from(format!("{page}-panel-says"));
    let said_as = div().id(id).role(Role::Label).aria_label(said.clone());
    said_as.text_color(look::colors(cx).text_muted).child(said).test_support()
}

/// A panel's buttons, at its end.
pub(super) fn buttons() -> Div {
    h_flex().justify_end().gap(look::Space::Small)
}

/// That a panel waits, beside a still circle.
pub(super) fn waiting_line(said: impl Into<SharedString>, cx: &App) -> Div {
    h_flex()
        .gap(look::Space::Small)
        .text_color(look::colors(cx).text_muted)
        .child(Icon::new(Lucide::LoaderCircle).sized(look::IconSize::Regular).flex_none())
        .child(said.into())
}

/// One of a panel's buttons, known to tests by `id`: its main one when it's `primary`, which can be `off`.
pub(super) fn button(
    id: &'static str,
    label: impl Into<SharedString>,
    primary: bool,
    off: bool,
    act: impl Fn(&mut Window, &mut App) + 'static,
) -> look::Button {
    let kind = if primary { look::ButtonKind::Main } else { look::ButtonKind::Outlined };
    look::button(id, kind, look::ControlSize::Regular, label)
        .off(off)
        .on_click(move |_: &ClickEvent, window, cx| act(window, cx))
}

/// What a panel offers to choose from, each a [`choice`], with a line between each.
pub(super) fn choices(choices: impl IntoIterator<Item = Div>, cx: &App) -> Div {
    let rows = choices.into_iter().enumerate();
    v_flex().children(rows.map(|(at, row)| row.when(at > 0, |row| row.line_over(cx))))
}

/// A way to go on that a panel offers: its icon, its name, with what's said faintly `beside` it, if anything, what it
/// takes under that, and its button.
pub(super) fn choice(
    icon: Lucide,
    name: &'static str,
    beside: Option<&'static str>,
    says: impl Into<SharedString>,
    act: look::Button,
    cx: &App,
) -> Div {
    let colors = look::colors(cx);
    let icon = Icon::new(icon).sized(look::IconSize::Large).text_color(colors.text_muted);
    let beside = beside.map(|beside| div().typeset(look::Type::Detail).text_color(colors.text_faint).child(beside));
    let name = h_flex().items_baseline().gap(look::Space::Small).child(div().emphasis().child(name)).children(beside);
    h_flex()
        .items_start()
        .gap(look::Space::Large)
        .py(look::Space::Large)
        .child(div().flex_none().child(icon))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(look::Space::XSmall)
                .child(name)
                .child(div().text_color(colors.text_muted).child(says.into())),
        )
        .child(h_flex().flex_none().justify_end().child(act))
}
