//! Selecting in the views other than a chat that take its selection: a Markdown file rendered in the file tab, and a
//! proposal's page. A drag held past the bottom of each scrolls it on, as it does the chat, and so does a run of
//! Shift-Down. How the keys move the end and keep it in view is tested in `crates/transcript/src/selection/keys`, and
//! here only that each view takes them. Only what the hub holds matters, so each is made up for a fake hub.

use client_core::fake_hub::World;
use desktop::workspace_pane::Showing;
use gpui_kit::base::test_support::texts;
use gpui_kit::{
    AppContext as _, Bounds, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, ScrollDelta,
    ScrollEvent, TestAppContext, point, px,
};
use protocol::model::{
    FileContents, Proposal, ProposalOption, ProposalQuestion, ProposalRevision, Proposed, RevisionNumber,
    SessionStatus, WorktreeFiles,
};

use super::super::{A_FRAME, press};
use super::{click, copy, head};
use crate::subagents::showing;
use crate::support::{FakeHub, Ui};

/// How many paragraphs each view holds, far more than the window shows.
pub(in crate::scrolling) const PARAGRAPHS: usize = 120;
/// How many frames a drag is held past the bottom.
const HELD: usize = 60;

/// A drag from the first paragraph of a long Markdown file, held past the bottom of the file tab, scrolls it on, and
/// selects what it scrolls past.
#[gpui_kit::test]
fn a_drag_held_past_the_bottom_of_a_rendered_markdown_file_scrolls_it_on(cx: &mut TestAppContext) {
    let (_fake, mut ui) = with_a_file(cx, &paragraphs("Paragraph"));
    ui.click("all-file-long.md");
    ui.wait("the file, rendered", |ui| painted(ui, "Paragraph 1.").is_some());

    let (shown, selected) = dragged_past_the_bottom(&mut ui, "Paragraph");

    assert!(selected.starts_with("Paragraph 1.\n\nParagraph 2."), "from where it started: {:?}", head(&selected));
    let last = last_paragraph(&selected, "Paragraph");
    assert!(last > shown, "on past paragraph {shown}, the last that showed, to {last}");
}

/// A drag from the first paragraph of a long proposal's context, held past the bottom of its page, scrolls the page
/// on, and selects what it scrolls past.
#[gpui_kit::test]
fn a_drag_held_past_the_bottom_of_a_proposal_scrolls_it_on(cx: &mut TestAppContext) {
    let (_fake, mut ui) = with_a_proposal(cx, &paragraphs("Context"));
    ui.wait("the proposal tab", |ui| showing(ui) == Showing::Proposal && painted(ui, "Context 1.").is_some());

    let (shown, selected) = dragged_past_the_bottom(&mut ui, "Context");

    // The context is one text, so a copy has its paragraphs a line apart.
    assert!(selected.starts_with("Context 1.\nContext 2."), "from where it started: {:?}", head(&selected));
    let last = last_paragraph(&selected, "Context");
    assert!(last > shown, "on past paragraph {shown}, the last that showed, to {last}");
}

/// A run of Shift-Down from the first paragraph of a long Markdown file goes on past the bottom of the file tab, which
/// scrolls to keep the end in view.
#[gpui_kit::test]
fn shift_down_goes_on_past_the_bottom_of_a_rendered_markdown_file(cx: &mut TestAppContext) {
    let (_fake, mut ui) = with_a_file(cx, &paragraphs("Paragraph"));
    ui.click("all-file-long.md");
    ui.wait("the file, rendered", |ui| painted(ui, "Paragraph 1.").is_some());

    let (shown, last, in_view) = keyed_past_the_bottom(&mut ui, "Paragraph");

    assert!(last > shown, "on past paragraph {shown}, the last that showed, to {last}");
    assert!(in_view, "with paragraph {last} in view");
}

/// A key pressed once the file tab has been scrolled away from what's selected goes back to it, as the chat does.
#[gpui_kit::test]
fn a_key_brings_the_end_back_after_a_rendered_markdown_file_is_scrolled_away(cx: &mut TestAppContext) {
    let (_fake, mut ui) = with_a_file(cx, &paragraphs("Paragraph"));
    ui.click("all-file-long.md");
    ui.wait("the file, rendered", |ui| painted(ui, "Paragraph 1.").is_some());
    let start = on_the_first(&mut ui, "Paragraph");
    click(&mut ui, start, 2);
    let word = copy(&mut ui);
    let over = start + point(px(100.), px(100.));
    press(&mut ui, MouseMoveEvent { position: over, ..Default::default() });
    let away = ScrollDelta::Pixels(point(px(0.), px(-3000.)));
    press(&mut ui, ScrollEvent { position: over, delta: away, ..Default::default() });
    ui.settle(A_FRAME);
    let scrolled_away = painted(&mut ui, "Paragraph 1.").is_none();

    ui.press("shift-right");
    ui.settle(A_FRAME);

    assert_eq!(word, "Paragraph", "a word of it");
    assert!(scrolled_away, "scrolled away from it");
    assert_eq!(copy(&mut ui), "Paragraph ", "a character on");
    assert!(painted(&mut ui, "Paragraph 1.").is_some(), "and back in view");
}

