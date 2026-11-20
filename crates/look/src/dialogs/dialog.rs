//! The app's dialog: a title, what it says under it, what's in it and its answers along its foot, raised over the
//! window, which it dims. It holds the kit's dialog and draws with it, but gives none of the kit's or GPUI's styling to
//! whoever makes it, so every dialog is laid out the same. `docs/standards/components.md` says why, and how a component's
//! API is laid out.

use std::rc::Rc;

use gpui_kit::component::dialog::{Dialog as KitDialog, DialogAction, DialogClose};
use gpui_kit::component::{WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
};

use super::DialogWidth;
use crate::colors::colors;
use crate::controls::Button;
use crate::shape::{Corner, Shaped as _};
use crate::space::Space;
use crate::typography::{Type, Typeset as _};

/// What confirming does, told the window, and whether the dialog closes after it.
type Confirm = Rc<dyn Fn(&mut Window, &mut App) -> bool>;

/// What's done once the dialog closes.
type Closed = Rc<dyn Fn(&mut Window, &mut App)>;

/// A dialog that asks or says `title`, with nothing in it yet, as wide as [`DialogWidth::Regular`]. It's opened with
/// [`open_dialog`], which builds it each time it's drawn.
pub fn dialog(title: impl Into<SharedString>) -> Dialog {
    Dialog {
        title: title.into(),
        lead: None,
        body: Vec::new(),
        start: None,
        answers: Vec::new(),
        width: DialogWidth::Regular,
        on_confirm: None,
        on_close: None,
        careful: false,
    }
}

/// Opens the dialog `build` makes over the window, built again each time it's drawn, so it can show what's changed,
/// such as a button that's waiting. Make what it keeps, such as a field's state, before opening it, not in `build`.
///
/// Escape and a click outside it close it, and Enter confirms it, as its button that confirms does.
pub fn open_dialog(window: &mut Window, cx: &mut App, build: impl Fn(&mut Window, &mut App) -> Dialog + 'static) {
    window.open_dialog(cx, move |surface, window, cx| build(window, cx).drawn(surface, cx));
}

/// Closes the dialog in front, as the app does once what it waited on has come, rather than as someone closes it, so
/// what's done once it closes isn't.
pub fn close_dialog(window: &mut Window, cx: &mut App) {
    window.close_dialog(cx);
}

/// Whether a dialog is open over the window, and not on its way out, such as for what the keyboard's focus goes to.
pub fn dialog_open(window: &mut Window, cx: &mut App) -> bool {
    window.has_active_dialog(cx)
}

/// Closes every dialog over the window, as the app does when what they were about has gone, such as the hub they
/// asked of, without what's done as someone closes one.
pub fn close_every_dialog(window: &mut Window, cx: &mut App) {
    window.close_all_dialogs(cx);
}

/// One of a dialog's answers along its foot.
enum Answer {
    /// Closes the dialog, as Cancel does.
    Closes(Button),
    /// Confirms it, as Enter does.
    Confirms(Button),
    /// Does what its own handler says, such as copying a link.
    Own(Button),
}

/// A dialog, made with [`dialog`] and opened with [`open_dialog`].
#[must_use]
pub struct Dialog {
    title: SharedString,
    lead: Option<SharedString>,
    body: Vec<AnyElement>,
    start: Option<AnyElement>,
    answers: Vec<Answer>,
    width: DialogWidth,
    on_confirm: Option<Confirm>,
    on_close: Option<Closed>,
    careful: bool,
}

impl Dialog {
    /// What it says under its title, in muted text, such as what answering does.
    pub fn lead(mut self, lead: impl Into<SharedString>) -> Self {
        self.lead = Some(lead.into());
        self
    }

    /// Something at the start of its foot, across from its answers, such as a link to read more.
    pub fn start(mut self, element: impl IntoElement) -> Self {
        self.start = Some(element.into_any_element());
        self
    }

    /// An answer that closes it, as Cancel does, which goes before the one that confirms it.
    pub fn closes(mut self, button: Button) -> Self {
        self.answers.push(Answer::Closes(button));
        self
    }

    /// The answer that confirms it, as Enter does unless it's [`Self::careful`], which goes last.
    pub fn confirms(mut self, button: Button) -> Self {
        self.answers.push(Answer::Confirms(button));
        self
    }

    /// An answer that does what its own handler says, such as copying a link, in the order it's added.
    pub fn answer(mut self, button: Button) -> Self {
        self.answers.push(Answer::Own(button));
        self
    }

    /// How wide it is.
    pub fn width(mut self, width: DialogWidth) -> Self {
        self.width = width;
        self
    }

    /// Asks before what would stop or lose something, so Enter closes it, keeping things as they are, as its answer
    /// that closes does, and only its button confirms it, as quitting with something unsent asks.
    pub fn careful(mut self) -> Self {
        self.careful = true;
        self
    }

