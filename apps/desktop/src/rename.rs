//! Renaming a workspace or a chat, in a dialog with its title in a field, selected, so typing replaces it, and what
//! to say when a title asked of the hub doesn't come.

use client_core::{ClientError, Hub};
use gpui_kit::component::input::InputState;
use gpui_kit::{App, AppContext as _, ParentElement as _, Window};
use protocol::model::{Title, TitleError};

/// Opens the dialog over the window. `rename` gets what was typed once it's confirmed, for the hub to tidy. A title
/// that would be empty keeps the dialog open, and any other the hub refuses comes back as an error from it. Without a
/// connection to `hub` that has caught up, Rename is off and confirming keeps the dialog open with what's typed, as it
/// reads each time it's drawn, so Rename comes on once one has.
pub(crate) fn open(
    what: Renaming,
    hub: Hub,
    current: &str,
    window: &mut Window,
    cx: &mut App,
    rename: impl Fn(String, &mut App) + 'static,
) {
    // Made here rather than in the builder, which can run again whenever the dialog is drawn.
    let input = cx.new(|cx| InputState::new(window, cx).default_value(current.to_string()));
    let rename = std::rc::Rc::new(rename);
    let field = input.clone();
    look::open_dialog(window, cx, move |_, _| {
        let (typed, rename, hub) = (field.clone(), rename.clone(), hub.clone());
        let confirm = look::button("rename-confirm", look::ButtonKind::Main, look::ControlSize::Large, "Rename");
        look::dialog(format!("Rename {}", what.noun()))
            .child(look::field("rename-field", &field, look::FieldKind::Boxed, look::ControlSize::Large))
            .closes(look::button("rename-cancel", look::ButtonKind::Edged, look::ControlSize::Large, "Cancel"))
            .confirms(confirm.off(!hub.caught_up()))
            .on_confirm(move |_, cx| {
                let title = typed.read(cx).value().to_string();
                if !hub.caught_up() || Title::typed(&title) == Err(TitleError::Empty) {
                    return false;
                }
                rename(title, cx);
                true
            })
    });
    // After opening, which focuses the dialog itself.
    input.update(cx, |input, cx| {
        input.select_all(window, cx);
        input.focus(window, cx);
    });
}

/// What to say when the hub couldn't title a workspace again.
pub(crate) fn regenerating_failed(e: &ClientError) -> String {
    client_core::refusals::couldnt("regenerate the title", e)
}

/// What's being renamed, which the dialog's title says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Renaming {
    Workspace,
    Chat,
}

impl Renaming {
    /// What to say when renaming fails.
    pub(crate) fn failed(self, e: &ClientError) -> String {
        client_core::refusals::couldnt(&format!("rename the {}", self.noun()), e)
    }

    fn noun(self) -> &'static str {
        match self {
            Renaming::Workspace => "workspace",
            Renaming::Chat => "chat",
        }
    }
}