/// A run of Shift-Down from the first paragraph of a long proposal's context goes on past the bottom of its page,
/// which scrolls to keep the end in view.
#[gpui_kit::test]
fn shift_down_goes_on_past_the_bottom_of_a_proposal(cx: &mut TestAppContext) {
    let (_fake, mut ui) = with_a_proposal(cx, &paragraphs("Context"));
    ui.wait("the proposal tab", |ui| showing(ui) == Showing::Proposal && painted(ui, "Context 1.").is_some());

    let (shown, last, in_view) = keyed_past_the_bottom(&mut ui, "Context");

    assert!(last > shown, "on past paragraph {shown}, the last that showed, to {last}");
    assert!(in_view, "with paragraph {last} in view");
}

/// Past the end of a short file, which fits its tab, a key with nowhere to go stops, and asks for no more frames, as
/// [`Ui::settle`] checks.
#[gpui_kit::test]
fn a_key_past_the_end_of_a_short_markdown_file_stops(cx: &mut TestAppContext) {
    let (_fake, mut ui) = with_a_file(cx, "Only one paragraph.\n\nAnd its last.");
    ui.click("all-file-long.md");
    ui.wait("the file, rendered", |ui| painted(ui, "And its last.").is_some());
    let last = painted(&mut ui, "And its last.").unwrap();
    click(&mut ui, last.origin + point(px(2.), px(4.)), 1);

    // To the end of its last text, and two past it, and down from its last line.
    for keys in ["shift-right"; 15].into_iter().chain(["shift-down"; 2]) {
        ui.press(keys);
        ui.settle(A_FRAME);
    }

    assert_eq!(copy(&mut ui), "And its last.", "to the end of the file, and no further");
}

/// Past the end of a proposal's page, a key with nowhere to go stops, and asks for no more frames.
#[gpui_kit::test]
fn a_key_past_the_end_of_a_proposal_stops(cx: &mut TestAppContext) {
    let (_fake, mut ui) = with_a_proposal(cx, "Only one paragraph.");
    ui.wait("the proposal tab", |ui| showing(ui) == Showing::Proposal && painted(ui, "Only one paragraph.").is_some());
    let start = on_the_first_of(&mut ui, "Only one paragraph.");
    click(&mut ui, start, 1);

    for keys in ["shift-down"; 20].into_iter().chain(["shift-right"; 40]) {
        ui.press(keys);
        ui.settle(A_FRAME);
    }

    let selected = copy(&mut ui);
    assert!(selected.starts_with("Only one paragraph."), "from the context on: {:?}", head(&selected));
}

/// [`PARAGRAPHS`] paragraphs of Markdown, each saying `word` and which it is.
pub(in crate::scrolling) fn paragraphs(word: &str) -> String {
    Vec::from_fn(PARAGRAPHS, |n| format!("{word} **{}**.", n + 1)).join("\n\n")
}

/// The app on a fake hub, with a workspace whose worktree holds `long.md`, saying `text`, and All files showing it.
pub(in crate::scrolling) fn with_a_file<'a>(cx: &'a mut TestAppContext, text: &str) -> (FakeHub, Ui<'a>) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    world.chat(&workspace, SessionStatus::Idle, Vec::new());
    world.worktree = WorktreeFiles { paths: vec!["long.md".to_owned()], folders: Vec::new(), truncated: false };
    world.worktree_files.insert("long.md".to_owned(), FileContents::Text { text: text.to_owned() });
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    ui.wait("a workspace", |ui| ui.view().selected().is_some() && ui.pane().is_some());
    ui.click("sidebar-tab-all-files");
    ui.wait("All files to list it", |ui| ui.has("all-file-long.md"));
    (fake, ui)
}

