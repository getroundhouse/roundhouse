use std::rc::Rc;

use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, ClickEvent, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, RenderOnce,
    SharedString, Styled as _, TestSupportExt as _, Window, div, px,
};

use crate::colors::colors;
use crate::heights::RowSize;
use crate::lines::Lined as _;
use crate::list_rows::list_row;
use crate::raised::raised;
use crate::space::Space;
use crate::typography::{Type, Typeset as _};

use super::dimmed::OnTap as OnClick;

/// The ID of what an action sheet dims, which a tap on cancels.
const BEHIND: &str = "action-sheet-behind";
/// The ID of its Cancel.
const CANCEL: &str = "action-sheet-cancel";

/// What a handheld asks before doing something that can't be had back from where it's asked, such as archiving from a
/// menu, as an iPhone's action sheet does: `asks` over the choices, along the screen's bottom, over what it dims. It
/// fills what it's put in, as [`crate::sheet`] does, so whatever owns what's open draws it last.
pub fn action_sheet(asks: impl Into<SharedString>) -> ActionSheet {
    ActionSheet { asks: asks.into(), says: None, choices: Vec::new(), clear: px(0.), on_cancel: None }
}

/// A handheld's action sheet, which [`action_sheet`] makes.
#[derive(IntoElement)]
#[must_use]
pub struct ActionSheet {
    asks: SharedString,
    says: Option<SharedString>,
    choices: Vec<Choice>,
    clear: Pixels,
    on_cancel: Option<OnClick>,
}

struct Choice {
    id: ElementId,
    words: SharedString,
    removes: bool,
    on_click: OnClick,
}

impl ActionSheet {
    /// More about what's asked, under it, such as why it asks.
    pub fn says(mut self, says: impl Into<SharedString>) -> Self {
        self.says = Some(says.into());
        self
    }

    /// A choice that removes something, such as Archive, in the error's colour, first, where it's seen first.
    pub fn removes(
        mut self,
        id: impl Into<ElementId>,
        words: impl Into<SharedString>,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        let choice = Choice { id: id.into(), words: words.into(), removes: true, on_click: Rc::new(on_click) };
        self.choices.insert(0, choice);
        self
    }

    /// Another choice, after the one that removes.
    pub fn choice(
        mut self,
        id: impl Into<ElementId>,
        words: impl Into<SharedString>,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        let choice = Choice { id: id.into(), words: words.into(), removes: false, on_click: Rc::new(on_click) };
        self.choices.push(choice);
        self
    }

    /// Clear of `bottom`, what the device draws over the screen's bottom, such as its home indicator.
    pub fn clear(mut self, bottom: Pixels) -> Self {
        self.clear = bottom;
        self
    }

    /// What Cancel, or a tap on what it dims, does, which is to put it away without doing anything.
    pub fn on_cancel(mut self, on_cancel: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_cancel = Some(Rc::new(on_cancel));
        self
    }
}

impl RenderOnce for ActionSheet {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = colors(cx);
        let asked = v_flex()
            .px(Space::Large)
            .py(Space::Medium)
            .gap(Space::XSmall)
            .typeset(Type::Detail)
            .text_color(colors.text_muted)
            .text_center()
            .child(div().emphasis().child(self.asks))
            .children(self.says);
        let choices = self.choices.into_iter().map(|choice| {
            let color = if choice.removes { colors.error } else { colors.text };
            let on_click = choice.on_click;
            let row = list_row(choice.id, RowSize::Large)
                .child(div().flex_1().text_center().text_color(color).child(choice.words))
                .on_click(move |event, window, cx| on_click(event, window, cx));
            div().w_full().line_over(cx).child(row)
        });
        let group = raised(cx).w_full().px(crate::ROWS_INSET).child(asked).children(choices);
        let on_cancel = self.on_cancel;
        let cancel = list_row(CANCEL, RowSize::Large)
            .child(div().flex_1().text_center().emphasis().child("Cancel"))
            .when_some(on_cancel.clone(), |row, on_cancel| {
                row.on_click(move |event, window, cx| on_cancel(event, window, cx))
            });
        let cancel = raised(cx).w_full().px(crate::ROWS_INSET).child(cancel);
        let sheet = v_flex()
            .id("action-sheet")
            .test_support()
            .occlude()
            .w_full()
            .px(Space::Medium)
            .pb(self.clear + Space::Small.size())
            .gap(Space::Small)
            .text_color(colors.text)
            .typeset(Type::Body)
            .child(group)
            .child(cancel);
        super::dimmed::over_what_it_dims(BEHIND, on_cancel, sheet, cx)
    }
}
