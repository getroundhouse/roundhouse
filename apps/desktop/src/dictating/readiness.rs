//! How dictation stands: off, downloading the model, preparing it, ready, failed, or on a macOS too old for it. It
//! follows the person's choice. Turned on, it downloads the model's files that aren't here, then loads it on the
//! thread that hears what's dictated. Turned off, it stops a download, keeping what's come, and lets go of the thread.
//! Each recording belongs to its field attachment, which ends it when the field goes, so model setup keeps none.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use dictation::model::{self, DownloadError, ModelFiles};
use dictation::nemotron::{self, Nemotron};
use dictation::{AudioIn, Dictation, Engine, EngineError, Recording, Update};
use gpui_kit::{App, AsyncApp, Context, Global, Subscription, Task};

use super::DictationChoice;
use super::downloading::{self, Downloaded, ModelDownload};

/// How long the model stays loaded with nothing dictated, after which the next recording loads it again, which takes
/// under a second once Core ML has compiled it, as `docs/spec/settings.md` says.
const KEEP_FOR: Duration = Duration::from_mins(15);

/// How dictation stands.
pub(super) enum Readiness {
    /// It isn't on: it asks first, or it's off.
    Off,
    /// The model's files are being downloaded, as [`downloading::progress`] says.
    Downloading,
    /// The thread that hears what's dictated is loading the model, which Core ML compiles for the Neural Engine the
    /// first time.
    Preparing(Hearing),
    /// Opted-in fields can be dictated to.
    Ready(Hearing),
    Failed(Failure),
    /// This Mac's macOS is older than the model needs.
    NeedsNewerMacos,
}

/// The thread that hears what's dictated, which only this module reaches.
pub(super) struct Hearing(Dictation);

/// Why dictation couldn't be made ready, by the step that failed.
#[derive(Debug)]
pub(super) enum Failure {
    Download(DownloadError),
    /// A thread dictation needs couldn't start.
    Start(io::Error),
    Prepare(EngineError),
}

/// Where the model's files are kept, where they come from, and how it's loaded from them: the real model, or a test's.
pub(super) struct Model {
    files: ModelFiles,
    /// The folder every revision of it is kept in.
    pub(super) dir: PathBuf,
    from: String,
    load: Load,
    /// Whether this Mac can run it.
    runs_here: fn() -> bool,
}

type Load = Arc<dyn Fn(&Path) -> Result<Box<dyn Engine>, EngineError> + Send + Sync>;

/// What the app holds of dictation.
pub(super) struct Dictating {
    readiness: Readiness,
    pub(super) model: Model,
    /// What recordings hear: the microphone, or what a test gives them.
    audio: AudioIn,
    /// Follows what's under way, the download or the loading, until it's done or dictation is turned off. Dropping it
    /// stops a download.
    following: Option<Task<()>>,
    /// Deletes the model, until it's done.
    pub(super) deleting: Option<Task<()>>,
    /// How much of the model is on this Mac, as of the last time the app changed that, so nothing reads the disk to
    /// draw it.
    bytes_on_disk: u64,
    _following_the_choice: Subscription,
}

impl Global for Dictating {}

/// Has dictation follow the person's choice, with the model kept in `data_dir`. Once, as the app starts.
pub(crate) fn start(data_dir: &Path, cx: &mut App) {
    if cx.has_global::<Dictating>() {
        return;
    }
    let load: Load = Arc::new(|dir| Ok(Box::new(Nemotron::load(dir)?) as Box<dyn Engine>));
    let files = nemotron::MODEL;
    let model_dir = data_dir.join("dictation").join("nemotron");
    let model = Model { files, dir: model_dir, from: files.hugging_face(), load, runs_here: nemotron::runs_here };
    begin(model, AudioIn::Microphone, cx);
}

fn begin(model: Model, audio: AudioIn, cx: &mut App) {
    dictation_ui::unavailable(cx);
    let bytes_on_disk = model::bytes_on_disk(&model.dir);
    let following_the_choice = crate::settings::observe_in_app(cx, follow_the_choice);
    let (readiness, following, deleting) = (Readiness::Off, None, None);
    let dictating = Dictating {
        readiness,
        model,
        audio,
        following,
        deleting,
        bytes_on_disk,
        _following_the_choice: following_the_choice,
    };
    cx.set_global(dictating);
    cx.set_global(ModelDownload(None));
    follow_the_choice(cx);
}

