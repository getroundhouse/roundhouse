//! Selecting each kind of text the chat records, besides the agent's replies and the person's prompts, which are
//! selected with the rest, as `super` checks: a plan, and what shows only while it's open, a thought, what shows of a
//! compaction's summary, and what a call shows of its own, in a row of its own, which is what the advisor said, what a
//! subagent was asked and reported, and a command and what it printed, each in a chat of its own. Only what the hub
//! recorded matters, so each chat is made up for a fake hub, as the hub records the fake agent's scripts.

use client_core::fake_hub::events::{self, said, text, typed_call};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Bounds, Pixels, SharedString, TestAppContext};
use protocol::capability::calls::Call;
use protocol::event::{ApprovalOutcome, Asking, ContentBlock, EventBody};
use protocol::fixtures::{ApprovalRequest, approval_request};
use protocol::model::ToolUseId;
use serde_json::json;

use super::super::{turns, wait_for_the_list_to_settle};
use super::made_up::{advice, command, compacted, open_on_chats_of, show, subagent};
use super::{COMMAND_LINE, LAST, PRINTED, everything, on_the_reply_saying};
use crate::support::Ui;

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
/// What the agent thinks, in words nothing else in the chat says.
const THOUGHT: &str = "The junction comes before the sidings, and each lever frame in its turn.\n\n";

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
    /// The chat that has it, and then the reply after it, which [`after`] says, as the hub records the fake agent's
    /// scripts that write them.
    chat: Vec<EventBody>,
    opens: Opens,
    copied: fn(&Ui<'_>) -> Copied,
}

/// What a copy has while a text is open, with what's around it, and how many times a copy has each of these once it's
/// closed again, for what else in the chat says them.
type Copied = (String, Vec<(String, usize)>);

/// A chat for each kind of text a call shows of its own, a thought, and a compaction, and then one with a plan. Each
/// but the plan is copied with the rest while it's open, between what comes before and after it, and isn't once it's
/// closed, though the rest is. A plan is selected with the rest, once it's approved.
#[gpui_kit::test]
fn each_kind_of_text_is_selected_with_the_rest_and_one_that_opens_only_while_it_s_open(cx: &mut TestAppContext) {
    let kinds = kinds();
    let mut chats = Vec::from_iter(kinds.iter().map(|kind| kind.chat.clone()));
    chats.push(planned());
    let (_fake, mut ui, chats, _) = open_on_chats_of(cx, chats);

    for (kind, chat) in kinds.into_iter().zip(&chats) {
        show(&mut ui, chat);
        let (name, reply) = (kind.name, after(kind.name));
        let (between, closed_has) = (kind.copied)(&ui);
        open_or_close(&mut ui, &kind.opens, true);
        if matches!(kind.opens, Opens::Call(SUBAGENT_LINE)) {
            // The subagent works in the background, kept apart, so its report is its record's, which comes on a
            // request of its own, apart from the chat's events, and is waited for rather than copied as soon as the
            // chat's last event is in.
            ui.wait("the subagent's report", |ui| !ui.shown("report/").is_empty());
        }
        let opened = everything_at_the_end(&mut ui, &reply);
        open_or_close(&mut ui, &kind.opens, false);
        let closed = everything_at_the_end(&mut ui, &reply);

        assert!(opened.contains(&between), "opened, {name} is copied, between the rest: {opened:?}");
        for (said, times) in closed_has {
            assert_eq!(closed.matches(&said).count(), times, "closed, {name} isn't copied: {closed:?}");
        }
        assert!(opened.ends_with(&reply) && closed.ends_with(&reply), "with {name} either way, the rest is copied");
    }
    show(&mut ui, chats.last().unwrap());
    let all = everything_at_the_end(&mut ui, LAST);

    // The prompt has the plan's words too, after what asks for it.
    assert!(all.contains(&format!("\n\n{PLAN}\n\n")), "the plan, between what's before and after it: {all:?}");
    assert!(all.ends_with(LAST), "to the last reply");
}

