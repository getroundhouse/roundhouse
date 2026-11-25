//! What the app holds of the recordings it keeps: where they are, how much room they take, and the ones being written.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use dictation_ui::{About, Keeping, Place};
use gpui_kit::{App, AppContext as _, Context, Global, Subscription, Task};
use hint_words::HintWord;
use kept_recordings::{Field, Kept, VERSION};

use super::sent::{Unsent, sent_from};
use super::writer::{self, Source};

/// The app's recordings.
pub(super) struct Recordings {
    pub(super) folder: PathBuf,
    /// The build of the app, which each recording says.
    build: &'static str,
    /// How much room they take, as of the last time the app changed that, so nothing reads the disk to draw it.
    pub(super) room_bytes: u64,
    being_written: Vec<BeingWritten>,
    pub(super) unsent: Unsent,
    /// Deletes them, until it's done.
    pub(super) deleting: Option<Task<()>>,
    /// Finds how much room they take, as the app starts.
    _finding: Task<()>,
    _following_the_choice: Subscription,
}

impl Global for Recordings {}

struct BeingWritten {
    id: String,
    dropped: Arc<AtomicBool>,
    /// Closes once its thread has ended.
    ended: flume::Receiver<()>,
}

/// Has the app keep recordings in `data_dir`, while the person has chosen to. Once, as the app starts.
pub(in crate::dictating) fn start(data_dir: &Path, build: &'static str, cx: &mut App) {
    if cx.has_global::<Recordings>() {
        return;
    }
    let folder = data_dir.join("recordings");
    let found = folder.clone();
    let finding = cx.spawn(async move |cx| {
        let room_bytes = cx.background_spawn(async move { models::bytes_on_disk(&found) }).await;
        cx.update(|cx| cx.global_mut::<Recordings>().room_bytes += room_bytes);
    });
    let following = crate::settings::observe_in_app(cx, follow_the_choice);
    cx.set_global(Recordings {
        unsent: Unsent::read(&folder),
        folder,
        build,
        room_bytes: 0,
        being_written: Vec::new(),
        deleting: None,
        _finding: finding,
        _following_the_choice: following,
    });
    crate::saving::saved_before_quitting(Recordings::written, cx);
}

/// Whether the person has chosen to keep what they dictate.
pub(in crate::dictating) fn keeps(cx: &App) -> bool {
    cx.has_global::<Recordings>() && crate::settings::get(cx).keep_recordings
}

/// Turns Keep recordings on, or off, as the person can in Settings.
#[cfg(any(test, feature = "testing"))]
pub fn keep_recordings(on: bool, cx: &mut App) {
    crate::settings::change(cx, |settings| settings.keep_recordings = on);
}

/// Has recordings take `bytes`, as they would if that much had been kept, for a screen to show.
#[cfg(any(test, feature = "testing"))]
pub fn recordings_take(bytes: u64, cx: &mut App) {
    cx.global_mut::<Recordings>().room_bytes = bytes;
}

/// Drops what's being written once the person turns it off, and keeps nothing sent from then on.
fn follow_the_choice(cx: &mut App) {
    if !keeps(cx) {
        let recordings = cx.global_mut::<Recordings>();
        recordings.drop_whats_being_written();
        recordings.unsent.forget();
    }
}

/// Keeps a recording, heard with the model's `revision`, that has begun to dictate.
pub(in crate::dictating) fn keep(keeping: Keeping, revision: &str, cx: &mut App) {
    // Turned off since it began to listen.
    if !keeps(cx) {
        return;
    }
    let kept = starting(&keeping, cx.global::<Recordings>().build, revision);
    let Keeping { id, place, about, taken, said } = keeping;
    let from = sent_from(place, about.within);
    let dropped = Arc::new(AtomicBool::new(false));
    let source = Source { audio: taken.audio, said, dropped: dropped.clone() };
    let folder = cx.global::<Recordings>().folder.clone();
    let (done, written) = flume::bounded(1);
    let (ends, ended) = flume::bounded::<()>(0);
    let recording_id = id.clone();
    let write = move || {
        match writer::write(&folder, kept, &source) {
            Ok(room_bytes) => drop(done.send(room_bytes)),
            Err(error) => {
                tracing::warn!(name: "dictation.not_kept", %recording_id, %error, "Couldn't keep a recording");
            }
        }
        drop(ends);
    };
    if let Err(error) = std::thread::Builder::new().name("recording-kept".to_string()).spawn(write) {
        tracing::warn!(name: "dictation.not_kept", recording_id = id, %error, "Couldn't keep a recording");
        return;
    }
    let recordings = cx.global_mut::<Recordings>();
    recordings.being_written.push(BeingWritten { id: id.clone(), dropped, ended });
    recordings.unsent.recorded(from);
    cx.spawn(async move |cx| {
        let room_bytes = written.recv_async().await.unwrap_or(0);
        cx.update(|cx| {
            let recordings = cx.global_mut::<Recordings>();
            recordings.room_bytes += room_bytes;
            recordings.being_written.retain(|writing| writing.id != id);
        });
    })
    .detach();
}

/// Has a view hear whenever what's kept changes, such as how much room it takes, while `showing` says it shows it.
pub(crate) fn observe<V: 'static>(cx: &mut Context<V>, showing: impl Fn(&V) -> bool + 'static) -> Subscription {
    cx.observe_global::<Recordings>(move |view, cx| {
        if showing(view) {
            cx.notify();
        }
    })
}

impl Recordings {
    /// Closes once everything being written is.
    fn written(&self) -> flume::Receiver<()> {
        let mut ending: Vec<_> = self.being_written.iter().map(|writing| writing.ended.clone()).collect();
        ending.push(self.unsent.saved());
        let (ends, ended) = flume::bounded(0);
        std::thread::spawn(move || {
            for writing in ending {
                let _ = writing.recv();
            }
            drop(ends);
        });
        ended
    }

    pub(super) fn drop_whats_being_written(&self) {
        for writing in &self.being_written {
            writing.dropped.store(true, Ordering::Release);
        }
    }
}

/// What's known of a recording as it begins.
fn starting(keeping: &Keeping, build: &str, revision: &str) -> Kept {
    let About { hints, picked_from, within } = &keeping.about;
    let hint = |hint: dictation_ui::Hint| HintWord { text: hint.text, spelled: hint.spelled };
    Kept {
        version: VERSION,
        id: keeping.id.clone(),
        started_at_ms: now_ms(),
        build: build.to_string(),
        model: revision.to_string(),
        language: keeping.taken.language.code().to_string(),
        hint_boost: dictation::nemotron::HINT_BOOST,
        hints: hints.kept().map(hint).collect(),
        picked_from: Some(picked_from.clone()),
        field: field(keeping.place),
        workspace: within.workspace.clone(),
        chat: within.chat.clone(),
        said_out_loud: within.said_out_loud,
        device: None,
        said: Vec::new(),
        stopped_ms: None,
        ended: None,
    }
}

pub(super) fn field(place: Place) -> Field {
    match place {
        Place::Composer => Field::Composer,
        Place::NewWorkspace => Field::NewWorkspace,
        Place::DiffComment => Field::DiffComment,
        Place::Answer => Field::Answer,
        Place::ProposalNote => Field::ProposalNote,
    }
}

/// Now, in milliseconds since 1970.
pub(super) fn now_ms() -> u64 {
    let since = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
}
