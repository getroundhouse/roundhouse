//! Adding or saving what's in a form. One with a key or with None has the hub ask the server for its tools first, which
//! adds and changes nothing, and one that doesn't list them is added or saved only if the person has it done anyway.
//! What the check comes to is client-core's `tool_server_checks` to say.

use client_core::tool_server_checks::{Checked, came_to};
use client_core::tool_server_edits::Does;
use gpui_kit::{App, Context, Task, Window};
use protocol::client::tool_server_checks::{ToolServerChecked, ToolServerToCheck};
use protocol::client::tool_servers::{ToolServerAdd, ToolServerEdit};
use protocol::model::ToolServer;

use super::form::{AuthFields, Form};
use super::kept::Kept;
use super::words;

/// How a form stands. A wait in it goes with the panel, so an answer to a panel that was closed, or replaced by
/// another, lands on none.
pub(super) enum FormStands {
    /// To be filled in and sent.
    Open,
    /// The hub is asking the server for its tools.
    AskingServer { _wait: Task<()> },
    /// The hub is adding or saving it.
    AskingHub { _wait: Task<()> },
    /// It wasn't added or saved, for this reason, for the person to read.
    Refused(String),
    /// The server didn't list its tools, for this reason, for the person to read. They choose whether to go on anyway.
    NotListed(String),
}

impl FormStands {
    fn is_waiting(&self) -> bool {
        match self {
            FormStands::AskingServer { .. } | FormStands::AskingHub { .. } => true,
            FormStands::Open | FormStands::Refused(_) | FormStands::NotListed(_) => false,
        }
    }

    /// Whether it's to be saved anyway, since the server didn't list its tools.
    pub(super) fn saves_anyway(&self) -> bool {
        match self {
            FormStands::NotListed(_) => true,
            FormStands::Open
            | FormStands::AskingServer { .. }
            | FormStands::AskingHub { .. }
            | FormStands::Refused(_) => false,
        }
    }

    /// Why it wasn't added or saved when it was last tried.
    pub(super) fn why_not(&self) -> Option<&str> {
        match self {
            FormStands::Refused(why) | FormStands::NotListed(why) => Some(why),
            FormStands::Open | FormStands::AskingServer { .. } | FormStands::AskingHub { .. } => None,
        }
    }

    /// Why it wasn't added or saved goes, since it may be for the way there was.
    pub(super) fn way_chosen(&mut self) {
        match self {
            FormStands::Open | FormStands::AskingServer { .. } | FormStands::Refused(_) | FormStands::NotListed(_) => {
                *self = FormStands::Open;
            }
            FormStands::AskingHub { .. } => {}
        }
    }

    /// What a check was of isn't what's in the form any more, so what came of it, or would, goes.
    pub(super) fn field_changed(&mut self) {
        match self {
            FormStands::AskingServer { .. } | FormStands::NotListed(_) => *self = FormStands::Open,
            FormStands::Open | FormStands::AskingHub { .. } | FormStands::Refused(_) => {}
        }
    }
}

impl Form {
    /// What the hub is asked to check before it's added or saved, if it is: not a server that's signed in to, whose
    /// sign-in says whether it works, nor an edit of the name alone.
    fn checks(&self, cx: &App) -> Option<ToolServerToCheck> {
        match &self.auth {
            AuthFields::SignIn => return None,
            AuthFields::Key { .. } | AuthFields::Neither => {}
        }
        self.asks_for(cx).filter(|of| match of {
            ToolServerToCheck::Edit(edit) => edit.url.is_some() || edit.auth.is_some(),
            ToolServerToCheck::Add(_) | ToolServerToCheck::Unknown => true,
        })
    }
}

impl Kept {
    /// Whether the form has what its button takes, and isn't waiting on the hub, with a hub to ask.
    pub(super) fn can_go(&self, cx: &App) -> bool {
        let Some(form) = self.form() else { return false };
        self.can_ask() && !form.stands.is_waiting() && form.filled(cx)
    }

