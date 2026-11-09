//! Renaming a workspace or a chat, in a dialog with its title in a field, selected, so typing replaces it.

use client_core::ClientError;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::{DialogAction, DialogClose, DialogFooter};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::{App, AppContext as _, ParentElement as _, Window};
use protocol::model::{Title, TitleError};

/// Opens the dialog over the window. `rename` gets what was typed once it's confirmed, for the hub to tidy. A title
/// that would be empty keeps the dialog open, and any other the hub refuses comes back as an error from it.
pub(crate) fn open(
    what: Renaming,
    current: &str,
    window: &mut Window,
    cx: &mut App,
    rename: impl Fn(String, &mut App) + 'static,
) {
    // Made here rather than in the builder, which can run again whenever the dialog is drawn.
    let input = cx.new(|cx| InputState::new(window, cx).default_value(current.to_string()));
    let rename = std::rc::Rc::new(rename);
    let field = input.clone();
    window.open_dialog(cx, move |dialog, _, _| {
        let (typed, rename) = (field.clone(), rename.clone());
        dialog
            .title(format!("Rename {}", what.noun()))
            .child(Input::new(&field))
            .footer(
                DialogFooter::new()
                    .child(DialogClose::new().trigger(|button| button.label("Cancel")))
                    .child(DialogAction::new().child(Button::new("rename-confirm").primary().label("Rename"))),
            )
            .on_ok(move |_, _, cx| {
                let title = typed.read(cx).value().to_string();
                if Title::typed(&title) == Err(TitleError::Empty) {
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

/// What's being renamed, which the dialog's title says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Renaming {
    Workspace,
    Chat,
}

impl Renaming {
    /// What to say when renaming fails.
    pub(crate) fn failed(self, e: &ClientError) -> String {
        let noun = self.noun();
        if e.is_unsupported() {
            return format!("couldn't rename the {noun}: the hub is older than this app");
        }
        format!("couldn't rename the {noun}: {e}")
    }

    fn noun(self) -> &'static str {
        match self {
            Renaming::Workspace => "workspace",
            Renaming::Chat => "chat",
        }
    }
}
