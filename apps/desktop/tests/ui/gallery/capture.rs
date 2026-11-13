//! Opens the real app on a screen's hub, in a window that's never on screen, and takes its picture.

#![cfg(test)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use desktop::app::AppView;
use desktop::pull_request::Lookup;
use desktop::sidebar_tabs::SidebarTab;
use desktop::updates::{Kind, Problem};
use desktop::workspace_pane::WorkspacePane;
use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, App, ClipboardEntry, ClipboardItem, Entity, ExternalPaths, HeadlessAppContext, Image, ImageFormat,
    PlatformTextSystem,
};
use image::RgbaImage;

use super::frame;
use super::steps::{Screen, Step, Target};
use crate::support::{first_labelled, menu_item};

mod files;
mod finding;
mod opening;
mod scrollbar;

pub(crate) use finding::{FoundBothWays, found_both_ways};
use opening::open;
use scrollbar::ChatScroll;

/// Long enough for the app to load a screen from a hub on the same machine, many times over.
const TIMEOUT: Duration = Duration::from_secs(20);
/// How many pictures in a row have to match before a screen counts as finished drawing.
const SAME_IN_A_ROW: usize = 3;
const BETWEEN_PICTURES: Duration = Duration::from_millis(50);
/// A tooltip, or the update card's details, shows once the pointer has rested on it for half a second.
const LONGER_THAN_A_TOOLTIP_WAITS: Duration = Duration::from_secs(1);
/// Somewhere to move the pointer that has no hover of its own: the line in the sidebar saying which build this is.
/// It isn't drawn while the sidebar shows an update's card, so a screen with one can't [`Step::Open`] a line or
/// [`Step::MoveAway`].
const AWAY: &str = "build-line";

pub(crate) struct Picture {
    pub(crate) image: RgbaImage,
    /// It stopped changing. If not, something on the screen moves, and the picture won't match from run to run.
    pub(crate) settled: bool,
}

/// Its picture, cut to its frame, and what's wrong with the frame, each said as what's wrong with the screen.
pub(crate) fn take(screen: &Screen, text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Vec<String>) {
    let (_dir, mut app) = open(screen.world, screen.frame.in_the_column(), text_system);
    for step in screen.steps {
        app.step(step);
    }
    let picture = app.settle();
    let picture = app.without_the_chats_scrollbar(screen, picture);
    let cropped = app.cx.update_window(app.handle, |_, window, cx| frame::crop(screen, window, &picture.image, cx));
    let (image, problems) = cropped.unwrap();
    app.close();
    (Picture { image, ..picture }, problems)
}

struct Driver {
    cx: HeadlessAppContext,
    handle: AnyWindowHandle,
    view: Entity<AppView>,
    /// The screen's own directory, for files it pastes.
    dir: PathBuf,
    chat: ChatScroll,
}

/// Whether the chat has every row it holds. It holds only the agent's own events, so how many there are isn't the
/// session's last `seq`.
fn chat_loaded(pane: &WorkspacePane, cx: &App) -> bool {
    let empty = pane.session().is_none_or(|s| s.last_seq == 0);
    pane.transcript().is_some_and(|t| {
        let t = t.read(cx);
        t.loaded_events() == t.row_count() && (empty || t.row_count() > 0)
    })
}

/// Whether the sidebar of changes shows what it starts on: the changes, or all the files where there are none.
fn sidebar_loaded(pane: &WorkspacePane, cx: &App) -> bool {
    pane.diff.as_ref().is_some_and(|diff| match pane.sidebar_tab() {
        Some(SidebarTab::Changes) => true,
        Some(SidebarTab::AllFiles) => diff.all_files.read(cx).is_listed(),
        None => false,
    })
}

