//! Chats made up for a fake hub to hold, for a test of what's selected of what the window draws, which needs no hub,
//! worker or agent.

use client_core::fake_hub::World;
use client_core::fake_hub::events::{
    self, advised, finished_turn, launched, printed, subagent_ended, subagent_started, subagents_listed, taken, text,
    typed_call, user_prompt,
};
use gpui_kit::{Bounds, Pixels, TestAppContext};
use protocol::capability::calls::Call;
use protocol::capability::calls::call::Runs;
use protocol::event::{CompactedBy, CompactionOutcome, EventBody, SessionEvent};
use protocol::model::{Seq, SessionId, SessionStatus};

use super::super::wait_for_the_list_to_settle;
use super::bounds;
use crate::support::{FakeHub, Ui};

/// Something the agent did in a turn.
pub(super) enum Said {
    /// Wrote this, as a reply of its own.
    Reply(String),
    /// Ran this command, which printed what follows it.
    Command(&'static str, &'static str),
}

/// A turn: what the person asked, and what the agent did about it.
pub(super) type Turn = (String, Vec<Said>);

/// The chats of one workspace, open on the first, kept to its end, once its list has settled there. The hub goes with
/// the test, so it's returned to be kept, and so is each chat, and where the transcript is.
pub(super) fn open_on_chats(
    cx: &mut TestAppContext,
    chats: Vec<Vec<Turn>>,
) -> (FakeHub, Ui<'_>, Vec<SessionId>, Bounds<Pixels>) {
    let (fake, ui, chats, window) = open_on_chats_of(cx, Vec::from_iter(chats.into_iter().map(events)));
    (fake, ui, Vec::from_iter(chats.into_iter().map(|(id, _)| id)), window)
}

/// The same, for chats of the events given, as the hub records them. Gives each chat with the newest of its events, to
/// show it by with [`show`].
pub(super) fn open_on_chats_of(
    cx: &mut TestAppContext,
    chats: Vec<Vec<EventBody>>,
) -> (FakeHub, Ui<'_>, Vec<(SessionId, SessionEvent)>, Bounds<Pixels>) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    let ids = Vec::from_iter(chats.into_iter().map(|events| world.chat(&workspace, SessionStatus::Idle, events)));
    let newest = |id: &SessionId| world.events_from(id, Seq::default()).last().cloned().unwrap();
    let chats = Vec::from_iter(ids.iter().map(|id| (id.clone(), newest(id))));
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let window = show(&mut ui, &chats[0]);
    (fake, ui, chats, window)
}

/// Shows the chat, kept to its end, once its list has settled there with the newest of its events, which is given with
/// it, loaded in its row. Gives where the transcript is.
pub(super) fn show(ui: &mut Ui<'_>, (chat, newest): &(SessionId, SessionEvent)) -> Bounds<Pixels> {
    ui.activate_chat(chat);
    let last = ui.row_of(newest);
    wait_for_the_list_to_settle(ui, last);
    bounds(ui, "transcript".into())
}

/// The same, for one chat of `turns`.
pub(super) fn open_on_a_chat(cx: &mut TestAppContext, turns: Vec<Turn>) -> (FakeHub, Ui<'_>, Bounds<Pixels>) {
    let (fake, ui, _, window) = open_on_chats(cx, vec![turns]);
    (fake, ui, window)
}

/// The events of a chat of `turns`, as the hub records them: each prompt, the agent taking it, what it did about it,
/// each reply and each call its own message, and the end of the turn.
fn events(turns: Vec<Turn>) -> Vec<EventBody> {
    let mut events = Vec::new();
    for (asked, said) in turns {
        let prompt = Seq::new(u64::try_from(events.len() + 1).unwrap());
        events.extend([user_prompt(&asked), taken(prompt)]);
        for said in said {
            let message = format!("msg-{}", events.len());
            match said {
                Said::Reply(reply) => events.push(text(&message, &reply)),
                Said::Command(line, output) => {
                    let call = format!("toolu-{}", events.len());
                    events.push(events::command(&message, &call, line, None));
                    events.push(printed(&call, output));
                }
            }
        }
        events.push(finished_turn(12, 0.04));
    }
    events
}

/// A reply of `paragraphs` paragraphs, each of a few lines, which `seed` tells apart from the others.
pub(super) fn prose(paragraphs: usize, seed: usize) -> Said {
    let paragraph = |n| {
        format!(
            "Ledger {seed}, entry {n}: the signal box keeps a ledger of every train that passes, and the lever frame \
             holds each route clear until the train has gone by and the block ahead of it is clear again."
        )
    };
    Said::Reply(Vec::from_fn(paragraphs, paragraph).join("\n\n"))
}

/// A reply that says `text`.
pub(super) fn reply(text: &str) -> Said {
    Said::Reply(text.to_owned())
}

/// Asking the advisor, as the `n`th call, and what it `said`, as the hub records them from Claude.
pub(super) fn advice(n: usize, said: &str) -> Vec<EventBody> {
    let (message, call) = (format!("msg-advisor-{n}"), format!("srvtoolu-{n}"));
    vec![typed_call(&message, &call, Call::Advisor), advised(&message, &call, said)]
}

/// A subagent started in the background as the `n`th call, for what its line says, `description`, asked `asked`, and
/// its `report` once it's done, as the hub records them from Claude.
pub(super) fn subagent(n: usize, description: &str, asked: &str, report: &str) -> Vec<EventBody> {
    let call = format!("toolu-agent-{n}");
    vec![
        events::subagent(&format!("msg-agent-{n}"), &call, "Explore", description, asked, Runs::Background),
        subagent_started(&call, "Explore", description, asked),
        subagents_listed(&[(&call, description)]),
        launched(&call),
        subagent_ended(&call, report, 1_500),
        subagents_listed(&[]),
    ]
}

/// A command run as the `n`th call, and what it printed, `output`, as the hub records them from Claude.
pub(super) fn command(n: usize, command: &str, output: &str) -> Vec<EventBody> {
    let call = format!("toolu-command-{n}");
    vec![events::command(&format!("msg-command-{n}"), &call, command, None), printed(&call, output)]
}

/// A compaction that was asked for, and its `summary`.
pub(super) fn compacted(summary: &str) -> EventBody {
    let outcome = CompactionOutcome::Compacted {
        by: CompactedBy::Asked,
        before_tokens: Some(16_000),
        after_tokens: Some(4_000),
        took_ms: Some(1_500),
        summary: Some(summary.to_owned()),
    };
    EventBody::AgentCompacted { asked: None, keeping: None, outcome }
}
