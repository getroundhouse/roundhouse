//! What's running in the background, by whose work it's part of: the agent's has its lines at the chat's end, and a
//! subagent's at the end of its tab, until the subagent ends and what it left going is the agent's. And what each line
//! says, on one line whatever its task is called. The app's alone, so a fake hub will do.

#![cfg(test)]

use client_core::fake_hub::events::{
    first_turn, launched, part_of, replied, subagent, subagent_started, task, text, undescribed, user_prompt,
};
use gpui_kit::{SharedString, TestAppContext};
use protocol::capability::calls::call::Runs;
use protocol::event::{AgentTask, EventBody, TaskKind};
use protocol::model::{Record, Session, SessionStatus};

use crate::support::{one_chat, open_on_the_chat};

const AGENT: &str = "toolu-1";
const SCRIPT: &str = "python3 - <<'EOF'\nprint('done')\nEOF";

/// A turn that left a subagent at work, with `tasks` listed beside it.
fn left_going(tasks: Vec<AgentTask>) -> Vec<EventBody> {
    let mut listed = vec![task(TaskKind::Subagent, AGENT, "Move the points")];
    listed.extend(tasks);
    first_turn(
        user_prompt("Move the points"),
        [
            subagent("msg-1", AGENT, "Explore", "Move the points", "Move them.", Runs::Background),
            subagent_started(AGENT, "Explore", "Move the points", "Move them."),
            launched(AGENT),
            EventBody::AgentTasks { tasks: listed, stopped: Vec::new() },
            text("msg-2", "It's going."),
            replied("It's going.", 5, 0.01),
        ],
    )
}

fn line(call: &str) -> SharedString {
    SharedString::from(format!("background-task-{call}"))
}

fn words(call: &str) -> SharedString {
    SharedString::from(format!("background-text/task-{call}"))
}

#[gpui_kit::test]
fn what_a_subagent_has_in_the_background_shows_in_its_tab_until_it_ends_and_then_in_the_chat(cx: &mut TestAppContext) {
    let theirs = part_of(AGENT, undescribed("theirs", SCRIPT));
    let own = task(TaskKind::Command, "own", "Run the tests");
    let (mut world, chat) = one_chat(SessionStatus::Background, left_going(vec![theirs.clone(), own]));
    let before = world.session(&chat).unwrap().clone();
    let later = world.after();
    let (_fake, mut ui) = open_on_the_chat(cx, world, &chat);

    ui.wait("the agent's own lines", |ui| ui.has(line(AGENT)) && ui.has(line("own")));
    assert!(!ui.has(line("theirs")), "a subagent's isn't in the chat");
    ui.click(SharedString::from(format!("background-open/task-{AGENT}")));
    ui.wait("the subagent's line, in its tab", |ui| ui.shows_label(&words("theirs"), "Run python3 - <<'EOF'"));
    assert!(!ui.has(line("own")) && !ui.has(line(AGENT)), "and the agent's aren't in the tab");

    // The chat's record alone, as it can come before the subagent's own that says it ended.
    let left = AgentTask { part_of: None, ..theirs };
    let background_tasks = before.background_tasks.iter().filter(|task| task.part_of.is_none()).cloned();
    let background_tasks = background_tasks.chain([left]).collect();
    ui.view().feed().unwrap().take_record(Record::Session(Session { background_tasks, rev: later, ..before }));

    ui.wait("its line to go from the tab", |ui| !ui.has(line("theirs")));
    ui.show_chat(&chat);
    ui.wait("its line, in the chat", |ui| ui.has(line("theirs")));
}

/// A command the agent said nothing of, and one recorded before tasks said their commands, which gives the whole of it
/// as what it's for.
#[gpui_kit::test]
fn a_background_tasks_line_is_one_line_however_many_its_command_is_on(cx: &mut TestAppContext) {
    let tasks = vec![
        task(TaskKind::Command, "said", "Run the tests"),
        undescribed("unsaid", SCRIPT),
        task(TaskKind::Command, "before", SCRIPT),
    ];
    let (world, chat) = one_chat(SessionStatus::Background, left_going(tasks));
    let (_fake, mut ui) = open_on_the_chat(cx, world, &chat);
    ui.wait("their lines", |ui| ["said", "unsaid", "before"].into_iter().all(|call| ui.has(line(call))));

    assert!(ui.shows_label(&words("unsaid"), "Run python3 - <<'EOF'"));
    assert!(ui.shows_label(&words("before"), "python3 - <<'EOF' print('done') EOF"));
    let each = [AGENT, "said", "unsaid", "before"].map(|call| ui.bounds(words(call)));
    for pair in each.windows(2) {
        assert!(pair[0].bottom() <= pair[1].top(), "a line over the next: {pair:?}");
    }
}