impl Driver {
    fn step(&mut self, step: &Step) {
        match step {
            Step::Select(name) => {
                let view = self.view.clone();
                self.cx.update(|cx| {
                    view.update(cx, |v, cx| {
                        let id =
                            v.records().workspaces().iter().find(|w| &w.name == name).map(|w| w.id.clone()).unwrap();
                        v.select(id, cx);
                    });
                });
                self.wait("the chat, the sidebar of changes and the pull request to load", |view, cx| {
                    selected_pane(view).is_some_and(|pane| {
                        let pane = pane.read(cx);
                        chat_loaded(pane, cx) && sidebar_loaded(pane, cx) && pane.pull_request != Lookup::Looking
                    })
                });
            }
            Step::OpenFile(path) => self.open_file(path),
            Step::AllFiles => self.all_files(),
            Step::ViewFile(path) => self.view_file(path),
            Step::WaitFor(id) => self.wait_for(&gpui_kit::SharedString::from(*id).into()),
            Step::StartComment(line) => self.start_comment(*line),
            Step::Click(id) => self.click(*id),
            Step::MoveAway => self.move_away(),
            Step::HoverTab(position) => self.hover_tab(*position),
            Step::Point(id) => self.point(*id),
            Step::Type(text) => {
                self.frame();
                self.cx.update_window(self.handle, |_, window, cx| window.input(text, cx)).unwrap();
            }
            Step::Hover { on, shows } => self.hover(*on, shows),
            Step::PasteImage(image) => self.paste(ClipboardItem::from(Image::from_bytes(ImageFormat::Png, image()))),
            Step::PasteFile(name, text) => self.paste_file(name, text),
            Step::AskToFinish => {
                self.step(&Step::Click("finish-update"));
                self.wait("the question", |view, _| view.update_question.is_some());
            }
            Step::RightClick(id) => self.right_click(id),
            Step::ClickItem(label) => self.click_item(label),
            Step::UpdateReady(version) => self.cx.update(|cx| desktop::updates::show_ready(version, cx)),
            Step::UpdateProblem => {
                let problem = Problem { kind: Kind::Unreachable, detail: "connection refused".to_string() };
                self.cx.update(|cx| desktop::updates::show_problem(problem, cx));
            }
            Step::OpenSettings => {
                let open = Box::new(desktop::OpenSettings);
                self.cx.update_window(self.handle, |_, window, cx| window.dispatch_action(open, cx)).unwrap();
            }
            Step::Open(label) => self.open(label),
            Step::Find(query) => self.find_in_chat(query),
            Step::OpenSetupLog => {
                self.update_pane(WorkspacePane::toggle_setup_log);
                self.wait("the setup log to load", |view, cx| {
                    selected_pane(view)
                        .is_some_and(|pane| pane.read(cx).setup_log.as_ref().is_some_and(|t| !t.is_empty()))
                });
            }
        }
    }

    fn hover_tab(&mut self, position: usize) {
        self.frame();
        let view = self.view.clone();
        let chat = self.cx.update(|cx| {
            let pane = selected_pane(view.read(cx)).unwrap();
            let open: Vec<_> = pane.read(cx).sessions().iter().filter(|s| s.closed_at_ms.is_none()).collect();
            open[position].id.clone()
        });
        self.point(format!("chat-{chat}"));
    }

    fn point(&mut self, id: impl Into<gpui_kit::SharedString>) {
        self.frame();
        let id = id.into();
        self.cx.update_window(self.handle, |_, window, cx| window.hover(id, cx)).unwrap();
    }

    fn right_click(&mut self, id: &'static str) {
        self.frame();
        let id = gpui_kit::SharedString::from(id);
        self.cx.update_window(self.handle, |_, window, cx| window.right_click(id, cx)).unwrap();
        // The menu opens on the next frame.
        self.frame();
    }

    fn click_item(&mut self, label: &str) {
        self.frame();
        let id = self.cx.update_window(self.handle, |_, window, _| menu_item(window, label)).unwrap();
        self.cx.update_window(self.handle, |_, window, cx| window.click(id, cx)).unwrap();
    }

    /// Something drawn from what's still loading, such as an image, can't be clicked until it's there.
    fn click(&mut self, id: impl Into<gpui_kit::SharedString>) {
        let id = gpui_kit::ElementId::from(id.into());
        self.wait_for(&id);
        self.cx.update_window(self.handle, |_, window, cx| window.click(id, cx)).unwrap();
    }

