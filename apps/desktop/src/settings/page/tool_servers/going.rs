//! Adding or saving what's in a form, with one request. The hub asks a server with a key or with None for its tools
//! first, and one that doesn't list them is added or saved only if the person has it done anyway. What a save comes to
//! is client-core's `tool_server_checks` to say, and the save is let go of as it goes with the form's state.
//!
//! Nothing in the panel changes what was sent while the hub saves it: its fields and its dropdown are off, as its button
//! is, and Cancel, or the panel going, lets go of the save.

use client_core::ToolServerSaving;
use client_core::tool_server_checks::{Saved, When};
use client_core::tool_server_edits::Does;
use gpui_kit::{App, Context, Task, Window};

use super::kept::Kept;
use super::words;

/// How a form stands. A wait in it goes with the panel, so an answer to a panel that was closed, or replaced by
/// another, lands on none.
pub(super) enum FormStands {
    /// To be filled in and sent.
    Open,
    /// The hub is adding or saving it, as `when` says, once it has asked the server for its tools where it asks. The
    /// save is let go of if it goes before the hub has answered, as it does with the panel.
    Saving { when: When, _saving: ToolServerSaving, _wait: Task<()> },
    /// It wasn't added or saved, for this reason, for the person to read.
    Refused(String),
    /// The server didn't list its tools, for this reason, for the person to read. They choose whether to go on anyway.
    NotListed(String),
}

impl FormStands {
    /// Whether the hub is adding or saving it, while which nothing in the panel can be changed.
    pub(super) fn is_waiting(&self) -> bool {
        match self {
            FormStands::Saving { .. } => true,
            FormStands::Open | FormStands::Refused(_) | FormStands::NotListed(_) => false,
        }
    }

    /// Why it wasn't added or saved when it was last tried.
    pub(super) fn why_not(&self) -> Option<&str> {
        match self {
            FormStands::Refused(why) | FormStands::NotListed(why) => Some(why),
            FormStands::Open | FormStands::Saving { .. } => None,
        }
    }

    /// Why it wasn't added or saved goes, since it may be for the way there was.
    pub(super) fn way_chosen(&mut self) {
        match self {
            FormStands::Refused(_) | FormStands::NotListed(_) => *self = FormStands::Open,
            FormStands::Open | FormStands::Saving { .. } => {}
        }
    }

    /// What was sent isn't what's in the form any more, so a server that didn't list its tools is to be asked again.
    pub(super) fn field_changed(&mut self) {
        match self {
            FormStands::NotListed(_) => *self = FormStands::Open,
            FormStands::Open | FormStands::Saving { .. } | FormStands::Refused(_) => {}
        }
    }

    /// Whether the hub is asking the server for its tools, as far as the app can tell, for a form that `is_asked`. The
    /// hub doesn't say when it has done, so it's for as long as the save is under way.
    pub(super) fn asks(&self, is_asked: impl FnOnce() -> bool) -> bool {
        match self {
            FormStands::Saving { when: When::Listed, .. } => is_asked(),
            FormStands::Saving { when: When::Anyway, .. }
            | FormStands::Open
            | FormStands::Refused(_)
            | FormStands::NotListed(_) => false,
        }
    }
}

impl Kept {
    /// Whether the form has what its button takes, and isn't waiting on the hub, with a hub to ask.
    pub(super) fn can_go(&self, cx: &App) -> bool {
        let Some(form) = self.form() else { return false };
        self.can_ask() && !form.stands.is_waiting() && form.filled(cx)
    }

    /// Adds the server the form says, or saves what's changed of it, once the hub has asked it for its tools where it
    /// asks, or signs in to it, which does either once it's approved. It's what Try again does too.
    pub(super) fn go(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_go(cx) {
            return;
        }
        let Some(form) = self.form() else { return };
        match form.does(cx) {
            Does::SignIn => {
                let to = form.signs_in_to(cx);
                self.sign_in(to, window, cx);
            }
            Does::Add | Does::Save => self.save(When::Listed, window, cx),
        }
    }

    /// Adds or saves it, though the server didn't list its tools.
    pub(super) fn go_anyway(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_go(cx) {
            return;
        }
        // The button that was pressed goes.
        self.focus_the_panel(window, cx);
        self.save(When::Anyway, window, cx);
    }

    /// Has the hub add it, or save it, as `when` says.
    fn save(&mut self, when: When, window: &mut Window, cx: &mut Context<Self>) {
        let Some((hub, form)) = self.asking.clone().zip(self.form()) else {
            return self.stand(FormStands::Open, cx);
        };
        // A server that's signed in to is added once that's approved.
        let Some(of) = form.to_save(cx) else { return };
        let (name, url) = (form.name.value(cx), form.url.value(cx));
        let (saving, answer) = ToolServerSaving::start(&hub.client, of, when, name, url);
        let wait = cx.spawn_in(window, async move |this, cx| {
            let answer = answer.recv().await;
            let _ = this.update_in(cx, |this, window, cx| this.answered(answer, window, cx));
        });
        self.stand(FormStands::Saving { when, _saving: saving, _wait: wait }, cx);
    }

    /// Closes the panel once the hub has added or saved it, and what's typed of a key goes with it, and otherwise says
    /// why it didn't. The panel is the one that asked, since the wait goes with it.
    fn answered(&mut self, answer: client_core::Result<Saved>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.form() else { return };
        let does = form.does(cx);
        match answer {
            Ok(Saved::Saved(_)) => self.put_away_panel(window, cx),
            Ok(Saved::NotListed(why)) => self.stand(FormStands::NotListed(why), cx),
            Ok(Saved::LetGo) => self.stand(FormStands::Open, cx),
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
