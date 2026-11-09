//! The real app in a real window, opened on a given view, for looking at it by eye or taking a screenshot.
//! Everything else in the app works normally. Through `cargo run` it uses this checkout's own hub, or
//! `ROUNDHOUSE_HOME` if it's set.
//!
//!     cargo run -p desktop --example look -- some-workspace
//!     cargo run -p desktop --example look -- some-workspace diff "Rename this"
//!     cargo run -p desktop --example look -- some-workspace click:mode-picker
//!
//! Selects the named workspace. With `diff`, opens its diff, and a further argument adds a comment on the
//! first changed line. `--note TEXT` anywhere says why it's open, as it does for the app.

#![expect(clippy::unwrap_used, clippy::expect_used, reason = "a test program, so failing loudly is what it should do")]

use std::time::Duration;

use desktop::app::{AppView, Connection, HubSource};
use desktop::checkout::Checkout;
use desktop::workspace_pane::WorkspacePane;
use gpui_kit::SharedString;
use gpui_kit::test::TestWindowExt as _;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let checkout = Checkout::of_this_build(desktop::checkout::take_note(&mut args));
    gpui_kit::application().with_assets(desktop::Assets).run(move |cx| {
        gpui_kit::init(cx);
        cx.activate(true);
        let (handle, view) = desktop::open_main_window(HubSource::default_local().expect("find the hub"), checkout, cx)
            .expect("open a window");
        let args = args.clone();
        cx.spawn(async move |cx| {
            let wanted = args.first().cloned();
            loop {
                cx.background_executor().timer(Duration::from_millis(100)).await;
                let ready = view.update(cx, |v: &mut AppView, cx| {
                    if !matches!(v.connection, Connection::Ready(_)) {
                        return false;
                    }
                    if let Some(name) = &wanted
                        && let Some(id) = v.workspaces.iter().find(|w| &w.name == name).map(|w| w.id.clone())
                        && v.selected.as_ref() != Some(&id)
                    {
                        v.select(id, cx);
                    }
                    v.selected.as_ref().is_some_and(|id| v.panes.contains_key(id))
                });
                if ready {
                    break;
                }
            }
            // Opens a menu or presses a button, by its ID, for looking at what that shows.
            if let Some(target) = args.get(1).and_then(|arg| arg.strip_prefix("click:")) {
                let target = SharedString::from(target.to_string());
                let _ = handle.update(cx, |_, window, cx| window.click(target, cx));
                return;
            }
            if args.get(1).map(String::as_str) != Some("diff") {
                return;
            }
            let (pane, client) = view
                .read_with(cx, |v: &AppView, _| (v.panes[v.selected.as_ref().unwrap()].clone(), v.client().unwrap()));
            if let Some(text) = args.get(2) {
                let id = pane.read_with(cx, |p, _| p.workspace.id.clone());
                let diff = client.workspace_diff(&id).unwrap();
                let parsed = first_added_line(&diff.patch);
                if let Some((path, line)) = parsed {
                    client
                        .comment_add(
                            &id,
                            &path,
                            protocol::model::DiffSide::New,
                            line,
                            text,
                            Some(client_core::new_request_id()),
                        )
                        .unwrap();
                }
            }
            pane.update(cx, WorkspacePane::toggle_changes);
        })
        .detach();
    });
}

fn first_added_line(diff: &str) -> Option<(String, u32)> {
    let mut path = None;
    let mut line = 0u32;
    for text in diff.lines() {
        if let Some(p) = text.strip_prefix("+++ b/") {
            path = Some(p.to_string());
        } else if let Some(rest) = text.strip_prefix("@@ ") {
            let new = rest.split(' ').find(|part| part.starts_with('+'))?;
            line = new[1..].split(',').next()?.parse().ok()?;
        } else if text.starts_with('+') && !text.starts_with("+++") {
            return path.map(|p| (p, line));
        } else if !text.starts_with('-') {
            line += 1;
        }
    }
    None
}
