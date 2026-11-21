//! The model's download, on a thread of its own, and how far it has got, which only what shows it follows.

use std::ops::ControlFlow;
use std::path::PathBuf;

use gpui_kit::{App, Context, Global, Subscription};
use models::{DownloadError, Fetched, ModelFiles, Progress};

/// How much more has to come before progress is told again, so telling doesn't cost more than reading.
const TELL_EVERY_BYTES: u64 = 4 << 20;

/// What a download says.
pub(super) enum Downloaded {
    Progress(Progress),
    /// Every file is here, or one failed. A download that's stopped says nothing more.
    Ended(Result<(), DownloadError>),
}

/// Downloads `files` into `model_dir` from `from`, on a thread of its own, saying how it goes until it ends, or until
/// what it says isn't heard any more, which stops it at its next read, keeping what's come of a file for next time.
pub(super) fn download(
    files: ModelFiles,
    model_dir: PathBuf,
    from: String,
) -> std::io::Result<flume::Receiver<Downloaded>> {
    let (tell, told) = flume::unbounded();
    std::thread::Builder::new().name("dictation-download".to_string()).spawn(move || {
        let mut last_told: Option<u64> = None;
        let mut progress = |now: Progress| {
            if tell.is_disconnected() {
                return ControlFlow::Break(());
            }
            let finished = now.done_bytes == now.total_bytes;
            if last_told.is_none_or(|was| finished || now.done_bytes >= was + TELL_EVERY_BYTES) {
                last_told = Some(now.done_bytes);
                let _ = tell.send(Downloaded::Progress(now));
            }
            ControlFlow::Continue(())
        };
        let ended = match files.download(&model_dir, &from, &mut progress) {
            Ok(Fetched::Done) => Ok(()),
            Ok(Fetched::Stopped) => return,
            Err(e) => Err(e),
        };
        let _ = tell.send(Downloaded::Ended(ended));
    })?;
    Ok(told)
}

/// How much of the model is here while it's being downloaded, once the download has said, and nothing otherwise. It's
/// apart from how dictation stands, so only what shows it, Settings and the composer's microphone, draws again as it
/// changes, and only while it shows it.
pub(super) struct ModelDownload(pub(super) Option<Progress>);

impl Global for ModelDownload {}

/// How much of the model is here, while it's being downloaded.
pub(super) fn progress(cx: &App) -> Option<Progress> {
    cx.global::<ModelDownload>().0
}

/// Has a view drawn again as the download gets further, while `showing` says it shows how far.
pub(crate) fn observe_progress<V: 'static>(
    cx: &mut Context<V>,
    showing: impl Fn(&V, &App) -> bool + 'static,
) -> Subscription {
    cx.observe_global::<ModelDownload>(move |view, cx| {
        if showing(view, cx) {
            cx.notify();
        }
    })
}
