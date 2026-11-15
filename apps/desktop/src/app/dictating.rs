//! What the window hears for dictation, wherever focus is: Space being let go of, and another app coming in front. Each
//! pane stops what it's dictating, if anything, and forgets Space held towards it.

use gpui_kit::base::{Root, RootPlugin};
use gpui_kit::{
    AnyElement, App, Context, Empty, InteractiveElement as _, IntoElement, KeyUpEvent, ParentElement as _, Render,
    Styled as _, Window, div,
};

use super::AppView;
use crate::workspace_pane::WorkspacePane;

impl AppView {
    /// Letting go of Space stops what holding it started, or was about to.
    fn dictation_key_up(&mut self, event: &KeyUpEvent, cx: &mut Context<Self>) {
        if crate::dictating::let_go(event, cx) {
            for pane in self.panes.values() {
                pane.update(cx, WorkspacePane::dictation_key_let_go);
            }
        }
    }

    /// Another app has come in front, which stops what's being dictated, as letting go of the key does.
    pub(super) fn dictation_window_left(&mut self, cx: &mut Context<Self>) {
        crate::dictating::window_left(cx);
        for pane in self.panes.values() {
            pane.update(cx, WorkspacePane::dictation_window_left);
        }
    }
}

/// Hears Space let go of before anything the window draws does, whatever has focus. The kit draws a dialog beside the
/// app's view, under the window's root, so only something around the whole root, as a plugin of the kit's is, has every
/// key on its way.
pub(crate) struct HearsTheKeyLetGo;

impl HearsTheKeyLetGo {
    /// For each window opened from now on.
    pub(crate) fn register(cx: &mut App) {
        Root::register_plugin::<Self>(cx, |_, _| Self);
    }
}

impl Render for HearsTheKeyLetGo {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

impl RootPlugin for HearsTheKeyLetGo {
    fn decorate(&self, surface: AnyElement, root: &Root, _: &mut Window, _: &mut App) -> impl IntoElement {
        let app = root.view().clone().downcast::<AppView>().ok();
        div()
            .size_full()
            .capture_key_up(move |event: &KeyUpEvent, _, cx| {
                if let Some(app) = &app {
                    app.update(cx, |app, cx| app.dictation_key_up(event, cx));
                }
            })
            .child(surface)
    }
}
