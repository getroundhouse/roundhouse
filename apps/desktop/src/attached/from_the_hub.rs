//! Reading back the files the hub kept with a draft, for a chat's composer or the new-workspace dialog to put back, off
//! the main thread, as `client_core::ReadBack` says.

use client_core::{Client, FilesFor, ReadBack};
use gpui_kit::{AppContext as _, Context};
use protocol::attachment::Attachment;

/// Reads back `files`, which the hub kept for `kept_for`, on a thread of the app's, and hands what came back to `then`.
pub(crate) fn read_back<T: 'static>(
    client: Client,
    kept_for: FilesFor,
    files: Vec<Attachment>,
    then: impl FnOnce(&mut T, ReadBack, &mut Context<T>) + 'static,
    cx: &mut Context<T>,
) {
    cx.spawn(async move |this, cx| {
        let read = cx.background_spawn(async move { client.read_back(&kept_for, files) }).await;
        let _ = this.update(cx, |t, cx| then(t, read, cx));
    })
    .detach();
}