    /// What confirming it does, with its button or Enter, which says whether it closes then, as a field that's empty
    /// keeps it open. Without one, confirming closes it.
    pub fn on_confirm(mut self, confirm: impl Fn(&mut Window, &mut App) -> bool + 'static) -> Self {
        self.on_confirm = Some(Rc::new(confirm));
        self
    }

    /// What's done once someone closes it, however they do, after what confirming does if they confirmed it. Nothing
    /// is done when the app closes it itself, with [`close_dialog`].
    pub fn on_close(mut self, closed: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_close = Some(Rc::new(closed));
        self
    }

    /// Its title and what it says over what's in it, and its answers along its foot, as a dialog lays them out.
    fn laid_out(self, cx: &App) -> LaidOut {
        let colors = colors(cx);
        let (title, lead) = (self.title, self.lead);
        // Each with what it says for a test to read, as an assistive app does.
        let heading = v_flex()
            .gap(Space::XSmall)
            .child(
                div()
                    .id("dialog-title")
                    .aria_label(title.clone())
                    .typeset(Type::Heading)
                    .text_color(colors.text)
                    .child(title)
                    .test_support(),
            )
            .children(lead.map(|lead| {
                div()
                    .id("dialog-lead")
                    .aria_label(lead.clone())
                    .text_color(colors.text_muted)
                    .child(lead)
                    .test_support()
            }));
        let content = v_flex().gap(Space::Large).child(heading).children(self.body).into_any_element();
        let (on_confirm, on_close, careful) = (self.on_confirm, self.on_close, self.careful);
        // The kit's wrappers that close and confirm fill what they're in, so each is in a box of its button's size.
        let answers = self.answers.into_iter().map(|answer| match answer {
            Answer::Closes(button) => div().flex_none().child(DialogClose::new().child(button)).into_any_element(),
            // Its own press, since Enter, which is the kit's confirming, only closes it. The kit runs what's done once
            // it closes only as it closes it itself, so it's run here.
            Answer::Confirms(button) if careful => {
                let (confirm, closed) = (on_confirm.clone(), on_close.clone());
                button
                    .on_click(move |_, window, cx| {
                        if confirm.as_ref().is_none_or(|confirm| confirm(window, cx)) {
                            window.close_dialog(cx);
                            if let Some(closed) = &closed {
                                closed(window, cx);
                            }
                        }
                    })
                    .into_any_element()
            }
            Answer::Confirms(button) => div().flex_none().child(DialogAction::new().child(button)).into_any_element(),
            Answer::Own(button) => button.into_any_element(),
        });
        let foot =
            h_flex().gap(Space::Small).children(self.start).child(div().flex_1()).children(answers).into_any_element();
        // A careful dialog's Enter only closes it.
        let on_confirm = if careful { None } else { on_confirm };
        LaidOut { content, foot, width: self.width, on_confirm, on_close }
    }

    /// The kit's dialog, drawn as this one: raised, with a card's corners and the one edge, its title and what it says
    /// over what's in it, which scrolls when it's taller than the window, and its answers along its foot.
    fn drawn(self, surface: KitDialog, cx: &App) -> KitDialog {
        let colors = colors(cx);
        let LaidOut { content, foot, width, on_confirm, on_close } = self.laid_out(cx);
        surface
            // The kit lays its own padding along the sides of what's in it and its foot, and the gap between them.
            .p(Space::XLarge)
            .gap(Space::Large)
            .w(width.width())
            .close_button(false)
            .bg(colors.raised)
            .border_color(colors.edge)
            .corners(Corner::Card)
            .typeset(Type::Body)
            .text_color(colors.text)
            .child(content)
            .footer(foot)
            .on_ok(move |_, window, cx| on_confirm.as_ref().is_none_or(|confirm| confirm(window, cx)))
            .when_some(on_close, |surface, closed| surface.on_close(move |_, window, cx| closed(window, cx)))
    }
}

/// A dialog's parts, laid out, with what it does as it's confirmed and closed.
struct LaidOut {
    content: AnyElement,
    foot: AnyElement,
    width: DialogWidth,
    on_confirm: Option<Confirm>,
    on_close: Option<Closed>,
}

/// `dialog`, drawn where it's put rather than opened over a window, for a sheet of how dialogs look: raised, with the
/// padding, edge and corners the kit's dialog is given.
#[cfg(any(test, feature = "testing"))]
pub fn shown_dialog(dialog: Dialog, cx: &App) -> AnyElement {
    let colors = colors(cx);
    let LaidOut { content, foot, width, .. } = dialog.laid_out(cx);
    v_flex()
        .w(width.width())
        .p(Space::XLarge)
        .gap(Space::Large)
        .bg(colors.raised)
        .border(crate::shape::EDGE)
        .border_color(colors.edge)
        .corners(Corner::Card)
        .typeset(Type::Body)
        .text_color(colors.text)
        .child(content)
        .child(foot)
        .into_any_element()
}

impl ParentElement for Dialog {
    /// What's in it, under what it says, such as a field.
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.body.extend(elements);
    }
}