/// What the app holds of dictation, which [`start`] makes before any window opens, so it's there whenever it's asked
/// for.
pub(super) fn dictating(cx: &App) -> &Dictating {
    cx.global::<Dictating>()
}

pub(super) fn dictating_mut(cx: &mut App) -> &mut Dictating {
    cx.global_mut::<Dictating>()
}

/// Turns dictation on or off as the person chose, once that changes, but not while the model is being deleted, which
/// follows the choice again once it's done.
pub(super) fn follow_the_choice(cx: &mut App) {
    let on = dictation::HEARS && crate::settings::get(cx).dictation == DictationChoice::On;
    let dictating = dictating(cx);
    let off = matches!(dictating.readiness, Readiness::Off);
    if on && off && dictating.deleting.is_none() {
        prepare(cx);
    } else if !on && !off {
        turn_off(cx);
    }
}

/// Tries again after a failure.
pub(super) fn retry(cx: &mut App) {
    if matches!(dictating(cx).readiness, Readiness::Failed(_)) {
        prepare(cx);
    }
}

/// Downloads whatever of the model isn't here, then loads it.
fn prepare(cx: &mut App) {
    let model = &dictating(cx).model;
    if !(model.runs_here)() {
        return set(Readiness::NeedsNewerMacos, None, cx);
    }
    if model.files.is_here(&model.dir) {
        return load(cx);
    }
    let told = match downloading::download(model.files, model.dir.clone(), model.from.clone()) {
        Ok(told) => told,
        Err(e) => return set(Readiness::Failed(Failure::Start(e)), None, cx),
    };
    let following = cx.spawn(async move |cx: &mut AsyncApp| {
        while let Ok(said) = told.recv_async().await {
            cx.update(|cx| downloaded(said, cx));
        }
    });
    set(Readiness::Downloading, Some(following), cx);
}

/// What the download says. It's only heard while it's followed, so dictation is downloading.
fn downloaded(said: Downloaded, cx: &mut App) {
    match said {
        Downloaded::Progress(progress) => cx.set_global(ModelDownload(Some(progress))),
        Downloaded::Ended(ended) => {
            reread_whats_on_disk(cx);
            match ended {
                Ok(()) => load(cx),
                Err(e) => set(Readiness::Failed(Failure::Download(e)), None, cx),
            }
        }
    }
}

/// Starts the thread that hears what's dictated, which loads the model now, so the first recording doesn't wait.
fn load(cx: &mut App) {
    let model = &dictating(cx).model;
    let (load, dir) = (model.load.clone(), model.files.dir(&model.dir));
    let dictation = match Dictation::start(move || load(&dir), KEEP_FOR) {
        Ok(dictation) => dictation,
        Err(e) => return set(Readiness::Failed(Failure::Start(e)), None, cx),
    };
    let prepared = dictation.prepare();
    let following = cx.spawn(async move |cx: &mut AsyncApp| {
        let prepared = prepared.recv_async().await;
        let stopped = || Err(EngineError("the thread that loads it stopped".to_string()));
        cx.update(|cx| self::prepared(prepared.unwrap_or_else(|_| stopped()), cx));
    });
    set(Readiness::Preparing(Hearing(dictation)), Some(following), cx);
}

/// The model has loaded, or failed to. It's only heard while it's followed, so dictation is preparing.
fn prepared(prepared: Result<(), EngineError>, cx: &mut App) {
    let dictating = dictating_mut(cx);
    dictating.readiness = match (std::mem::replace(&mut dictating.readiness, Readiness::Off), prepared) {
        (Readiness::Preparing(hearing), Ok(())) => Readiness::Ready(hearing),
        (Readiness::Preparing(_), Err(e)) => Readiness::Failed(Failure::Prepare(e)),
        (readiness, _) => readiness,
    };
}

/// Stops a download, keeping what's come, and lets go of the thread that hears what's dictated, and with it the
/// model. A thread still loading the model finishes first, so turning dictation on again meanwhile loads a second copy
/// for as long as the first takes, and only for someone who turns it off and on that fast.
fn turn_off(cx: &mut App) {
    dictation_ui::unavailable(cx);
    set(Readiness::Off, None, cx);
    reread_whats_on_disk(cx);
}

