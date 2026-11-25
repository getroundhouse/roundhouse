//! Quitting once what this computer keeps for the app is saved. Saving it is `client_core::this_device`'s.

use gpui_kit::{App, Global};

/// Has the app wait, as it quits, for what a global was asked to save, which `saved` hears of, so a change made just
/// before is saved. Quitting gives this a moment.
pub(crate) fn saved_before_quitting<G: Global>(saved: fn(&G) -> flume::Receiver<()>, cx: &App) {
    cx.on_app_quit(move |cx| {
        let all_saved = cx.try_global::<G>().map(saved);
        async move {
            if let Some(all_saved) = all_saved {
                let _ = all_saved.recv_async().await;
            }
        }
    })
    .detach();
}
