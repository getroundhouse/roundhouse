//! A proposal the agent makes opens the proposal tab by itself, behind the chat, so nothing the person is doing is
//! interrupted. Open, on its line in the chat, brings the tab to the front, where the File menu's Close Chat says
//! Close Proposal Tab and `Cmd-W` closes it, and the tab closes with its chat. An answer chosen there is sent, and
//! the chat draws the prompt that told the agent as the proposal's line.

#![cfg(test)]

use std::time::Duration;

use desktop::workspace_pane::Showing;
use gpui_kit::{AppContext as _, ElementId, SharedString, TestAppContext};
use protocol::event::EventBody;
use protocol::model::{OptionPlace, SessionStatus};

use crate::menus::items;
use crate::queue::queued;
use crate::subagents::showing;
use crate::support::{Gate, Stack, Ui};
use crate::tools::in_a_new_workspace;

/// What the fake agent proposes: one question with two options, the first recommended.
const PROPOSE: &str = r#"mcp:propose {"title":"Holding a train","context":"Trains wait a minute.","questions":[{"header":"Holding","question":"Where does the wait live?","options":[{"label":"The signal","description":"It counts down."},{"label":"The train"}],"recommended":"A","why":"It survives a restart."}]}"#;

/// Has the agent propose, and waits for its line's Open. That button's ID.
pub(crate) fn a_proposal(ui: &mut Ui<'_>) -> SharedString {
    proposing(ui, PROPOSE)
}

/// Has the agent make the proposal `propose` says, as [`a_proposal`] does.
fn proposing(ui: &mut Ui<'_>, propose: &str) -> SharedString {
    let chat = ui.active_chat();
    ui.paste_text(&format!("{propose}; say:done"));
    ui.press("enter");
    ui.wait_idle_with("the turn to finish", |text| text.contains("done"));
    // It's there once the app has the proposal's record, which comes on the feed.
    ui.wait("the proposal's record", |ui| !ui.view().records().proposals(&chat).is_empty());
    let id = ui.view().records().proposals(&chat)[0].id.clone();
    let open = SharedString::from(format!("open-proposal/{id}"));
    ui.wait("the line's Open", |ui| ui.has(open.clone()));
    open
}

#[gpui_kit::test]
fn a_proposal_opens_its_tab_behind_the_chat_and_open_brings_it_to_the_front(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let open = a_proposal(&mut ui);
    ui.wait("the tab to open by itself", |ui| ui.has("show-proposal-tab"));
    assert_eq!(showing(&ui), Showing::Chat, "behind the chat");
    let line = open.replace("open-proposal/", "proposed/");
    assert!(ui.has(SharedString::from(line)), "a line that doesn't open, since the tab shows what it would");
    // As Open on an answer's line does before the proposal's record has come.
    let (pane, chat) = (ui.pane().unwrap(), ui.active_chat());
    ui.cx.update_entity(&pane, |pane, cx| pane.open_proposal_tab(chat, &"not-come-yet".into(), true, cx));
    assert_eq!(showing(&ui), Showing::Chat, "not the tab, with another proposal in it");

    ui.click(open);

    ui.wait("the proposal tab", |ui| showing(ui) == Showing::Proposal);
    assert_eq!(items(&ui, "File")[5], "Close Proposal Tab", "⌘W closes the tab, not the chat behind it");
    let outline = |part: u64| ElementId::NamedInteger("proposal-outline".into(), part);
    // The context, the question, its two options and the recommendation.
    assert!(ui.has(outline(4)) && !ui.has(outline(5)), "a part of the outline for each part of the page");
    ui.press("secondary-w");
    assert_eq!(showing(&ui), Showing::Chat);
    assert!(!ui.has("show-proposal-tab"));
}

/// The worker reads the drawing the proposal points to in the worktree as the agent proposes, the hub keeps it with
/// the proposal, and the page draws it, at the size it says, where the proposal points to it.
#[gpui_kit::test]
fn an_image_a_proposal_points_to_in_the_worktree_is_drawn_on_its_page(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let workspace = ui.new_workspace();
    let drawing = r##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="140"><rect width="80" height="140" fill="#3a6"/></svg>"##;
    std::fs::write(ui.worktree(&workspace).join("flow.svg"), drawing).unwrap();
    let propose = PROPOSE.replace("Trains wait a minute.", "Trains wait a minute.\\n\\n![How it flows](flow.svg)");

    let open = proposing(&mut ui, &propose);
    ui.click(open);

    ui.wait("the proposal tab", |ui| showing(ui) == Showing::Proposal);
    ui.wait("the drawing, from the hub", |ui| ui.shown("linked-image/").len() == 1);
    let image = ui.shown("linked-image/").remove(0);
    let drawn = ui.bounds(image);
    assert_eq!(drawn.size.height, gpui_kit::px(140.), "at the size it says: {drawn:?}");
}

