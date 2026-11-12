//! Telling the person of each chat that has come to need them, by a notification and a sound as Settings chooses,
//! and taking back the notification of one that no longer does. Clicking a notification opens its chat.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::notification::Notification;
use gpui_kit::{Context, SharedString, Window};
use protocol::model::{SessionId, WorkspaceId};

use super::AppView;
use crate::notifications::{Came, Watch};

/// How long after a sound the next one waits, so several chats coming to need the person together sound once.
pub(super) const SOUNDS_APART: Duration = Duration::from_secs(2);

/// What a chat's notification is known by, with the chat's ID, so a newer one for the chat replaces it.
struct ChatNeedsYou;

/// What's been told, and when it last sounded.
#[derive(Default)]
pub(super) struct Telling {
    watch: Watch,
    /// By the app's clock, which a test can move.
    sounded_at: Option<Instant>,
}

impl AppView {
    /// Tells of each chat in a listed workspace that has come to need the person since the last look, unless it's the
    /// one being read, and takes back what told of one that no longer needs them. For after anything that changes the
    /// records or which chat is being read.
    pub(super) fn tell_of_needs(&mut self, cx: &mut Context<Self>) {
        let reading = self.selected_pane().and_then(|pane| pane.read(cx).chat_being_read().cloned());
        let listed: HashSet<WorkspaceId> = self.visible_workspaces().into_iter().map(|w| w.id.clone()).collect();
        let sessions = self.records.every_session().filter(|s| listed.contains(&s.workspace_id));
        let changes = self.telling.watch.look(sessions, reading.as_ref());
        if changes.came.is_empty() && changes.went.is_empty() {
            return;
        }
        let settings = crate::settings::get(cx);
        let now = cx.background_executor().now();
        let may_sound = self.telling.sounded_at.is_none_or(|at| now.duration_since(at) >= SOUNDS_APART);
        if settings.play_sound && !changes.came.is_empty() && may_sound {
            self.telling.sounded_at = Some(now);
            crate::sounds::play(&settings.sound, cx);
        }
        let notes: Vec<Notification> = if settings.notify {
            changes.came.into_iter().filter_map(|came| self.notification(came, cx)).collect()
        } else {
            Vec::new()
        };
        let (went, handle) = (changes.went, self.window);
        // Showing one needs the window, which this can be called in the middle of updating.
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                for chat in went {
                    window.remove_notification1::<ChatNeedsYou>(SharedString::from(chat.to_string()), cx);
                }
                for note in notes {
                    window.push_notification(note, cx);
                }
            });
        });
    }

    /// A chat's notification: its workspace's name, then why it needs the person, and the chat's title if it has one.
    /// `None` once its workspace has gone.
    fn notification(&self, came: Came, cx: &mut Context<Self>) -> Option<Notification> {
        let workspace = self.records.workspace(&came.workspace)?;
        let said = match &came.title {
            Some(title) => format!("{}: {title}", came.why.said()),
            None => came.why.said().to_string(),
        };
        let (view, at, chat) = (cx.weak_entity(), came.workspace, came.chat);
        let note = Notification::new()
            .id1::<ChatNeedsYou>(SharedString::from(chat.to_string()))
            .title(workspace.label().to_string())
            .message(said)
            .system()
            .on_click(move |_, window, cx| {
                let _ = view.update(cx, |t, cx| t.open_chat(at.clone(), chat.clone(), window, cx));
            });
        Some(note)
    }

    /// Shows a chat in its workspace, in place of Settings or Home, as a click on its notification does. The chat is
    /// chosen before its workspace shows, so no other chat there is read on the way.
    fn open_chat(&mut self, at: WorkspaceId, chat: SessionId, window: &mut Window, cx: &mut Context<Self>) {
        match self.panes.get(&at).cloned() {
            Some(pane) => pane.update(cx, |p, cx| p.activate(chat, cx)),
            None => self.chat_to_show = Some((at.clone(), chat)),
        }
        self.open_workspace(at, cx);
        self.close_settings(window, cx);
    }
}
