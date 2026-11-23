//! What the agent asks, and answering it: its questions and its requests to use a tool, in the composer's place, and
//! its plans, on their cards in the transcript.

#![cfg(test)]

use gpui_kit::{ElementId, SharedString};
use protocol::event::{ContentBlock, EventBody};
use protocol::model::SessionStatus;

use super::Ui;

/// The ID of an element named `name`, as the panel names its own.
pub(crate) fn id(name: &str) -> SharedString {
    SharedString::from(name.to_string())
}

impl Ui<'_> {
    /// Sends `prompt`, which has the fake agent ask something, and waits for it to be asked in the composer's place.
    pub(crate) fn ask(&mut self, prompt: &str) {
        self.input(prompt);
        self.press("enter");
        self.wait("the agent to ask", |ui| ui.session_status() == Some(SessionStatus::WaitingApproval));
        let asked = self.next_open_approval();
        self.wait("what it asks to show", |ui| ui.has(id("question")));
        // What's asked comes with the session's record, and its request can be fetched on its own, so it can show
        // before the chat behind it has what came before it, which comes through the chat's own subscription.
        self.wait("the chat behind it to have what it asked", |ui| ui.events().iter().any(|event| event.seq == asked));
    }

    /// Lets the time pass after which what's just been asked takes keys, and the chat behind it settle where it is.
    pub(crate) fn let_what_is_asked_settle(&mut self) {
        self.cx.executor().advance_clock(transcript::QUESTION_SETTLES);
        self.frame();
        self.wait_for_the_chat_to_settle();
    }

    /// Allows the tool's use that's asked about in the composer's place, by choosing Allow and sending it.
    pub(crate) fn allow_tool_use(&mut self) {
        self.next_open_approval();
        self.wait("the request to be asked", |ui| ui.has(id("question-answer-0")));
        self.click(id("question-answer-0"));
        self.click(id("question-send"));
    }

    /// Approves the newest plan, with "Approve plan" on its card.
    pub(crate) fn approve_plan(&mut self) {
        self.answer_plan("approve-plan");
    }

    /// Sends the newest plan back, with "Keep planning" on its card.
    pub(crate) fn keep_planning(&mut self) {
        self.answer_plan("keep-planning");
    }

    /// Clicks `button` on the card of the newest open approval, a plan, once the card has drawn it. When the chat's
    /// record says a request waits before the transcript's log has taken it in, the transcript fetches it on its own,
    /// and has it open from then. A plan's card is drawn in the request's row, which comes only once the log takes it
    /// in, so the card can come a moment after what [`Ui::next_open_approval`] waits for.
    fn answer_plan(&mut self, button: &'static str) {
        let id = ElementId::NamedInteger(button.into(), self.next_open_approval().get());
        self.wait("the plan's buttons", |ui| ui.visible(id.clone()));
        self.click(id);
    }

    /// What the agent was told its calls came to, such as the answers to its questions, one after another.
    pub(crate) fn told_to_agent(&self) -> String {
        let mut told = String::new();
        for event in self.events() {
            if let EventBody::AgentUser { blocks, .. } = &event.body {
                for block in blocks {
                    if let ContentBlock::CallResult { output, .. } = block {
                        told.push_str(&client_core::calls::typed::output(output));
                    }
                }
            }
        }
        told
    }

    pub(crate) fn wait_for_the_turn(&mut self) {
        self.wait("the turn to end", |ui| ui.session_status() == Some(SessionStatus::Idle));
    }
}