/// A video the proposal points to in the worktree is kept with it as a drawing is, and plays on its page where it's
/// pointed to, as one in a reply does, so a choice about motion can be seen moving.
#[gpui_kit::test]
fn a_video_a_proposal_points_to_in_the_worktree_plays_on_its_page(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let workspace = ui.new_workspace();
    std::fs::write(ui.worktree(&workspace).join("demo.webm"), video::fixtures::TWO_SECONDS).unwrap();
    let propose = PROPOSE.replace("Trains wait a minute.", "Trains wait a minute.\\n\\n![The demo](demo.webm)");

    let open = proposing(&mut ui, &propose);
    ui.click(open);

    ui.wait("the proposal tab", |ui| showing(ui) == Showing::Proposal);
    ui.wait("the video's still, from the hub", |ui| !ui.labelled("Play").is_empty());
    assert!(ui.texts_shown().contains("0:00 / 0:02"), "how long it is, from the file");
    ui.click_labelled("Play", 0);
    assert_eq!(ui.labelled("Pause").len(), 1, "playing");
    // A frame drawn a little later each time, as a display would while it plays.
    ui.wait("a second in", |ui| {
        ui.play(Duration::from_millis(120));
        ui.texts_shown().contains("0:01 / 0:02")
    });
}

/// A Mermaid diagram in what the agent proposed is drawn on its page, as in a reply.
#[gpui_kit::test]
fn a_diagram_in_a_proposal_is_drawn_on_its_page(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let diagram = "\\n\\n```mermaid\\nflowchart TD\\n  A[Wait] --> B[Go]\\n```";
    let propose = PROPOSE.replace("Trains wait a minute.", &format!("Trains wait a minute.{diagram}"));

    let open = proposing(&mut ui, &propose);
    ui.click(open);

    ui.wait("the proposal tab", |ui| showing(ui) == Showing::Proposal);
    ui.wait("the diagram", |ui| ui.shown("diagram/").len() == 1);
    let diagram = ui.shown("diagram/").remove(0);
    assert!(ui.bounds(diagram).size.height > gpui_kit::px(80.), "two boxes, one under the other");
}

#[gpui_kit::test]
fn the_proposal_tab_closes_with_its_chat(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let open = a_proposal(&mut ui);
    let chat = ui.active_chat();
    ui.click(open);
    ui.wait("the proposal tab", |ui| showing(ui) == Showing::Proposal);

    ui.close_from_tab(&chat);

    ui.wait("the tab to close with its chat", |ui| {
        ui.cx.read_entity(&ui.pane().unwrap(), |p, cx| p.proposal_tab(cx).is_none())
    });
    assert_eq!(showing(&ui), Showing::Chat);
}

/// Choose on an option's card picks it, Send sends it once one is chosen for each question, and the chat draws the
/// prompt that told the agent as the proposal's line, not as something the person said.
#[gpui_kit::test]
fn an_answer_chosen_in_the_tab_is_sent_and_drawn_in_the_chat_as_the_proposals_line(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let open = a_proposal(&mut ui);
    let chat = ui.active_chat();
    ui.click(open);
    ui.wait("the proposal tab", |ui| showing(ui) == Showing::Proposal);
    assert!(ui.off("send-proposal-answer"), "nothing's chosen yet");

    ui.click("choose-0-1");
    ui.click("send-proposal-answer");

    let answered = |ui: &mut Ui<'_>| ui.view().records().proposals(&chat)[0].current.choice().cloned();
    ui.wait("the answer's record", |ui| answered(ui).is_some());
    let answered = answered(&mut ui).unwrap();
    assert_eq!(answered.chosen, [OptionPlace(1)]);
    assert!(!ui.has("choose-0-0"), "once it's answered, only what was chosen says so");
    ui.click("proposal-band-chat");
    ui.wait("the chat", |ui| showing(ui) == Showing::Chat);
    let id = ui.view().records().proposals(&chat)[0].id.clone();
    let told = |ui: &mut Ui<'_>| {
        ui.events().into_iter().find_map(|event| match event.body {
            EventBody::UserPrompt { client_msg_id: Some(message), from, .. } if from.responds_to() == Some(&id) => {
                Some((event.seq, message))
            }
            _ => None,
        })
    };
    ui.wait("the prompt that told the agent", |ui| told(ui).is_some());
    let (seq, message) = told(&mut ui).unwrap();
    ui.wait("its line", |ui| ui.has(SharedString::from(format!("answered-proposal/{seq}"))));
    assert!(!ui.has(SharedString::from(format!("bubble/prompt/{message}"))), "not as the person's prompt");
    let proposal = ui.view().records().proposals(&chat)[0].clone();
    let line = transcript::proposal_response_line(Some(&proposal), proposal.current.number);
    let said = format!("Answered {}: {}", line.title.unwrap_or_default(), line.said);
    ui.wait("it says what was chosen", |ui| !ui.labelled(&said).is_empty());
    ui.wait("its mark in the outline", |ui| !ui.labelled("Answered Holding a train").is_empty());
    assert!(!ui.labelled("Prompt 1 of 1").is_empty(), "and not counted among the person's prompts");
}

