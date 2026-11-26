//! A request to use a tool, asked in the composer's place.

#![cfg(test)]

use client_core::fake_hub::World;
use client_core::fake_hub::events::{command, first_turn, user_prompt};
use client_core::records::HubRecords;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ClipboardItem, TestAppContext, point, px};
use protocol::capability::approvals::{Asks, Decision};
use protocol::capability::calls::Call;
use protocol::capability::calls::call::Command;
use protocol::event::EventBody;
use protocol::fixtures::{
    ApprovalRequest, allow_once, approval_request, deny, tool_server, typed_command, typed_request,
};
use protocol::model::{Records, SessionStatus, ToolServer, ToolServerName};
use serde_json::json;

use crate::support::{FakeHub, Stack, Ui, id, one_chat, open_on_the_chat};

/// A removal the agent asks about even with full access, since what it removes is only known once it runs.
const REMOVAL: &str = r#"rm -rf "$(cat gone-path)""#;

/// What was decided of each request so far, with `None` for one nobody decided.
fn outcomes(ui: &Ui) -> Vec<Option<Decision>> {
    let outcome = |event: &protocol::event::SessionEvent| match &event.body {
        EventBody::ApprovalResolved { decision, .. } => Some(decision.as_deref().cloned()),
        _ => None,
    };
    ui.events().iter().filter_map(outcome).collect()
}

/// A chat with full access, in a worktree with a directory `gone` for [`REMOVAL`] to remove. Making it isn't asked
/// about, which is what says the chat has full access.
fn with_something_to_remove<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> (Ui<'a>, std::path::PathBuf) {
    let mut ui = Ui::open_with_a_repository(cx, stack);
    let workspace = ui.new_workspace();
    let gone = ui.worktree(&workspace).join("gone");
    ui.input(r#"tool:mkdir gone && echo "$PWD/gone" > gone-path"#);
    ui.press("enter");
    ui.wait("the directory", |ui| ui.session_status() == Some(SessionStatus::Idle) && gone.exists());
    (ui, gone)
}

#[gpui_kit::test]
fn a_tools_use_is_asked_in_the_composers_place_and_denied_in_the_persons_words(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, gone) = with_something_to_remove(cx, &stack);

    // It's asked in the composer's place, with the command and why it's asked about, and what it's for apart from
    // them. Denying it is one of its answers, so there's nothing to skip it with.
    ui.ask(&format!("tool:{REMOVAL}"));
    assert!(!ui.has(id("add")), "the composer is put away");
    assert!(ui.shows_label("question-given", REMOVAL), "the command, whole");
    assert!(ui.has(id("question-reason")) && !ui.has(id("question-skip")));
    assert!(ui.shows_label("question-text", "Run a command the test asked for"), "what the agent said it's for");

    // Enter in the typing row sends only what's typed there: with Allow chosen and nothing typed, it sends nothing,
    // since the row says it denies.
    ui.click(id("question-answer-0"));
    ui.click(id("question-other"));
    ui.press("enter");
    ui.frame();
    assert_eq!(ui.session_status(), Some(SessionStatus::WaitingApproval), "nothing was sent");
    assert!(gone.exists() && outcomes(&ui).is_empty());

    // Nor does a sentence meant for the composer: a key that types something takes back the Allow its `1` chose, so
    // its Enter sends nothing.
    ui.let_what_is_asked_settle();
    for key in ["1", "x"] {
        ui.press(key);
    }
    ui.let_what_is_asked_settle();
    ui.press("enter");
    ui.view().drafts().flushed().recv().unwrap();
    assert_eq!(ui.session_status(), Some(SessionStatus::WaitingApproval), "nothing was sent");
    assert!(gone.exists() && outcomes(&ui).is_empty());

    // What's typed there denies it in those words.
    ui.click(id("question-other"));
    ui.input("Remove it by its full path");
    ui.press("enter");
    ui.wait("the person's words", |ui| ui.told_to_agent().contains("Remove it by its full path"));
    ui.wait_for_the_turn();
    assert_eq!(outcomes(&ui), [Some(deny(Some("Remove it by its full path")))]);
    assert!(gone.exists(), "denied, so nothing was removed");
    assert!(ui.has(id("add")), "the composer is back");
    let denied = "Denied: Remove it by its full path";
    ui.wait("the call to say so", |ui| ui.shows_label("call-denied", denied));
}