    fn wait_for(&mut self, id: &gpui_kit::ElementId) {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            self.frame();
            let found = id.clone();
            if self.cx.update_window(self.handle, |_, window, _| window.try_find(found).is_some()).unwrap() {
                return;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {id:?} to be drawn");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// As Cmd-V does, then waits for where it was pasted to have one more file: the new-workspace dialog while it's
    /// open, or else the composer showing.
    fn paste(&mut self, item: ClipboardItem) {
        let dialog = self.cx.update_window(self.handle, |_, window, cx| window.has_active_dialog(cx)).unwrap();
        let view = self.view.clone();
        let had = self.cx.update(|cx| attached(view.read(cx), dialog, cx));
        self.cx.update(|cx| cx.write_to_clipboard(item));
        self.frame();
        self.cx.update_window(self.handle, |_, window, cx| window.press("secondary-v", cx)).unwrap();
        self.wait("the paste to be taken", |view, cx| attached(view, dialog, cx) > had);
    }

    /// As copying a file in Finder and pasting it does.
    fn paste_file(&mut self, name: &str, text: &str) {
        let path = self.dir.join(name);
        std::fs::write(&path, text).unwrap();
        let copied = ExternalPaths(vec![path].into());
        self.paste(ClipboardItem::from(ClipboardEntry::ExternalPaths(copied)));
    }

    /// Clicks the line with this label, then moves away from it, so the picture shows it open rather than hovered. A
    /// line that's clicked stays where it is in the window, with what it opened to running on under it, so the chat
    /// then goes back to its end, where every screen's chat starts, and the picture shows what the line opened to.
    fn open(&mut self, label: &'static str) {
        let id = self.find(Target::Label(label));
        self.cx.update_window(self.handle, |_, window, cx| window.click(id, cx)).unwrap();
        self.update_pane(|pane, cx| {
            if let Some(chat) = pane.transcript() {
                chat.update(cx, |chat, cx| {
                    chat.list().go_to_latest();
                    cx.notify();
                });
            }
        });
        self.move_away();
    }

    fn move_away(&mut self) {
        self.cx.update_window(self.handle, |_, window, cx| window.hover(AWAY, cx)).unwrap();
    }

    /// The ID of what's drawn now as `target`. One found by its label is the first drawn with it.
    fn find(&mut self, target: Target) -> gpui_kit::ElementId {
        self.frame();
        match target {
            Target::Id(id) => gpui_kit::SharedString::from(id).into(),
            Target::Label(label) => {
                let found = self.cx.update_window(self.handle, |_, window, _| first_labelled(window, label)).unwrap();
                found.unwrap_or_else(|| panic!("nothing is labelled {label}")).path().last().unwrap().clone()
            }
        }
    }

    fn update_pane(&mut self, change: impl FnOnce(&mut WorkspacePane, &mut gpui_kit::Context<WorkspacePane>)) {
        let view = self.view.clone();
        self.cx.update(|cx| {
            let pane = selected_pane(view.read(cx)).unwrap();
            pane.update(cx, change);
        });
    }

    /// Draws until [`SAME_IN_A_ROW`] pictures match, or the time's up.
    fn settle(&mut self) -> Picture {
        let deadline = Instant::now() + TIMEOUT;
        let (mut last, mut same) = (self.picture(), 1);
        while same < SAME_IN_A_ROW && Instant::now() < deadline {
            std::thread::sleep(BETWEEN_PICTURES);
            let next = self.picture();
            same = if next == last { same + 1 } else { 1 };
            last = next;
        }
        Picture { image: last, settled: same >= SAME_IN_A_ROW }
    }

    fn picture(&mut self) -> RgbaImage {
        self.frame();
        self.cx.capture_screenshot(self.handle).unwrap()
    }

    fn wait(&mut self, what: &str, mut done: impl FnMut(&AppView, &App) -> bool) {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            self.frame();
            let view = self.view.clone();
            if self.cx.update(|cx| done(view.read(cx), cx)) {
                return;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Rests the pointer on `on`, and waits until the window draws `shows`, such as its tooltip.
    fn hover(&mut self, on: Target, shows: &'static str) {
        let on = self.find(on);
        self.cx.update_window(self.handle, |_, window, cx| window.hover(on, cx)).unwrap();
        // The app's clock only moves when told to, and a tooltip waits for it.
        self.cx.run_until_parked();
        self.cx.advance_clock(LONGER_THAN_A_TOOLTIP_WAITS);
        let deadline = Instant::now() + TIMEOUT;
        loop {
            self.frame();
            if self.cx.update_window(self.handle, |_, window, _| window.try_find(shows).is_some()).unwrap() {
                return;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {shows}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn frame(&mut self) {
        self.cx.run_until_parked();
        self.cx.update_window(self.handle, |_, window, cx| window.render_frame(cx)).unwrap();
        self.cx.run_until_parked();
        self.note_where_the_chat_is();
    }

    /// Before the hub goes, so the app doesn't see it go and try to reconnect.
    fn close(mut self) {
        self.cx.update_window(self.handle, |_, window, _| window.remove_window()).unwrap();
        self.cx.run_until_parked();
    }
}

/// How many files are waiting in the new-workspace dialog, or with `dialog` false, in the composer showing.
fn attached(view: &AppView, dialog: bool, cx: &App) -> usize {
    if dialog {
        return view.new_workspace.as_ref().map_or(0, |dialog| dialog.read(cx).attached_names().len());
    }
    selected_pane(view).map_or(0, |pane| pane.read(cx).attached_names().len())
}

fn selected_pane(view: &AppView) -> Option<Entity<WorkspacePane>> {
    view.selected.as_ref().and_then(|id| view.panes.get(id).cloned())
}
