//! Selecting each kind of text the chat records, besides the agent's replies and the person's prompts, which are
//! selected with the rest, as `super` checks: a plan, and what shows only while it's open, a thought, what shows of a
//! compaction's summary, and what a call shows of its own, in a row of its own, which is what the advisor said, what a
//! subagent was asked and reported, and a command and what it printed, each in the order the chat shows it.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Bounds, Pixels, SharedString, TestAppContext};
use protocol::event::{ContentBlock, EventBody};
use protocol::model::SessionStatus;

use protocol::model::SessionId;

use super::super::{settle_at_the_end, wait_for_the_list_to_settle};
use super::{COMMAND_LINE, LAST, PRINTED, everything, on_the_reply_saying, open};
use crate::support::{Stack, Ui};

pub(super) const ADVISOR_LINE: &str = "Ask the advisor";
/// What the advisor says, in words nothing else in the chat says as its own text.
const ADVICE: &str = "Test the points before the timetable changes.";
/// The fake agent's subagent's call, and what it's asked, which the prompt that starts it doesn't say with the stop.
pub(super) const SUBAGENT_LINE: &str = "Look around";
pub(super) const ASKED: &str = "Look around.";
/// What it reports, once it's done.
pub(super) const REPORT: &str = "There's a README here.";
/// What a compaction is told to keep, which its summary says too.
const KEEP: &str = "the timetables and the sidings";
const PLAN: &str = "Timetables first, then the sidings, then each junction.";