#[gpui_kit::test]
fn subagents_that_ask_at_once_are_asked_in_turn_with_who_asks_and_how_many_wait(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, gone) = with_something_to_remove(cx, &stack);

    // Two subagents ask before either is answered. The first is asked, by what it's for, and the other waits.
    ui.ask(&format!("agents-ask:2:{REMOVAL}"));
    ui.wait("who asks", |ui| ui.shows_label("question-header", "Command · Check 1"));
    ui.wait("the count of what waits", |ui| ui.shows_label("question-more-waiting", "1 more waiting"));

    // Denied, the next is asked, with nothing waiting behind it.
    ui.click(id("question-answer-1"));
    ui.click(id("question-send"));
    ui.wait("the second to be asked", |ui| ui.shows_label("question-header", "Command · Check 2"));
    assert!(!ui.has(id("question-more-waiting")));
    assert!(gone.exists(), "the first was denied");

    // Allowed, its command runs, and the turn ends.
    ui.click(id("question-answer-0"));
    ui.click(id("question-send"));
    // The chat's record says it's idle on the hub's feed, apart from its events, and the worktree changes on the disk,
    // so the answers recorded are waited for, with the end of their turn, rather than read as soon as the chat is idle.
    ui.wait("the removal, and its turn to be drawn to its end", |ui| {
        ui.session_status() == Some(SessionStatus::Idle) && !gone.exists() && ui.turn_drawn_to_its_end()
    });
    assert_eq!(outcomes(&ui), [Some(deny(None)), Some(allow_once())]);
}

/// Only what's asked matters here, and nothing is answered, so the chat is made up for a fake hub.
#[gpui_kit::test]
fn the_command_asked_about_can_be_selected_and_copied_while_the_typing_row_has_focus(cx: &mut TestAppContext) {
    let (_fake, mut ui) = asked_to_remove(cx);
    ui.click(id("question-other"));

    let given = ui.bounds(id("question-given"));
    let from = point(given.left() + px(14.), given.center().y);
    let to = point(given.right() - px(14.), given.center().y);
    ui.cx.update_window(ui.handle, |_, window, cx| window.drag(from, to, cx)).unwrap();
    ui.frame();
    ui.cx.write_to_clipboard(ClipboardItem::new_string(String::new()));
    ui.press("secondary-c");

    let copied = ui.cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some(REMOVAL));
}

/// The server is named as the hub has it now, so one removed while the request waits, or added again under another
/// name, shows so. The panel stays the one it was, with what's chosen and typed in it.
#[gpui_kit::test]
fn a_request_to_use_a_servers_tool_names_it_as_the_person_does_and_follows_the_hubs_servers(cx: &mut TestAppContext) {
    let input = json!({ "title": "Signals should say why a train stopped" });
    let call = Call::Mcp { server: "GitHub_Northwind".into(), tool: "create_issue".into(), input };
    let events = first_turn(user_prompt("File it."), [typed_request("approval-1", "mcp-1", call)]);
    let (mut world, chat) = one_chat(SessionStatus::WaitingApproval, events);
    let named = |id: &str, name: &str| ToolServer { name: ToolServerName::stored(name.into()), ..tool_server(id) };
    world.tool_servers.push(named("github", "GitHub (Northwind)"));
    let (_fake, mut ui) = open_on_the_chat(cx, world, &chat);

    let whole = "{\n  \"title\": \"Signals should say why a train stopped\"\n}";
    ui.wait("the request", |ui| ui.shows_label("question-given", whole));
    assert!(ui.shows_label("question-header", "Create issue · GitHub (Northwind)"));
    assert!(ui.shows_label("question-text", "Use this tool?"));

    let transcript = ui.transcript().unwrap();
    let panel = |ui: &Ui<'_>| ui.cx.read_entity(&transcript, |t, _| t.question().unwrap().entity_id());
    let before = panel(&ui);
    let mut follow = |tool_servers: Vec<ToolServer>, shown: &str| {
        let mut held = HubRecords::default();
        held.apply_list(Records { tool_servers, ..Records::default() });
        ui.cx.update_entity(&transcript, |t, cx| t.records_changed(&held.snapshot(), cx));
        assert!(ui.shows_label("question-header", shown), "{shown}");
    };
    follow(vec![named("github-2", "GitHub, Northwind")], "Create issue · GitHub, Northwind");
    follow(Vec::new(), "Create issue · GitHub_Northwind");
    assert_eq!(panel(&ui), before);
}

/// A chat held by a fake hub whose agent asks to run [`REMOVAL`], as the hub records it from Claude, open with the
/// request asked in the composer's place. The hub goes with the test, so it's returned to be kept.
fn asked_to_remove(cx: &mut TestAppContext) -> (FakeHub, Ui<'_>) {
    let says_its_for = "Run a command the test asked for";
    let removal = Command { description: Some(says_its_for.to_string()), ..typed_command(REMOVAL) };
    let request = ApprovalRequest {
        tool_use_id: Some("toolu-1".into()),
        ..approval_request("approval-1", Asks::Call { call: Call::Command(removal) })
    };
    let request =
        request.because("Dangerous rm operation on statically-unresolvable target: command substitution output");
    let events = first_turn(
        user_prompt(&format!("tool:{REMOVAL}")),
        [command("msg-1", "toolu-1", REMOVAL, Some(says_its_for)), request.into()],
    );
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    world.chat(&workspace, SessionStatus::WaitingApproval, events);
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    ui.wait("the request", |ui| ui.shows_label("question-given", REMOVAL));
    (fake, ui)
}