/// Has the agent propose `title`, from outside the app, so the composer needn't show, and waits for its record. Its ID.
fn another(ui: &mut Ui<'_>, stack: &Stack, title: &str) -> protocol::model::ProposalId {
    let chat = ui.active_chat();
    let proposals = |ui: &mut Ui<'_>| ui.view().records().proposals(&chat).to_vec();
    let before = proposals(ui).len();
    let propose = PROPOSE.replace("Holding a train", title);
    stack.client().session_prompt(&chat, &propose, None).unwrap();
    ui.wait("the new proposal's record", |ui| proposals(ui).len() > before);
    proposals(ui).last().unwrap().id.clone()
}

fn tab_shows(ui: &Ui<'_>) -> Option<protocol::model::ProposalId> {
    ui.cx.read_entity(&ui.pane().unwrap(), desktop::workspace_pane::WorkspacePane::proposal_tab)
}

/// A new proposal shows in the tab by itself only where it takes nothing from the person: not over the tab while it's
/// in front, and not over an answer they've chosen or written and not sent.
#[gpui_kit::test]
fn a_new_proposal_takes_the_tabs_place_only_where_nothing_is_lost(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    a_proposal(&mut ui);
    ui.wait("the tab to open by itself", |ui| ui.has("show-proposal-tab"));

    let second = another(&mut ui, &stack, "Clearing a platform");
    ui.wait("the second to take the place of the first, behind the chat", |ui| tab_shows(ui) == Some(second.clone()));
    let open = SharedString::from(format!("open-proposal/{second}"));
    ui.wait("the second's line, which the record can come before", |ui| ui.has(open.clone()));
    ui.click(open);
    ui.wait("the proposal tab", |ui| showing(ui) == Showing::Proposal);

    let third = another(&mut ui, &stack, "Closing a line");
    let (pane, chat) = (ui.pane().unwrap(), ui.active_chat());
    ui.wait("the pane to have the third, with the tab in front", |ui| {
        ui.cx.read_entity(&pane, |pane, _| pane.has_proposal(&chat, &third))
    });
    ui.click("proposal-band-chat");
    ui.wait("the third's line, once the pane has it", |ui| {
        ui.has(SharedString::from(format!("open-proposal/{third}")))
    });
    assert_eq!(tab_shows(&ui), Some(second.clone()), "not over the tab in front");

    ui.click(SharedString::from(format!("open-proposal/{second}")));
    ui.wait("the proposal tab", |ui| showing(ui) == Showing::Proposal);
    ui.click("choose-0-0");
    ui.click("proposal-band-chat");
    ui.wait("the chat", |ui| showing(ui) == Showing::Chat);
    let fourth = another(&mut ui, &stack, "Opening a line");
    ui.wait("the fourth's line", |ui| ui.has(SharedString::from(format!("open-proposal/{fourth}"))));
    assert_eq!(tab_shows(&ui), Some(second), "not over what's chosen and not sent");
}

/// Chooses the second option in the proposal tab, sends it, and goes back to the chat.
fn send_an_answer(ui: &mut Ui<'_>, open: SharedString) {
    ui.click(open);
    ui.wait("the proposal tab", |ui| showing(ui) == Showing::Proposal);
    ui.click("choose-0-1");
    ui.click("send-proposal-answer");
    ui.click("proposal-band-chat");
}

/// The prompt that told the agent of the answer, by where it's recorded, once it is.
fn told(ui: &mut Ui<'_>) -> Option<protocol::model::Seq> {
    ui.events().into_iter().find_map(|event| match &event.body {
        EventBody::UserPrompt { from, .. } if !from.is_person() => Some(event.seq),
        _ => None,
    })
}

