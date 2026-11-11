//! Scrolling through rows whose height comes late, such as a long text the kit reads on another thread, which moves
//! nothing in the window when it does.

use gpui_kit::{AppContext as _, ElementId, Entity, Pixels, ScrollDelta, TestAppContext, point, px};
use protocol::model::{SessionId, SessionStatus, seq_index};
use transcript::{Place, Transcript};

use super::{A_FRAME, wait_for_the_list_to_settle};
use crate::support::{Stack, Ui};

/// How far each turn of the wheel goes, about what a trackpad sends at once.
const STEP: Pixels = px(40.);

/// A long thought, shown open as "Always expand thinking" has it, just above a long reply at the end of a session.
/// The kit reads a text over 4 KB on another thread and gives it no height until that's done. A thought was only
/// read once it scrolled in at the top of the window, as a line with nothing under it, and then it grew there by all
/// of its length, which took the window up to the thought's start, past the reply being scrolled up through.
#[gpui_kit::test]
fn scrolling_up_into_a_long_open_thought_moves_as_far_as_the_wheel_turns(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, "say:a short reply first");
    stack.run(&session, "ponder:40:1; prose:40:2");
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let transcript = ui.transcript().unwrap();
    ui.cx.update_entity(&transcript, |t, cx| t.set_thinking_open(true, cx));
    wait_for_the_list_to_settle(&mut ui, last_row(&stack, &session));

    let steps = steps_up_to_the_start(&mut ui, &transcript);

    each_went_as_far_as_the_wheel(&steps);
}

/// A long plan, approved, just above a long reply. A plan is drawn in an approval's card, a row unlike a reply's, so
/// this shows that whatever long text a row draws is kept.
#[gpui_kit::test]
fn scrolling_up_into_a_long_plan_moves_as_far_as_the_wheel_turns(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, "say:a short reply first");
    let plan = vec!["the signal box keeps a ledger of every train that passes. ".repeat(12); 12].join("\n\n");
    assert!(plan.len() > 4 * 1024, "a plan the kit reads on another thread");
    stack.client().session_prompt(&session, &format!("plan:{plan}"), None).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the workspace", |ui| ui.transcript().is_some());
    let approval = ui.next_open_approval();
    ui.click(ElementId::NamedInteger("approve-plan".into(), approval));
    ui.wait("the plan's turn to end", |ui| ui.session_status() == Some(SessionStatus::Idle));
    stack.run(&session, "prose:40:2");
    let transcript = ui.transcript().unwrap();
    wait_for_the_list_to_settle(&mut ui, last_row(&stack, &session));

    let steps = steps_up_to_the_start(&mut ui, &transcript);

    each_went_as_far_as_the_wheel(&steps);
}

/// The row of the session's latest event.
fn last_row(stack: &Stack, session: &SessionId) -> usize {
    seq_index(stack.client().session_get(session).unwrap().last_seq) - 1
}

/// Turns the wheel up a [`STEP`] at a time, as a trackpad does, until the list is at the start, letting the list come
/// to rest after each. Returns how far it was from the top of the window to the end of the session after each, by
/// the rows' heights as measured.
fn steps_up_to_the_start(ui: &mut Ui<'_>, transcript: &Entity<Transcript>) -> Vec<Pixels> {
    let to_the_end = |ui: &Ui<'_>| {
        ui.cx.read_entity(transcript, |t, _| {
            let (list, top) = (t.list(), t.list().top());
            let below: Pixels = (top.row..list.row_count()).map(|row| list.row_height(row).unwrap_or_default()).sum();
            below - top.offset
        })
    };
    let top = |ui: &Ui<'_>| ui.cx.read_entity(transcript, |t, _| t.list().top());
    let mut steps = vec![to_the_end(ui)];
    while top(ui) != Place::default() {
        ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), STEP)));
        ui.settle(A_FRAME);
        steps.push(to_the_end(ui));
        assert!(steps.len() < 2000, "the top is never reached");
    }
    steps
}

/// Every step went as far as the wheel turned, but the last, which stops at the start however far that is.
fn each_went_as_far_as_the_wheel(steps: &[Pixels]) {
    let moved: Vec<Pixels> = steps.windows(2).map(|pair| pair[1] - pair[0]).collect();
    let (last, before) = moved.split_last().unwrap();
    assert!(*last <= STEP + px(0.5), "the last step went {last:?}");
    let off = before.iter().enumerate().find(|(_, moved)| (**moved - STEP).abs() > px(0.5));
    assert!(off.is_none(), "step {off:?} of {} went another way than the wheel's {STEP:?}", before.len());
}
