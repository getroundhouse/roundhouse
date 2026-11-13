//! The steps that find text in a chat, and finding in a chat both ways, as it's drawn and as it's counted.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::PlatformTextSystem;
use gpui_kit::test::TestWindowExt as _;

use super::{Driver, open, selected_pane};
use crate::gallery::screens::FOCUS;
use crate::gallery::steps::Step;
use crate::gallery::world::World;

impl Driver {
    pub(super) fn find_in_chat(&mut self, query: &str) {
        let find = Box::new(desktop::FindInChat);
        self.cx.update_window(self.handle, |_, window, cx| window.dispatch_action(find, cx)).unwrap();
        self.frame();
        self.cx.update_window(self.handle, |_, window, cx| window.input(query, cx)).unwrap();
        // Longer than the bar waits for typing to pause.
        self.cx.advance_clock(Duration::from_secs(1));
        self.wait("every match to be counted and the first shown", |view, cx| {
            let transcript = selected_pane(view).and_then(|pane| pane.read(cx).transcript());
            let says = transcript.and_then(|t| t.read(cx).find_says());
            says.is_some_and(|says| says.contains(" of ") && !says.ends_with('…'))
        });
    }
}

/// How many matches of a query each text of a chat has, by text.
pub(crate) struct FoundBothWays {
    pub(crate) query: String,
    /// As the chat draws them.
    pub(crate) drawn: BTreeMap<String, usize>,
    /// As finding counts them.
    pub(crate) counted: BTreeMap<String, usize>,
}

/// What each text of the chat `world` holds has of each of `queries`, as the chat draws it and as finding counts it,
/// as [`transcript::Transcript::found_both_ways`] says.
pub(crate) fn found_both_ways(
    world: fn() -> World,
    queries: &[&str],
    text_system: &Arc<dyn PlatformTextSystem>,
) -> Vec<FoundBothWays> {
    // What a text has of a query is the same however wide the window is.
    let (_dir, mut app) = open(Some(world), false, text_system);
    app.step(&Step::Select(FOCUS));
    let transcript = app.cx.update(|cx| selected_pane(app.view.read(cx)).and_then(|pane| pane.read(cx).transcript()));
    let transcript = transcript.expect("the chat");
    let found = queries
        .iter()
        .map(|query| {
            let (drawn, counted) = app
                .cx
                .update_window(app.handle, |_, window, cx| {
                    transcript.update(cx, |t, cx| t.found_both_ways(query, window, cx))
                })
                .unwrap();
            FoundBothWays { query: (*query).to_string(), drawn, counted }
        })
        .collect();
    app.close();
    found
}

/// What a copy holds of the chat `world` holds with everything selected, once each call in `calls` has been opened, as
/// [`transcript::Transcript::all_as_copied`] says.
pub(crate) fn copied_whole(
    world: fn() -> World,
    calls: &[&'static str],
    text_system: &Arc<dyn PlatformTextSystem>,
) -> String {
    let (_dir, mut app) = open(Some(world), false, text_system);
    app.step(&Step::Select(FOCUS));
    for label in calls {
        app.step(&Step::Open(label));
    }
    let transcript = app.cx.update(|cx| selected_pane(app.view.read(cx)).and_then(|pane| pane.read(cx).transcript()));
    let copied = app.cx.update(|cx| transcript.expect("the chat").read(cx).all_as_copied());
    app.close();
    copied
}