/// The app on a fake hub, with a chat that has made a proposal whose context says `context`, open in its tab.
pub(in crate::scrolling) fn with_a_proposal<'a>(cx: &'a mut TestAppContext, context: &str) -> (FakeHub, Ui<'a>) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    let chat = world.chat(&workspace, SessionStatus::Idle, Vec::new());
    let session = world.sessions.last().unwrap().clone();
    let option = |label: &str| ProposalOption {
        label: label.into(),
        description: format!("{label}, as it says."),
        body: String::new(),
        letter: None,
    };
    let question = ProposalQuestion {
        header: "Holding".into(),
        text: "Where does a held train's wait live?".into(),
        details: String::new(),
        options: vec![option("The signal"), option("The train")],
        recommended: None,
    };
    let proposed = Proposed { title: "Holding a train".into(), context: context.to_owned(), questions: vec![question] };
    let revision = ProposalRevision {
        number: RevisionNumber::FIRST,
        changed: String::new(),
        call: None,
        proposed,
        made_at_ms: session.created_at_ms,
        response: None,
        images: Vec::new(),
    };
    let proposal = Proposal {
        id: "proposal-1".into(),
        workspace_id: session.workspace_id.clone(),
        session_id: chat.clone(),
        current: revision.clone(),
        revisions: vec![revision.summary()],
        withdrawn: None,
        created_at_ms: session.created_at_ms,
        rev: protocol::model::Rev::new(1),
    };
    let id = proposal.id.clone();
    world.proposals.insert(chat.clone(), vec![proposal]);
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    ui.wait("the chat", |ui| ui.transcript().is_some());
    let pane = ui.pane().unwrap();
    ui.wait("the proposal's record", |ui| {
        ui.cx.update_entity(&pane, |pane, cx| pane.open_proposal_tab(chat.clone(), &id, true, cx));
        showing(ui) == Showing::Proposal
    });
    (fake, ui)
}

/// Where the kit painted the text that says `said`, if it shows.
pub(in crate::scrolling) fn painted(ui: &mut Ui<'_>, said: &str) -> Option<Bounds<Pixels>> {
    ui.frame();
    let found = |window: &mut gpui_kit::Window| {
        texts(window).into_iter().find(|text| text.visible() && text.text() == said).map(|text| text.bounds())
    };
    ui.cx.update_window(ui.handle, |_, window, _| found(window)).unwrap()
}

/// The last of the paragraphs that say `word` that shows, counting from the first.
pub(in crate::scrolling) fn last_shown(ui: &mut Ui<'_>, word: &str) -> usize {
    (1..=PARAGRAPHS).take_while(|n| painted(ui, &format!("{word} {n}.")).is_some()).last().unwrap_or(0)
}

/// Where a press on the first of the paragraphs that say `word` goes.
pub(in crate::scrolling) fn on_the_first(ui: &mut Ui<'_>, word: &str) -> gpui_kit::Point<Pixels> {
    on_the_first_of(ui, &format!("{word} 1."))
}

/// Where a press at the start of the text that says `said` goes.
fn on_the_first_of(ui: &mut Ui<'_>, said: &str) -> gpui_kit::Point<Pixels> {
    painted(ui, said).unwrap().origin + point(px(2.), px(4.))
}

/// Presses on the first of the paragraphs that say `word`, holds the drag past the window's bottom for [`HELD`] frames,
/// and lets go. Says the last of them that showed before, and what a copy has of what the drag selected.
fn dragged_past_the_bottom(ui: &mut Ui<'_>, word: &str) -> (usize, String) {
    let (shown, start) = (last_shown(ui, word), on_the_first(ui, word));
    let bottom = ui.cx.update_window(ui.handle, |_, window, _| window.viewport_size().height).unwrap();
    let below = point(start.x, bottom + px(30.));
    let (button, held) = (MouseButton::Left, Some(MouseButton::Left));

    press(ui, MouseDownEvent { button, position: start, click_count: 1, ..Default::default() });
    press(ui, MouseMoveEvent { position: below, pressed_button: held, ..Default::default() });
    for _ in 0..HELD {
        ui.play(A_FRAME);
    }
    press(ui, MouseUpEvent { button, position: below, click_count: 1, ..Default::default() });

    (shown, copy(ui))
}

/// Clicks the first of the paragraphs that say `word`, and presses Shift-Down, with the frames each asks for played
/// after it, until the end of what's selected is a few paragraphs past the last that showed before. Says which that
/// was, the last paragraph a copy has whole, and whether that shows now.
fn keyed_past_the_bottom(ui: &mut Ui<'_>, word: &str) -> (usize, usize, bool) {
    let (shown, start) = (last_shown(ui, word), on_the_first(ui, word));
    click(ui, start, 1);
    for _ in 0..shown + 3 {
        ui.press("shift-down");
        ui.settle(A_FRAME);
    }
    let last = last_paragraph(&copy(ui), word);
    (shown, last, painted(ui, &format!("{word} {last}.")).is_some())
}

/// Which the last of the paragraphs that say `word` in `selected` is, as far as it has it.
fn last_paragraph(selected: &str, word: &str) -> usize {
    let last = selected.lines().rev().find(|paragraph| paragraph.starts_with(word)).unwrap_or_default();
    last.trim_start_matches(word).trim().trim_end_matches('.').parse().unwrap_or(0)
}
