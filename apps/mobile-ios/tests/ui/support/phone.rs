//! The app in a phone-sized window, driven the way someone would: tapping, pressing long, typing, and reading what it
//! shows.

use std::sync::Arc;
use std::time::Duration;

use client_core::link::Waits;
use client_core::this_device::Preferences;
use gpui_kit::test::{TestQueryExt as _, TestWindowExt as _};
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, ElementId, Entity, GestureTuning, InputEvent as _, Pixels, Point,
    SharedString, TestAppContext, TouchEvent, TouchId, TouchPhase, WindowBounds, WindowOptions, point, px, size,
};
use mobile::{MemoryVault, MobileApp, Vault as _};
use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::model::{Seq, Workspace};
use protocol::pairing::PairingLink;

use super::Stack;

pub(crate) const TIMEOUT: Duration = Duration::from_secs(20);
/// How far the app's own clock moves on each look while a test waits on a real hub.
const STEP: Duration = Duration::from_millis(10);
/// How long the phone waits before trying a hub again, first and longest, in place of the second and the thirty
/// seconds every client waits.
const RETRY_WAITS: Waits = Waits::retrying(Duration::from_millis(20), Duration::from_millis(100));

/// The ID of an element named `name`, as the question panel names its own.
pub(crate) fn id(name: &str) -> SharedString {
    SharedString::from(name.to_string())
}

pub(crate) struct Phone<'a> {
    pub(crate) cx: &'a mut TestAppContext,
    pub(crate) handle: AnyWindowHandle,
    pub(crate) app: Entity<MobileApp>,
    pub(crate) vault: Arc<MemoryVault>,
    /// Pairing links, as the Camera app opens them.
    pub(crate) links: flume::Sender<String>,
    /// The approval [`Phone::next_open_approval`] last returned.
    seen_approval: Seq,
}

/// What a phone already paired with the stack's hub keeps of it.
pub(crate) fn paired_vault(stack: &Stack) -> Arc<MemoryVault> {
    let vault = Arc::new(MemoryVault::default());
    let link = PairingLink::parse(&stack.pairing_link()).unwrap();
    vault.save(&client_core::pair(&link, "phone").unwrap()).unwrap();
    vault
}