/// Each kind of text that shows only while it's open, each in a chat of its own.
fn kinds() -> [Kind; 5] {
    [
        Kind {
            name: "advice",
            chat: asked(&format!("advise:{ADVICE}"), "advice", advice(1, ADVICE)),
            opens: Opens::Call(ADVISOR_LINE),
            // The prompt says it too, before what follows it there.
            copied: |_| (format!("\n\n{ADVICE}\n\n{}", after("advice")), vec![(ADVICE.to_owned(), 1)]),
        },
        Kind {
            name: "a subagent's ask and report",
            chat: asked(
                &format!("agent-calls:1:{SUBAGENT_LINE}"),
                "a subagent's ask and report",
                subagent(1, SUBAGENT_LINE, ASKED, REPORT),
            ),
            opens: Opens::Call(SUBAGENT_LINE),
            copied: |_| {
                let reply = after("a subagent's ask and report");
                let after_its_prompt = format!("{SUBAGENT_LINE}; say:{reply}\n\n{ASKED}\n\n{REPORT}\n\n{reply}");
                (after_its_prompt, vec![(ASKED.to_owned(), 0), (REPORT.to_owned(), 0)])
            },
        },
        Kind {
            name: "a command and its output",
            chat: asked(
                &format!("tool:echo {PRINTED}"),
                "a command and its output",
                command(1, &format!("echo {PRINTED}"), &format!("{PRINTED}\n")),
            ),
            opens: Opens::Call(COMMAND_LINE),
            copied: |_| {
                let between = format!("\n\necho {PRINTED}\n\n{PRINTED}\n\n{}", after("a command and its output"));
                (between, vec![(PRINTED.to_owned(), 1)])
            },
        },
        Kind {
            name: "a thought",
            chat: asked("ponder:1:7", "a thought", vec![events::thought("msg-thought", THOUGHT)]),
            opens: Opens::Thinking,
            copied: |ui| {
                let start: String = thought(ui).chars().take(40).collect();
                (start.clone(), vec![(start, 0)])
            },
        },
        Kind {
            name: "a summary",
            chat: turns(vec![
                (
                    format!("/compact {KEEP}"),
                    vec![compacted(&format!("1. What was asked: the prompts so far.\n2. What to keep: {KEEP}"))],
                ),
                (format!("say:{}", after("a summary")), vec![text("msg-after", &after("a summary"))]),
            ]),
            opens: Opens::Summary,
            // The prompt that asked for the compaction says it too.
            copied: |_| (format!("What to keep: {KEEP}"), vec![(KEEP.to_owned(), 1)]),
        },
    ]
}

/// A chat of one turn, as the hub records the fake agent's `script` followed by a reply after the kind of text
/// `name`: what the script did, `did`, and then that reply.
fn asked(script: &str, name: &str, mut did: Vec<EventBody>) -> Vec<EventBody> {
    let reply = after(name);
    did.push(text("msg-after", &reply));
    turns(vec![(format!("{script}; say:{reply}"), did)])
}

/// A chat whose agent presented [`PLAN`], which was approved, and then said [`LAST`], as the hub records the fake
/// agent's scripts.
fn planned() -> Vec<EventBody> {
    let (call, approval) = ("toolu-plan", "approval-plan");
    let request = ApprovalRequest {
        tool_use_id: Some(ToolUseId::from(call)),
        input: json!({ "plan": PLAN }),
        asking: Some(Asking::Plan { plan: PLAN.to_owned() }),
        ..approval_request(approval, "ExitPlanMode")
    };
    let approved = EventBody::ApprovalResolved {
        approval_id: approval.into(),
        outcome: ApprovalOutcome::Allowed,
        answers_by_text: None,
        answers_by_position: None,
        by_setting: false,
    };
    let planning = vec![
        typed_call("msg-plan", call, Call::Plan),
        request.into(),
        approved,
        said(call, "User has approved your plan."),
    ];
    turns(vec![(format!("plan:{PLAN}"), planning), (format!("say:{LAST}"), vec![text("msg-last", LAST)])])
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
