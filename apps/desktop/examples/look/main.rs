//! The real app in a real window, opened on a given view, for looking at it by eye or taking a screenshot.
//! Everything else in the app works normally. Through `cargo run` it uses this checkout's own hub, or
//! `ROUNDHOUSE_HOME` if it's set.
//!
//!     cargo run -p desktop --example look -- some-workspace
//!     cargo run -p desktop --example look -- some-workspace diff "Rename this"
//!     cargo run -p desktop --example look -- some-workspace click:model-picker
//!     cargo run -p desktop --example look -- some-workspace click:file-shot.png click:image-difference
//!
//!     cargo run -p desktop --example look -- some-workspace scroll:40
//!     cargo run -p desktop --example look -- some-workspace drag:3
//!     cargo run -p desktop --example look -- some-workspace jump:3
//!
//! Selects the named workspace, whose sidebar of changes is open, as it is from the start. Each `click:ID` presses
//! what has that ID, in turn. With `diff`, a further
//! argument adds a comment on the first changed line. `scroll:PX` turns the wheel over the transcript every 16 ms, up
//! to its start and back down, and `drag:PX` drags the scrollbar's thumb up, each printing where the list and its
//! scrollbar are after every step, then quits. `probes.rs` says why.
//! `jump:K` rests the pointer on the outline and clicks its Kth prompt, printing where the list is before and after.
//! `--note TEXT` anywhere says why it's open, as it does for the app.

#![expect(clippy::unwrap_used, clippy::expect_used, reason = "a test program, so failing loudly is what it should do")]

use std::time::Duration;

use desktop::app::{AppView, Connection, HubSource};
use desktop::checkout::Checkout;
use gpui_kit::SharedString;
use gpui_kit::test::TestWindowExt as _;

mod probes;

/// How many tenths of a second to wait for something to click to be drawn: long enough for a hub to read a file.
const CLICK_WAITS: usize = 100;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let checkout = Checkout::of_this_build(desktop::checkout::take_note(&mut args));
    gpui_kit::application().with_assets(desktop::Assets).run(move |cx| {
        gpui_kit::init(cx);
        cx.activate(true);
        // In its usual place, so one picture is the same size as the next.
        desktop::why_quit::say(checkout.as_ref(), cx);
        let hub = HubSource::default_local().expect("find the hub");
        let (handle, view) =
            desktop::open_main_window(hub, checkout, desktop::Opens::InItsUsualPlace, cx).expect("open a window");
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
            if let Some(probe) = args.get(1).and_then(|arg| probes::Probe::parse(arg)) {
                probe.run(&handle, &view, cx).await;
                cx.update(|cx| desktop::why_quit::done("its probe finished", cx));
                return;
            }
            // Opens a menu or presses a button, by its ID, for looking at what that shows, then each one after it in
            // turn, once it's drawn, since what a click opens can take a moment to load.
            let clicks: Vec<SharedString> = args
                .iter()
                .skip(1)
                .filter_map(|arg| arg.strip_prefix("click:"))
                .map(|target| SharedString::from(target.to_string()))
                .collect();
            if !clicks.is_empty() {
                click_each(handle, clicks, cx).await;
                return;
            }
            if args.get(1).map(String::as_str) != Some("diff") {
                return;
            }
            if let Some(text) = args.get(2) {
                comment_on_the_first_change(&view, text, cx);
            }
        })
        .detach();
    });
}

/// Presses each of `targets` in turn, once it's drawn, or once it's been waited for long enough.
async fn click_each(handle: gpui_kit::AnyWindowHandle, targets: Vec<SharedString>, cx: &mut gpui_kit::AsyncApp) {
    for target in targets {
        for _ in 0..CLICK_WAITS {
            let drawn = handle.update(cx, |_, window, _| window.try_find(target.clone()).is_some());
            if drawn.unwrap_or(false) {
                break;
            }
            cx.background_executor().timer(Duration::from_millis(100)).await;
        }
        let _ = handle.update(cx, |_, window, cx| window.click(target, cx));
    }
}

/// Adds a comment on the first line the workspace's diff adds.
fn comment_on_the_first_change(view: &gpui_kit::Entity<AppView>, text: &str, cx: &mut gpui_kit::AsyncApp) {
    let (pane, client) =
        view.read_with(cx, |v: &AppView, _| (v.panes[v.selected.as_ref().unwrap()].clone(), v.client().unwrap()));
    let id = pane.read_with(cx, |p, _| p.workspace.id.clone());
    let diff = client.workspace_diff(&id).unwrap();
    if let Some((path, line)) = first_added_line(&diff.patch) {
        let request = Some(client_core::new_request_id());
        client.comment_add(&id, &path, protocol::model::DiffSide::New, line, text, request).unwrap();
    }
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
