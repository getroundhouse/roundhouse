//! Chats for the outline to draw, held by a fake hub, since what it draws of what a hub holds needs no hub, worker or
//! agent. Each is made of turns as the hub records them from the fake agent's: the prompt, the agent taking it, what
//! it said, each as a message of its own, and the turn's end.

#![cfg(test)]

use client_core::fake_hub::World;
use client_core::fake_hub::events::{finished_turn, next_seq, start_turn, text, turn_ending_with, user_prompt};
use gpui_kit::TestAppContext;
use protocol::event::{CompactedBy, CompactionOutcome, EventBody, SessionEvent};
use protocol::model::{Seq, SessionId, SessionStatus};

use crate::support::{FakeHub, Ui};

/// A chat's events, built a turn at a time.
#[derive(Default)]
pub(super) struct Turns(Vec<EventBody>);

impl Turns {
    /// A turn with the agent saying each of `said`, as the fake agent's `say:` does.
    pub(super) fn turn(mut self, said: &[&str]) -> Self {
        let prompt = next_seq(&self.0);
        let number = self.0.iter().filter(|event| matches!(event, EventBody::UserPrompt { .. })).count() + 1;
        let messages = said.iter().enumerate().map(|(i, said)| text(&format!("msg-{}-{i}", prompt.get()), said));
        turn_ending_with(&mut self.0, user_prompt(&format!("Turn {number}.")), messages, finished_turn(12, 0.04));
        self
    }

    /// A turn of the fake agent's `prose:PARAGRAPHS:SEED`, which writes a long reply.
    pub(super) fn prose(self, paragraphs: usize, seed: usize) -> Self {
        self.turn(&[&prose(paragraphs, seed)])
    }

    /// A turn of `burst:3000; say:SAID`: a reply many lines long, and then `said`.
    pub(super) fn burst(self, said: &str) -> Self {
        let burst = Vec::from_fn(3000, |i| i.to_string()).join(" ");
        self.turn(&[&burst, said])
    }

    /// A turn of `say:message 0; say:message 1; …`, saying `count` messages.
    pub(super) fn messages(self, count: usize) -> Self {
        let said = Vec::from_fn(count, |i| format!("message {i}"));
        self.turn(&Vec::from_iter(said.iter().map(String::as_str)))
    }

    /// A compaction someone asked for, as the hub and the agent's adapter record it: a turn of its own, with no prompt.
    pub(super) fn compacted(mut self) -> Self {
        let outcome = CompactionOutcome::Compacted {
            by: CompactedBy::Asked,
            before_tokens: Some(226_869),
            after_tokens: Some(9_009),
            took_ms: Some(51_558),
            summary: Some("The summary".to_string()),
        };
        let mut ended = finished_turn(52, 0.0);
        if let EventBody::TurnEnded { only_compacted, .. } = &mut ended {
            *only_compacted = true;
        }
        let asked = start_turn(&mut self.0, EventBody::CompactionAsked { keeping: None });
        self.0.extend([EventBody::AgentCompacted { asked: Some(asked), keeping: None, outcome }, ended]);
        self
    }
}

/// `paragraphs` paragraphs of made-up prose, the same for the same `seed`, of lengths as a long reply's vary, from a
/// line to several.
fn prose(paragraphs: usize, seed: usize) -> String {
    const WORDS: [&str; 8] = ["signal", "box", "keeps", "the", "ledger", "of", "every", "train"];
    const LENGTHS: [usize; 4] = [30, 120, 60, 90];
    let paragraph = |p: usize| {
        let words = Vec::from_fn(LENGTHS[(seed + p) % LENGTHS.len()], |w| WORDS[(seed + p + 3 * w) % WORDS.len()]);
        format!("{}.", words.join(" "))
    };
    Vec::from_fn(paragraphs, paragraph).join("\n\n")
}

/// A world of one workspace, with a chat holding `turns`, and that chat.
pub(super) fn chat_world(turns: Turns) -> (World, SessionId) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    let chat = world.chat(&workspace, SessionStatus::Idle, turns.0);
    (world, chat)
}

/// The window on a fake hub holding `world`, showing `chat`, and every event in the chat, as the hub holds them. The
/// hub goes with the test, so it's returned to be kept.
pub(super) fn open_on<'a>(
    cx: &'a mut TestAppContext,
    world: World,
    chat: &SessionId,
) -> (FakeHub, Ui<'a>, Vec<SessionEvent>) {
    let log = world.events_from(chat, Seq::default()).cloned().collect();
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    ui.wait("the chat", |ui| ui.transcript().is_some());
    ui.activate_chat(chat);
    (fake, ui, log)
}

/// The window on a chat of `turns` alone.
pub(super) fn open_on_a_chat(cx: &mut TestAppContext, turns: Turns) -> (FakeHub, Ui<'_>, Vec<SessionEvent>) {
    let (world, chat) = chat_world(turns);
    open_on(cx, world, &chat)
}