/// Changes the readiness, and what follows it: dropping what followed the last one stops a download. How far a
/// download has got goes with it.
fn set(readiness: Readiness, following: Option<Task<()>>, cx: &mut App) {
    let downloading = matches!(readiness, Readiness::Downloading);
    let dictating = dictating_mut(cx);
    dictating.readiness = readiness;
    dictating.following = following;
    if !downloading && downloading::progress(cx).is_some() {
        cx.set_global(ModelDownload(None));
    }
}

/// Reads again how much of the model is on this Mac, once the app has changed that.
pub(super) fn reread_whats_on_disk(cx: &mut App) {
    let dictating = dictating_mut(cx);
    dictating.bytes_on_disk = model::bytes_on_disk(&dictating.model.dir);
}

/// Has a view hear whenever how dictation stands changes.
pub(crate) fn observe<V: 'static>(
    cx: &mut Context<V>,
    changed: impl Fn(&mut V, &mut Context<V>) + 'static,
) -> Subscription {
    cx.observe_global::<Dictating>(changed)
}

/// How dictation stands.
pub(super) fn readiness(cx: &App) -> &Readiness {
    &dictating(cx).readiness
}

/// Whether the model is being deleted.
pub(super) fn deleting(cx: &App) -> bool {
    dictating(cx).deleting.is_some()
}

/// How much of the model is on this Mac.
pub(super) fn bytes_on_disk(cx: &App) -> u64 {
    dictating(cx).bytes_on_disk
}

/// How big the model is, whole.
pub(super) fn model_size_bytes(cx: &App) -> u64 {
    dictating(cx).model.files.size_bytes()
}

/// Whether this Mac can run the model, so prompts can offer dictation.
pub(crate) fn runs_here(cx: &App) -> bool {
    dictation::HEARS && (dictating(cx).model.runs_here)()
}

/// Whether opted-in fields can be dictated to.
pub(crate) fn ready(cx: &App) -> bool {
    matches!(readiness(cx), Readiness::Ready(_))
}

/// Whether a recording hears the microphone, which the system has to allow first, rather than audio a test gives it.
pub(crate) fn hears_the_microphone(cx: &App) -> bool {
    dictating(cx).audio == AudioIn::Microphone
}

/// Starts a recording, once dictation is ready, and gives it and what's heard of it.
pub(crate) fn record(cx: &App) -> Option<(Recording, flume::Receiver<Update>)> {
    let dictating = dictating(cx);
    match &dictating.readiness {
        Readiness::Ready(Hearing(dictation)) => Some(dictation.record(dictating.audio)),
        Readiness::Off
        | Readiness::Downloading
        | Readiness::Preparing(_)
        | Readiness::Failed(_)
        | Readiness::NeedsNewerMacos => None,
    }
}

/// Has dictation use the model `files`, kept in `model_dir`, downloading any that aren't there from `from`, and loading
/// them with `load`, from the folder the product loads them from, on the thread that hears what's dictated. Each
/// recording hears only what a test gives it, as `WorkspacePane::hear` does. It follows the choice from now, as it
/// does once the app starts.
#[cfg(feature = "testing")]
pub fn use_model(
    model_dir: PathBuf,
    files: ModelFiles,
    from: String,
    load: impl Fn(&Path) -> Result<dictation::fake::FakeEngine, EngineError> + Send + Sync + 'static,
    cx: &mut App,
) {
    let load: Load = Arc::new(move |dir| Ok(Box::new(load(dir)?) as Box<dyn Engine>));
    begin(Model { files, dir: model_dir, from, load, runs_here: || true }, AudioIn::Fed, cx);
}

/// Turns dictation on with `engine` as its model, which has no files, so it's prepared as soon as the thread starts,
/// and ready once [`is_ready`] says so. Each recording hears only what a test gives it. The model's folder is the
/// app's, in the test's own data directory.
#[cfg(feature = "testing")]
pub fn use_engine(engine: dictation::fake::FakeEngine, cx: &mut App) {
    let files = ModelFiles { repository: "none", revision: "none", files: &[] };
    let model_dir = dictating(cx).model.dir.clone();
    use_model(model_dir, files, String::new(), move |_| Ok(engine.clone()), cx);
    crate::settings::change(cx, |settings| settings.dictation = DictationChoice::On);
}

/// Whether opted-in fields can be dictated to, for a test to wait for.
#[cfg(feature = "testing")]
pub fn is_ready(cx: &App) -> bool {
    ready(cx)
}