impl<'a> Phone<'a> {
    /// The app on a phone that has never paired.
    pub(crate) fn unpaired(cx: &'a mut TestAppContext) -> Phone<'a> {
        Phone::open(cx, Arc::new(MemoryVault::default()), Preferences::in_memory())
    }

    /// The app on a phone already paired with the stack's hub, and never left anywhere.
    pub(crate) fn paired(cx: &'a mut TestAppContext, stack: &Stack) -> Phone<'a> {
        Phone::open(cx, paired_vault(stack), Preferences::in_memory())
    }

    /// The app as it opens on a phone that keeps the hub it's paired with in `vault`, and where it was left in
    /// `preferences`.
    pub(crate) fn open(cx: &'a mut TestAppContext, vault: Arc<MemoryVault>, preferences: Preferences) -> Phone<'a> {
        // A real hub answers on real threads, which the scheduler otherwise reports as nondeterminism.
        cx.executor().allow_parking();
        cx.update(gpui_kit::init);
        // Tried again by the system's clock, which a test can't move, so it would sit through the phone's waits.
        cx.update(|cx| mobile::the_retry_waits_are(RETRY_WAITS, cx));
        let (links, opened) = flume::unbounded();
        // An iPhone's screen, in points.
        let screen = Bounds::new(point(px(0.), px(0.)), size(px(402.), px(874.)));
        let options = WindowOptions { window_bounds: Some(WindowBounds::Windowed(screen)), ..Default::default() };
        let (handle, app) =
            cx.update(|cx| mobile::open_window(vault.clone(), preferences, opened, options, cx)).unwrap();
        // In front, as an app the person just opened is.
        cx.update_window(handle, |_, window, _| window.activate_window()).unwrap();
        Phone { cx, handle, app, vault, links, seen_approval: Seq::default() }
    }

    pub(crate) fn is_connected(&self) -> bool {
        self.cx.read_entity(&self.app, MobileApp::is_connected)
    }

    /// Whether the last frame drew it.
    pub(crate) fn has(&mut self, id: impl Into<ElementId>) -> bool {
        let id = id.into();
        self.frame();
        self.cx.update_window(self.handle, |_, window, _| window.try_find(id).is_some()).unwrap()
    }

    /// Runs the app until `done`, on real time, because a real hub is on the other end of the socket. The app's own
    /// timers run on the test's clock, so that moves on with it. The wait before connecting again is client-core's, on
    /// the system's clock, which [`RETRY_WAITS`] shortens. One that times out says how the link to the hub stood, such
    /// as why its last try to connect failed.
    #[track_caller]
    pub(crate) fn wait(&mut self, what: &str, mut done: impl FnMut(&mut Self) -> bool) {
        let look = |phone: &mut Self| {
            phone.frame();
            let done = done(phone);
            phone.cx.executor().advance_clock(STEP);
            done
        };
        let said = |phone: &mut Self| {
            format!("the link to the hub was {}", phone.cx.read_entity(&phone.app, MobileApp::link_state))
        };
        waiting::thread::until_or_say(TIMEOUT, what, self, look, said);
    }

    pub(crate) fn frame(&mut self) {
        self.cx.run_until_parked();
        let _ = self.cx.update_window(self.handle, |_, window, cx| window.render_frame(cx));
        self.cx.run_until_parked();
    }

    pub(crate) fn tap(&mut self, id: impl Into<ElementId>) {
        let id = id.into();
        self.frame();
        self.cx.update_window(self.handle, |_, window, cx| window.click(id, cx)).unwrap();
        self.frame();
    }

    /// Makes another app the one in front, or the phone locked.
    pub(crate) fn send_to_back(&mut self) {
        gpui_kit::VisualTestContext::from_window(self.handle, self.cx).deactivate_window();
        self.frame();
    }

    /// Brings the app to the front again.
    pub(crate) fn bring_to_front(&mut self) {
        self.cx.update_window(self.handle, |_, window, _| window.activate_window()).unwrap();
        self.frame();
    }

    /// Where the last frame drew `id`.
    pub(crate) fn bounds(&mut self, id: impl Into<ElementId>) -> Bounds<Pixels> {
        let id = id.into();
        self.frame();
        self.cx.update_window(self.handle, |_, window, _| window.find(id).bounds()).unwrap()
    }

    /// Holds a finger on `id` until it's a long press, then lets go, as opens its menu.
    pub(crate) fn press_long(&mut self, id: impl Into<ElementId>) {
        let at = self.bounds(id).center();
        self.touch(TouchPhase::Started, at);
        self.cx.executor().advance_clock(GestureTuning::default().long_press_duration + Duration::from_millis(1));
        self.frame();
        self.touch(TouchPhase::Ended, at);
        self.frame();
    }

    fn touch(&mut self, phase: TouchPhase, at: Point<Pixels>) {
        let touch = TouchEvent { id: TouchId(1), phase, position: at, predicted_position: None, force: None };
        self.cx
            .update_window(self.handle, |_, window, cx| window.dispatch_event(touch.to_platform_input(), cx))
            .unwrap();
    }

    /// Taps the row labelled `label` in the menu that's open, rather than anything else with that label.
    pub(crate) fn choose(&mut self, label: &str) {
        self.frame();
        let menu = ElementId::Name("popup-menu".into());
        let (place, row) = self
            .cx
            .update_window(self.handle, |_, window, _| {
                let found = window.find_all_by_label(label).into_iter().find(|found| found.path().contains(&menu));
                let path =
                    found.unwrap_or_else(|| panic!("the open menu has nothing labelled {label}")).path().to_vec();
                // By its place in the menu too, since two rows can say the same, such as two untitled chats.
                (path[path.len() - 2].clone(), path[path.len() - 1].clone())
            })
            .unwrap();
        self.cx.update_window(self.handle, |_, window, cx| window.within(menu).within(place).click(row, cx)).unwrap();
        self.frame();
    }

    /// Types where the cursor is, as the on-screen keyboard does.
    pub(crate) fn type_text(&mut self, text: &str) {
        self.cx.update_window(self.handle, |_, window, cx| window.input(text, cx)).unwrap();
        self.frame();
    }

    /// Taps the field, then types into it.
    pub(crate) fn type_into(&mut self, field: &str, text: &str) {
        self.tap(id(field));
        self.type_text(text);
    }

    /// Taps the workspace in the list, once it's there and the phone is connected, and waits for its chat. The list
    /// draws the records client-core holds, which can be there before the connection is ready, or after it went, and a
    /// tap opens nothing without a connection.
    pub(crate) fn open_chat(&mut self, workspace: &Workspace) {
        let row = ElementId::Name(format!("workspace-{}", workspace.id).into());
        self.wait("the workspace", |phone| phone.has(row.clone()) && phone.is_connected());
        self.tap(row);
        self.wait("its chat", |phone| phone.has("composer") && phone.transcript_shown());
    }

    /// Types into the chat's composer and sends it.
    pub(crate) fn send(&mut self, text: &str) {
        self.type_into("composer", text);
        self.tap("send");
    }

    /// The newest open approval, once the transcript has it, and it's newer than the last this returned. Its status
    /// and its events come apart, so either can arrive first.
    pub(crate) fn next_open_approval(&mut self) -> Seq {
        let seen = self.seen_approval;
        let newest = |phone: &Self| {
            let transcript = phone.cx.read_entity(&phone.app, MobileApp::transcript)?;
            let approvals = phone.cx.read_entity(&transcript, |t, _| t.open_approvals());
            approvals.iter().map(|event| event.seq).filter(|seq| *seq > seen).max()
        };
        self.wait("the transcript to show the approval", |phone| newest(phone).is_some());
        self.seen_approval = newest(self).unwrap();
        self.seen_approval
    }

    pub(crate) fn chat_title(&self) -> Option<String> {
        self.cx.read_entity(&self.app, MobileApp::chat_title)
    }

    pub(crate) fn transcript_shown(&self) -> bool {
        self.cx.read_entity(&self.app, MobileApp::transcript).is_some()
    }

    /// The chat the open chat's screen shows.
    pub(crate) fn chat_shown(&self) -> Option<protocol::model::SessionId> {
        self.cx.read_entity(&self.app, MobileApp::chat_shown)
    }

    /// The phone's own record of the chat shown, which can be behind the hub's.
    pub(crate) fn shown_record(&self) -> Option<protocol::model::Session> {
        self.cx.read_entity(&self.app, MobileApp::shown_record)
    }

    /// What's typed in the open chat's composer.
    pub(crate) fn typed(&self) -> String {
        self.cx.read_entity(&self.app, MobileApp::typed).unwrap_or_default()
    }

    /// Whether a subagent's work is shown over the chat.
    pub(crate) fn subagent_shown(&self) -> bool {
        self.cx.read_entity(&self.app, MobileApp::subagent_transcript).is_some()
    }

    /// The first element the last frame drew whose ID starts with `part`, for one a test can't know the whole ID of,
    /// such as the Open on a subagent's line, which ends with its call's.
    pub(crate) fn found_starting(&mut self, part: &str) -> Option<ElementId> {
        self.frame();
        self.cx
            .update_window(self.handle, |_, window, _| {
                let mut ids = window.elements().into_iter().filter_map(|element| element.path().last().cloned());
                ids.find(|id| matches!(id, ElementId::Name(name) if name.starts_with(part)))
            })
            .unwrap()
    }

    /// Whether the first thing labelled `label`, such as a call's line, is open, if it's something that opens.
    pub(crate) fn expanded(&mut self, label: &str) -> Option<bool> {
        self.frame();
        self.cx
            .update_window(self.handle, |_, window, _| {
                window.find_all_by_label(label).first().and_then(gpui_kit::test::ElementSnapshot::expanded)
            })
            .unwrap()
    }

    /// Taps the first thing labelled `label`, such as a call's line, which is labelled by what the call did.
    pub(crate) fn tap_labelled(&mut self, label: &str) {
        self.frame();
        let found = self
            .cx
            .update_window(self.handle, |_, window, _| {
                window.find_all_by_label(label).first().and_then(|found| found.path().last().cloned())
            })
            .unwrap();
        self.tap(found.unwrap_or_else(|| panic!("nothing is labelled {label}")));
    }

    /// The text on the clipboard.
    pub(crate) fn copied(&self) -> Option<String> {
        self.cx.read_from_clipboard().and_then(|copied| copied.text())
    }

    /// The first element the last frame drew whose ID is `name` and a number, such as a code block, which the kit
    /// numbers by where it starts in its text.
    pub(crate) fn found_numbered(&mut self, name: &str) -> Option<ElementId> {
        self.frame();
        self.cx
            .update_window(self.handle, |_, window, _| {
                let mut ids = window.elements().into_iter().filter_map(|element| element.path().last().cloned());
                ids.find(|id| matches!(id, ElementId::NamedInteger(named, _) if named == name))
            })
            .unwrap()
    }

    /// Waits for an element whose ID starts with `part`, then taps it.
    pub(crate) fn tap_starting(&mut self, part: &str) {
        self.wait(&format!("something whose ID starts {part}"), |phone| phone.found_starting(part).is_some());
        let found = self.found_starting(part).unwrap();
        self.tap(found);
    }

    fn events(&self) -> Vec<SessionEvent> {
        let Some(transcript) = self.cx.read_entity(&self.app, MobileApp::transcript) else { return Vec::new() };
        self.cx.read_entity(&transcript, |t, _| (0..t.row_count()).filter_map(|row| t.event_at(row)).collect())
    }

    pub(crate) fn assistant_text(&self) -> String {
        let mut text = String::new();
        for event in self.events() {
            if let EventBody::Assistant { blocks, .. } = &event.body {
                for block in blocks {
                    if let ContentBlock::Text { text: t, .. } = block {
                        text.push_str(t);
                    }
                }
            }
        }
        text
    }

    /// What each prompt the chat's log holds says, in order, which a prompt sent while the agent works is once it's
    /// steered it rather than queued.
    pub(crate) fn prompts(&self) -> Vec<String> {
        let prompt = |event: SessionEvent| match event.body {
            EventBody::UserPrompt { prompt, .. } => Some(prompt.text),
            _ => None,
        };
        self.events().into_iter().filter_map(prompt).collect()
    }

    /// What the agent was told its calls came to, such as the answer to its question.
    pub(crate) fn told_to_agent(&self) -> String {
        let mut told = String::new();
        for event in self.events() {
            if let EventBody::AgentUser { blocks, .. } = &event.body {
                for block in blocks {
                    if let ContentBlock::CallResult { output, .. } = block {
                        told.push_str(&client_core::calls::typed::output(output));
                    }
                }
            }
        }
        told
    }
}
