//! Quitting once the commands started on the hub have finished, so a prompt sent just before quitting still reaches
//! it. Quitting holds the main thread, so the wait doesn't need that thread. It's the app's rather than the view's,
//! since closing the window quits, and takes the view with it.

use client_core::{Client, Commands};
use gpui_kit::{App, Global};

/// The current connection's commands. Those on a connection that has gone failed when it went.
struct Started(Commands);

impl Global for Started {}

/// Has quitting wait for the commands started on `client`, rather than on the connection before it.
pub(super) fn wait_for(client: &Client, cx: &mut App) {
    if !cx.has_global::<Started>() {
        cx.on_app_quit(|cx| {
            let finished = cx.try_global::<Started>().map(|started| started.0.finished());
            async move {
                if let Some(finished) = finished {
                    let _ = finished.recv_async().await;
                }
            }
        })
        .detach();
    }
    cx.set_global(Started(client.commands()));
}