    /// Adds the server the form says, or saves what's changed of it, once it has been checked, or signs in to it, which
    /// does either once it's approved. It's what Try again does too.
    pub(super) fn go(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_go(cx) {
            return;
        }
        let Some(form) = self.form() else { return };
        match (form.does(cx), form.checks(cx)) {
            (Does::SignIn, _) => {
                let to = form.signs_in_to(cx);
                self.sign_in(to, window, cx);
            }
            (Does::Add | Does::Save, Some(of)) => self.check(of, window, cx),
            (Does::Add | Does::Save, None) => self.go_on(false, window, cx),
        }
    }

    /// Adds or saves it, though the server didn't list its tools.
    pub(super) fn go_anyway(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_go(cx) {
            return;
        }
        // The button that was pressed goes.
        self.focus_the_panel(window, cx);
        self.go_on(true, window, cx);
    }

    /// Has the hub ask the server for its tools, and goes on once it has listed them.
    fn check(&mut self, of: ToolServerToCheck, window: &mut Window, cx: &mut Context<Self>) {
        let Some(hub) = self.asking.clone() else { return };
        // On a thread of its own, since it blocks for as long as the server takes to answer.
        let answer = hub.client.start(move |client| client.tool_server_check(of));
        let wait = cx.spawn_in(window, async move |this, cx| {
            let answer = answer.recv().await;
            let _ = this.update_in(cx, |this, window, cx| this.checked(answer, window, cx));
        });
        self.stand(FormStands::AskingServer { _wait: wait }, cx);
    }

    /// Goes on with the server that was checked, or says why it listed no tools, or why it can't be added or saved.
    /// It's as it was when it was checked, since the wait goes as a field changes.
    fn checked(&mut self, answer: client_core::Result<ToolServerChecked>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.form() else { return };
        match came_to(answer, &form.name.value(cx), &form.url.value(cx)) {
            Ok(Checked::Fine) => self.go_on(false, window, cx),
            Ok(Checked::Not(why)) => self.stand(FormStands::NotListed(why), cx),
            Err(e) => {
                let why = words::couldnt(words::to_do(form.does(cx)), &e);
                self.stand(FormStands::Refused(why), cx);
            }
        }
    }

    /// Has the hub add it, or save it, `anyway` if the server didn't list its tools.
    fn go_on(&mut self, anyway: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some((hub, form)) = self.asking.clone().zip(self.form()) else {
            return self.stand(FormStands::Open, cx);
        };
        let does = form.does(cx);
        let answer = match form.asks_for(cx) {
            Some(ToolServerToCheck::Add(ToolServerAdd { id, name, url, auth })) => {
                hub.client.start(move |client| client.tool_server_add(&id, &name, &url, auth))
            }
            Some(ToolServerToCheck::Edit(edit)) => {
                let edit = ToolServerEdit { saved_anyway: anyway, ..edit };
                hub.client.start(move |client| client.tool_server_edit(edit))
            }
            // A server that's signed in to is added once that's approved, and a form asks for no change this build
            // doesn't know.
            Some(ToolServerToCheck::Unknown) | None => return,
        };
        let wait = cx.spawn_in(window, async move |this, cx| {
            let answer = answer.recv().await;
            let _ = this.update_in(cx, |this, window, cx| this.answered(does, answer, window, cx));
        });
        self.stand(FormStands::AskingHub { _wait: wait }, cx);
    }

    /// Closes the panel once the hub has done what it was asked, and what's typed of a key goes with it, and otherwise
    /// says why it couldn't. The panel is the one that asked, since the wait goes with it.
    fn answered(
        &mut self,
        does: Does,
        answer: client_core::Result<ToolServer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match answer {
            Ok(_) => self.put_away_panel(window, cx),
            Err(e) => self.stand(FormStands::Refused(words::couldnt(words::to_do(does), &e)), cx),
        }
    }

    fn stand(&mut self, stands: FormStands, cx: &mut Context<Self>) {
        if let Some(form) = self.form_mut() {
            form.stands = stands;
        }
        if let Some(panel) = &mut self.panel {
            panel.place.end_changed();
        }
        cx.notify();
    }
}
