//! What MCP servers keeps from one frame to the next while it shows: the panel that's open, and what the hub last
//! couldn't do. How a sign-in stands is the window's, which the page follows. What a server is called, where it is and
//! how it knows the person are never kept here: each frame reads them from the hub's records, as client-core keeps them.

use gpui_kit::{Bounds, Context, Entity, FocusHandle, Pixels, ScrollHandle, Window};
use protocol::model::{ToolServer, ToolServerId};

use super::super::panel::Place;
use super::Asking;
use super::adding::AddPanel;
use super::words;
use crate::asking::{self, Question};
use crate::tool_sign_in::ToolSigningIn;

/// The page's state, kept only while it shows, so a key half pasted goes with it, and so does the wait for what the hub
/// answers.
pub(super) struct Kept {
    /// The sign-in there is, if there's one, which the page follows.
    pub(super) signing_in: Entity<ToolSigningIn>,
    /// The hub to ask, as of the page's last frame. `None` while there's no hub to ask.
    pub(super) asking: Option<Asking>,
    /// Settings' focus, as of the page's last frame, where focus goes back to as a panel goes while it has it, so keys
    /// still reach Settings.
    settings_focus: Option<FocusHandle>,
    pub(super) panel: Option<Panel>,
    /// Why the hub couldn't do what was last asked of a server, to say under its row.
    failed: Option<(ToolServerId, String)>,
}

/// The panel open under Added's heading.
pub(super) struct Panel {
    pub(super) shows: Shows,
    pub(super) place: Place,
}

pub(super) enum Shows {
    /// Add server, choosing how the server knows the person.
    Choosing,
    Adding(Box<AddPanel>),
    /// The sign-in there is, as it stands.
    SigningIn,
}

impl Kept {
    /// The state of a page that shows, following `signing_in`.
    pub(super) fn new(signing_in: Entity<ToolSigningIn>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe_in(&signing_in, window, |kept, _, window, cx| kept.follow_sign_in(window, cx)).detach();
        // One may have been started, or ended, before the page showed. This is while the window draws, so it's shown
        // once that's done.
        cx.defer_in(window, Self::follow_sign_in);
        Self { signing_in, asking: None, settings_focus: None, panel: None, failed: None }
    }

    /// What the page was drawn with: the hub to ask, and Settings' focus.
    pub(super) fn drawn(&mut self, asking: Option<Asking>, settings_focus: FocusHandle) {
        (self.asking, self.settings_focus) = (asking, Some(settings_focus));
    }

    /// Whether there's a hub to ask.
    pub(super) fn can_ask(&self) -> bool {
        self.asking.is_some()
    }

    /// Why the hub couldn't do what was last asked of the server `id`, if it couldn't.
    pub(super) fn failed(&self, id: &ToolServerId) -> Option<&str> {
        self.failed.as_ref().filter(|(server, _)| server == id).map(|(_, said)| said.as_str())
    }

    /// Opens a panel showing `shows`, in place of any that was open.
    pub(super) fn open(&mut self, shows: Shows, cx: &mut Context<Self>) {
        self.panel = Some(Panel { shows, place: Place::opening(cx) });
        self.failed = None;
        cx.notify();
    }

    /// Takes the panel away, with what's typed in it, and any wait for the hub's answer. Focus something in it had goes
    /// back to Settings, where keys still reach it.
    pub(super) fn put_away_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.panel.take() else { return };
        if let Some(settings) = &self.settings_focus {
            panel.place.give_focus(settings, window, cx);
        }
        cx.notify();
    }

    /// Gives the panel itself the focus, when something in it had it, as what had it goes, so keys still reach Settings.
    pub(super) fn focus_the_panel(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = &self.panel {
            panel.place.keep_focus(window, cx);
        }
    }

    /// The panel was laid out at `bounds`, on Settings' `scroll`, as [`Place::laid_out`] says.
    pub(super) fn laid_out(&mut self, bounds: Bounds<Pixels>, scroll: &ScrollHandle, window: &mut Window) {
        if let Some(panel) = &mut self.panel {
            panel.place.laid_out(bounds, scroll, window);
        }
    }

    /// Asks whether to have the hub forget `server`, saying what that forgets, and does once it's confirmed.
    pub(super) fn ask_to_remove(&mut self, server: &ToolServer, window: &mut Window, cx: &mut Context<Self>) {
        self.failed = None;
        let asked = Question {
            id: "remove-mcp-server",
            title: words::remove_asks(&server.name).into(),
            says: words::remove_says(&server.auth).into(),
            keep: "Cancel",
            act: "Remove",
            removes: true,
        };
        let (id, this) = (server.id.clone(), cx.weak_entity());
        asking::ask(asked, window, cx, move |_, cx| {
            let _ = this.update(cx, |this, cx| this.remove(id.clone(), cx));
        });
    }

    /// Has the hub forget the server `id`, and says under its row why it couldn't, if it couldn't, as with no hub to ask.
    /// One that's gone was removed already, as by another app, which is what was asked.
    fn remove(&mut self, id: ToolServerId, cx: &mut Context<Self>) {
        let Some(asking) = self.asking.clone() else {
            self.failed = Some((id, words::NO_HUB.to_string()));
            cx.notify();
            return;
        };
        let asked = id.clone();
        let answer = asking.client.start(move |client| match client.tool_server_remove(&asked) {
            Err(e) if !e.is_not_found() => Err(e),
            Ok(_) | Err(_) => Ok(()),
        });
        cx.spawn(async move |this, cx| {
            let answer = answer.recv().await;
            let _ = this.update(cx, |this, cx| {
                if let Err(e) = answer {
                    this.failed = Some((id, words::couldnt("remove it", &e)));
                }
                cx.notify();
            });
        })
        .detach();
    }
}