/// An answer given while the agent works goes into its turn at once, as the person's own prompt would, rather than
/// waiting in the queue for the turn to end, so the agent can act on it sooner. It's the proposal's line meanwhile.
#[gpui_kit::test]
fn an_answer_given_while_the_agent_works_is_sent_into_its_turn(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let open = a_proposal(&mut ui);
    let chat = ui.active_chat();
    let done = Gate::new(&stack, "answer-steers");
    stack.client().session_prompt(&chat, &format!("hang-until:{done}"), None).unwrap();
    ui.wait("the agent to start working", |ui| ui.session_status() == Some(SessionStatus::Running));

    send_an_answer(&mut ui, open);

    ui.wait("the prompt that told the agent, while it works", |ui| told(ui).is_some());
    assert_eq!(ui.session_status(), Some(SessionStatus::Running), "sent into the turn, not after it");
    assert!(queued(&ui).is_empty(), "never left in the queue");
    let seq = told(&mut ui).unwrap();
    ui.wait("its line in the chat", |ui| ui.has(SharedString::from(format!("answered-proposal/{seq}"))));
    done.open();
    let taken = |ui: &mut Ui<'_>| {
        ui.events().into_iter().any(|event| matches!(event.body, EventBody::PromptTaken { prompt } if prompt == seq))
    };
    ui.wait("the agent to take it", taken);
    let answers = ui
        .events()
        .into_iter()
        .filter(|event| matches!(&event.body, EventBody::UserPrompt { from, .. } if !from.is_person()));
    assert_eq!(answers.count(), 1, "the agent is told once");
}

/// Stopped before it read the answer, the agent never got it, and the answer's line says so, and where it still is,
/// since the proposal keeps it.
#[gpui_kit::test]
fn an_answer_the_agent_never_got_says_so(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let open = a_proposal(&mut ui);
    let chat = ui.active_chat();
    let done = Gate::new(&stack, "answer-dropped");
    stack.client().session_prompt(&chat, &format!("hang-until:{done}"), None).unwrap();
    ui.wait("the agent to start working", |ui| ui.session_status() == Some(SessionStatus::Running));
    send_an_answer(&mut ui, open);
    ui.wait("the prompt that told the agent", |ui| told(ui).is_some());
    let seq = told(&mut ui).unwrap();

    stack.client().session_cancel(&chat).unwrap();

    ui.wait("its line to say the agent never got it", |ui| {
        ui.has(SharedString::from(format!("answer-never-got/{seq}")))
    });
    let answered = ui.view().records().proposals(&chat)[0].current.response.is_some();
    assert!(answered, "the proposal keeps the answer, for the agent to read");
    done.open();
}

/// While the queue waits for the person, such as after they stopped the agent, the answer waits there too, as the
/// proposal's line, which can't be edited. Taken out, it takes the answer back, and the proposal waits for the person
/// again, with what was chosen still chosen.
#[gpui_kit::test]
fn an_answer_given_while_the_queue_is_held_waits_there_and_can_be_taken_back(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let open = a_proposal(&mut ui);
    let chat = ui.active_chat();
    let done = Gate::new(&stack, "answer-held");
    stack.client().session_prompt(&chat, &format!("hang-until:{done}"), None).unwrap();
    ui.wait("the agent to start working", |ui| ui.session_status() == Some(SessionStatus::Running));
    stack.client().session_cancel(&chat).unwrap();
    ui.wait("the queue held", |ui| crate::queue::held(ui));
    done.open();

    send_an_answer(&mut ui, open.clone());

    ui.wait("the answer in the queue", |ui| queued(ui).len() == 1);
    let message = queued(&ui)[0].client_msg_id.clone();
    ui.wait("its row", |ui| ui.has(SharedString::from(format!("queued-{message}"))));
    assert!(!ui.has(SharedString::from(format!("edit-queued-{message}"))), "not edited, since the hub wrote it");
    ui.hover(SharedString::from(format!("queued-{message}")));
    ui.click(SharedString::from(format!("remove-queued-{message}")));
    let answered = |ui: &mut Ui<'_>| ui.view().records().proposals(&chat)[0].current.response.is_some();
    ui.wait("the answer taken back with it", |ui| queued(ui).is_empty() && !answered(ui));
    ui.click(open);
    ui.wait("the proposal tab", |ui| showing(ui) == Showing::Proposal);
    ui.click("send-proposal-answer");
    ui.wait("the same answer queued again", |ui| queued(ui).len() == 1 && answered(ui));
    assert!(told(&mut ui).is_none(), "the agent's told nothing while the queue's held");
}
