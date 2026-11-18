use std::rc::Rc;

use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::component::{Icon, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Action, App, InteractiveElement as _, IntoElement as _, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};

use super::NoteKind;
use crate::colors::colors;
use crate::space::Space;
use crate::typography::{Type, Typeset};

/// What choosing a row does.
type Choose = Rc<dyn Fn(&mut Window, &mut App) + 'static>;

/// A row of a menu that says `words`, which choosing does what [`MenuRow::on_choose`] is given.
pub fn menu_row(words: impl Into<SharedString>) -> MenuRow {
    MenuRow {
        words: words.into(),
        icon: None,
        key: None,
        note: None,
        chosen: false,
        off: false,
        tooltip: None,
        choose: None,
    }
}

/// A row of a menu, which [`menu_row`] makes and [`crate::Menu::row`] adds.
#[must_use]
pub struct MenuRow {
    words: SharedString,
    icon: Option<Icon>,
    key: Option<Box<dyn Action>>,
    note: Option<(SharedString, NoteKind)>,
    chosen: bool,
    off: bool,
    tooltip: Option<SharedString>,
    choose: Option<Choose>,
}

impl MenuRow {
    /// An icon before its words, for what choosing it does or what it names, such as a hub's machine.
    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// The key that does what choosing it does, at its end, from the action that key is bound to.
    pub fn key(mut self, action: Box<dyn Action>) -> Self {
        self.key = Some(action);
        self
    }

    /// A note at its end, of a kind, such as a hub's address.
    pub fn note(mut self, note: impl Into<SharedString>, kind: NoteKind) -> Self {
        self.note = Some((note.into(), kind));
        self
    }

    /// Checked at its end while `chosen`, as the one of a few that's chosen.
    pub fn chosen(mut self, chosen: bool) -> Self {
        self.chosen = chosen;
        self
    }

    /// Greyed out while `off`, so it can't be chosen.
    pub fn off(mut self, off: bool) -> Self {
        self.off = off;
        self
    }

    /// What it says while the pointer rests on it, such as why choosing it starts again. Assistive apps are told it
    /// too, as its description, since they don't read a tooltip out.
    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    /// What choosing it does.
    pub fn on_choose(mut self, choose: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.choose = Some(Rc::new(choose));
        self
    }

    /// The kit's row: its own, where the kit draws all of it, and one drawn here where it has a note or a tooltip.
    pub(super) fn kit(self) -> PopupMenuItem {
        let MenuRow { words, icon, key, note, chosen, off, tooltip, choose } = self;
        let item = if note.is_some() || tooltip.is_some() {
            PopupMenuItem::element(move |_, cx| drawn(&words, note.as_ref(), tooltip.as_ref(), cx))
        } else {
            PopupMenuItem::new(words)
        };
        item.when_some(icon, PopupMenuItem::icon)
            .when_some(key, PopupMenuItem::action)
            .checked(chosen)
            .disabled(off)
            .when_some(choose, |item, choose| item.on_click(move |_, window, cx| choose(window, cx)))
    }
}

/// A row's words, cut short where there isn't room, then its note, in a detail's text and its kind's colour. Assistive
/// apps are told the note with the words, and what the tooltip says as its description.
fn drawn(
    words: &SharedString,
    note: Option<&(SharedString, NoteKind)>,
    tooltip: Option<&SharedString>,
    cx: &App,
) -> gpui_kit::AnyElement {
    let colors = colors(cx);
    let label = match note {
        Some((note, _)) => SharedString::from(format!("{words}, {note}")),
        None => words.clone(),
    };
    h_flex()
        .id(words.clone())
        .flex_1()
        .min_w_0()
        .gap(Space::Small)
        .aria_label(label)
        .when_some(tooltip.cloned(), |row, tooltip| {
            let described = tooltip.clone();
            row.aria_description(described).tooltip(crate::tooltip(tooltip))
        })
        .child(div().flex_1().min_w_0().truncate().child(words.clone()))
        .when_some(note.cloned(), |row, (note, kind)| {
            row.child(
                div()
                    .flex_none()
                    .typeset(Type::Detail)
                    .when(kind == NoteKind::Count, Typeset::emphasis)
                    .text_color(kind.color(colors))
                    .child(note),
            )
        })
        .into_any_element()
}

impl gpui_kit::prelude::FluentBuilder for MenuRow {}