/// How a text that shows only while it's open is opened, and closed again.
enum Opens {
    /// By a click on its call's line, labelled this.
    Call(&'static str),
    /// By the transcript's setting for every thought.
    Thinking,
    /// By a click on the compaction's line.
    Summary,
}

/// A kind of text that shows only while it's open, and what a copy of everything has of it either way.
struct Kind {
    name: &'static str,
    /// What has the agent write it, and then the reply after it, which [`after`] says.
    prompts: Vec<String>,
    opens: Opens,
    copied: fn(&Ui<'_>) -> Copied,
}

/// What a copy has while a text is open, with what's around it, and how many times a copy has each of these once it's
/// closed again, for what else in the chat says them.
type Copied = (String, Vec<(String, usize)>);

/// One chat, which has a call of each kind that shows text of its own, a thought, a compaction and then a plan, each
/// the newest in the chat as it's copied. Each but the plan is copied with the rest while it's open, between what comes
/// before and after it, and isn't once it's closed, though the rest is. A plan is selected with the rest, once it's
/// approved.
#[gpui_kit::test]
fn each_kind_of_text_is_selected_with_the_rest_and_one_that_opens_only_while_it_s_open(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let session = stack.chat_in_a_new_workspace();
    stack.run(&session, "say:Hello");
    let mut ui = open(cx, &stack);

    for kind in kinds() {
        for prompt in &kind.prompts {
            run_in_the_open_chat(&mut ui, &stack, &session, prompt);
        }
        let (name, reply) = (kind.name, after(kind.name));
        let (between, closed_has) = (kind.copied)(&ui);
        open_or_close(&mut ui, &kind.opens, true);
        let opened = everything_at_the_end(&mut ui, &reply);
        open_or_close(&mut ui, &kind.opens, false);
        let closed = everything_at_the_end(&mut ui, &reply);

        assert!(opened.contains(&between), "opened, {name} is copied, between the rest: {opened:?}");
        for (said, times) in closed_has {
            assert_eq!(closed.matches(&said).count(), times, "closed, {name} isn't copied: {closed:?}");
        }
        assert!(opened.ends_with(&reply) && closed.ends_with(&reply), "with {name} either way, the rest is copied");
    }
    // Last, since once it's approved, the agent asks before it runs a command.
    stack.client().session_prompt(&session, &format!("plan:{PLAN}"), None).unwrap();
    ui.approve_plan();
    ui.wait("the plan's turn to end", |ui| ui.session_status() == Some(SessionStatus::Idle));
    run_in_the_open_chat(&mut ui, &stack, &session, &format!("say:{LAST}"));
    let all = everything_at_the_end(&mut ui, LAST);

    // The prompt has the plan's words too, after what asks for it.
    assert!(all.contains(&format!("\n\n{PLAN}\n\n")), "the plan, between what's before and after it: {all:?}");
    assert!(all.ends_with(LAST), "to the last reply");
}

/// Each kind of text that shows only while it's open, in the order the chat has them.
fn kinds() -> [Kind; 5] {
    [
        Kind {
            name: "advice",
            prompts: vec![format!("advise:{ADVICE}; say:{}", after("advice"))],
            opens: Opens::Call(ADVISOR_LINE),
            // The prompt says it too, before what follows it there.
            copied: |_| (format!("\n\n{ADVICE}\n\n{}", after("advice")), vec![(ADVICE.to_owned(), 1)]),
        },
        Kind {
            name: "a subagent's ask and report",
            prompts: vec![format!("agent-calls:1:{SUBAGENT_LINE}; say:{}", after("a subagent's ask and report"))],
            opens: Opens::Call(SUBAGENT_LINE),
            copied: |_| {
                let reply = after("a subagent's ask and report");
                let after_its_prompt = format!("{SUBAGENT_LINE}; say:{reply}\n\n{ASKED}\n\n{REPORT}\n\n{reply}");
                (after_its_prompt, vec![(ASKED.to_owned(), 0), (REPORT.to_owned(), 0)])
            },
        },
        Kind {
            name: "a command and its output",
            prompts: vec![format!("tool:echo {PRINTED}; say:{}", after("a command and its output"))],
            opens: Opens::Call(COMMAND_LINE),
            copied: |_| {
                let between = format!("\n\necho {PRINTED}\n\n{PRINTED}\n\n{}", after("a command and its output"));
                (between, vec![(PRINTED.to_owned(), 1)])
            },
        },
        Kind {
            name: "a thought",
            prompts: vec![format!("ponder:1:7; say:{}", after("a thought"))],
            opens: Opens::Thinking,
            copied: |ui| {
                let start: String = thought(ui).chars().take(40).collect();
                (start.clone(), vec![(start, 0)])
            },
        },
        Kind {
            name: "a summary",
            prompts: vec![format!("/compact {KEEP}"), format!("say:{}", after("a summary"))],
            opens: Opens::Summary,
            // The prompt that asked for the compaction says it too.
            copied: |_| (format!("What to keep: {KEEP}"), vec![(KEEP.to_owned(), 1)]),
        },
    ]
}

/// Opens the newest text `opens` says, or closes it.
fn open_or_close(ui: &mut Ui<'_>, opens: &Opens, open: bool) {
    match opens {
        Opens::Call(line) => {
            let lines = ui.labelled(line);
            let newest = lines.last().unwrap_or_else(|| panic!("no line labelled {line}"));
            ui.click(newest.path().last().unwrap().clone());
        }
        Opens::Thinking => {
            let transcript = ui.transcript().unwrap();
            ui.cx.update_entity(&transcript, |t, cx| t.set_thinking_open(open, cx));
        }
        Opens::Summary => {
            let compacted =
                ui.events().into_iter().find(|event| matches!(event.body, EventBody::AgentCompacted { .. }));
            ui.click(SharedString::from(format!("compaction/event/{}/0", compacted.unwrap().seq)));
        }
    }
}

/// The reply after the text of the kind `name`, which no other reply says.
fn after(name: &str) -> String {
    format!("After {name}: {LAST}")
}

/// Everything Cmd-A selects, with the chat kept to its end and given the focus by a click on the reply that says
/// `reply`, the newest.
fn everything_at_the_end(ui: &mut Ui<'_>, reply: &str) -> String {
    ui.keep_to_the_end();
    let window = settled_at_its_last_row(ui);
    let on_it = on_the_reply_saying(ui, window, reply);
    everything(ui, on_it)
}

/// Has the agent run `prompt` in `session`, which the window has open, and waits for the window to show all of it.
/// [`Stack::run`] waits for the hub to have the whole turn, but the window takes the turn's events in over its own
/// connection, and only as the app runs, so it can still be a turn behind. So this runs the app until the window's log
/// holds, in its row, the newest of the chat's own events the hub has, which is the last of the turn: the log takes
/// the chat's events in the order the hub sends them, so it has the rest of the turn by then too, and every row the
/// turn adds. A subagent's events aren't the chat's own, and the turn's own go on after the subagent's end. Then the
/// list settles there. Where the transcript is.
pub(super) fn run_in_the_open_chat(
    ui: &mut Ui<'_>,
    stack: &Stack,
    session: &SessionId,
    prompt: &str,
) -> Bounds<Pixels> {
    stack.run(session, prompt);
    settle_at_the_end(ui, stack, session);
    settled_at_its_last_row(ui)
}

/// Where the transcript is, once its list has settled at the chat's last row, for a chat whose rows the window has
/// all taken in. A subagent's events have places in the log the chat's rows don't, so this goes by the rows.
pub(super) fn settled_at_its_last_row(ui: &mut Ui<'_>) -> Bounds<Pixels> {
    let rows = ui.cx.read_entity(&ui.transcript().unwrap(), |t, _| t.row_count());
    wait_for_the_list_to_settle(ui, rows - 1);
    ui.cx.update_window(ui.handle, |_, window, _| window.find("transcript").bounds()).unwrap()
}

/// What the agent thought in the chat `ui` shows.
pub(super) fn thought(ui: &Ui<'_>) -> String {
    let thought = ui.events().into_iter().find_map(|event| match event.body {
        EventBody::Assistant { blocks, .. } => blocks.iter().find_map(|block| match block {
            ContentBlock::Thinking { text, .. } => Some(text.clone()),
            _ => None,
        }),
        _ => None,
    });
    thought.unwrap()
}
