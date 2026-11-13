//! What the agent asks in the composer's place, its questions and its requests to use a tool, and answering it.

#![cfg(test)]

use gpui_kit::SharedString;
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
        self.next_open_approval();
        self.wait("what it asks to show", |ui| ui.has(id("question")));
    }

    /// Lets the time pass after which what's just been asked takes keys.
    pub(crate) fn let_what_is_asked_settle(&mut self) {
        self.cx.executor().advance_clock(transcript::QUESTION_SETTLES);
        self.frame();
    }

    /// Allows the tool's use that's asked about in the composer's place, by choosing Allow and sending it.
    pub(crate) fn allow_tool_use(&mut self) {
        self.next_open_approval();
        self.wait("the request to be asked", |ui| ui.has(id("question-answer-0")));
        self.click(id("question-answer-0"));
        self.click(id("question-send"));
    }

    /// What the agent was told its calls came to, such as the answers to its questions, one after another.
    pub(crate) fn told_to_agent(&self) -> String {
        let mut told = String::new();
        for event in self.events() {
            if let EventBody::AgentUser { blocks, .. } = &event.body {
                for block in blocks {
                    if let ContentBlock::ToolResult { text, .. } = block {
                        told.push_str(text);
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
