//! Keeps the app up to date through Velopack, which packages and installs it. It looks for an update on launch and
//! every hour, downloads it in the background, and installs it when the app quits, without relaunching. This is the
//! one module that knows about Velopack, so replacing it touches nothing else. A build that Velopack didn't install,
//! such as `cargo run` or `tools/install.sh`, doesn't update.

mod feed;
#[cfg(test)]
mod serving;

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gpui_kit::{App, AppContext as _, BorrowAppContext as _, Context, Global, Subscription};
use velopack::{UpdateCheck, UpdateInfo, UpdateManager, UpdateOptions, VelopackApp};

use feed::{FeedError, FeedKey, SignedFeed};

/// Where updates come from. A build can name another in `ROUNDHOUSE_UPDATE_URL`, such as a server on this Mac.
const FEED_URL: &str = match option_env!("ROUNDHOUSE_UPDATE_URL") {
    Some(url) => url,
    None => "https://f001.backblazeb2.com/file/getroundhouse-releases",
};

/// The public half of the key the release workflow signs the feed with, the raw 32 bytes in base64. The workflow
/// checks its key against this line. A build can name another in `ROUNDHOUSE_UPDATE_KEY`, such as one made for a test.
const FEED_KEY: FeedKey = FeedKey::from_base64(match option_env!("ROUNDHOUSE_UPDATE_KEY") {
    Some(key) => key,
    None => "qF43VUP56iLRGkrpvzwDStkQtj+34JyWp5SgpUeisdc=",
});

/// How long to wait before looking again. An update only installs when the app quits, so looking more often would
/// mostly download versions that nobody runs.
const CHECK_EVERY: Duration = Duration::from_hours(1);

/// How long the update server can be out of reach before the sidebar says so. A laptop is often offline for an hour or
/// two, which says nothing about updating.
const QUIET_FOR: Duration = Duration::from_hours(24);

/// Whether Velopack installs a downloaded update when the app launches, which would quit and relaunch it as it opens.
const INSTALL_AT_LAUNCH: bool = false;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Check,
    Download,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The update server couldn't be reached, or didn't send the file, for longer than [`QUIET_FOR`].
    Unreachable,
    /// The feed or a package wasn't what the release workflow signed, so nothing from it was used. Says what.
    Refused(&'static str),
    /// Anything else that stopped a check or a download.
    Failed(Step),
    /// macOS runs the app from a read-only copy, because it was opened where it was downloaded.
    Translocated,
    /// The app is somewhere this user can't change.
    ReadOnly,
}

/// Why the app can't update. The sidebar says so until a later look succeeds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub kind: Kind,
    /// Velopack's, the feed's or the system's own words, for whoever wants the detail.
    pub detail: String,
}

impl Problem {
    pub fn summary(&self) -> String {
        match self.kind {
            Kind::Unreachable => "Couldn't reach the update server".to_string(),
            Kind::Refused(what) => format!("Update refused: {what}"),
            Kind::Failed(Step::Check) => "Couldn't check for updates".to_string(),
            Kind::Failed(Step::Download) => "Couldn't download an update".to_string(),
            Kind::Translocated => "Move Roundhouse to Applications to update it".to_string(),
            Kind::ReadOnly => "Can't update Roundhouse where it is".to_string(),
        }
    }
}

/// What the sidebar shows about updating, and when looking last worked.
struct Shown {
    problem: Option<Problem>,
    last_worked: Instant,
}

impl Global for Shown {}

impl Shown {
    fn new(now: Instant) -> Self {
        Self { problem: None, last_worked: now }
    }

    /// Takes in how a look went. Being out of reach only shows once it has lasted, and anything else at once.
    fn record(&mut self, outcome: Result<(), Problem>, now: Instant) {
        match outcome {
            Ok(()) => *self = Self::new(now),
            Err(problem) if problem.kind == Kind::Unreachable && now - self.last_worked < QUIET_FOR => {}
            Err(problem) => self.problem = Some(problem),
        }
    }
}

/// Velopack, and the update it has downloaded, if any.
struct Updater {
    manager: UpdateManager,
    ready: Option<UpdateInfo>,
}

impl Global for Updater {}

/// Velopack's hooks, which it runs through the app's own binary, so this comes first in `main`.
pub fn run_hooks() {
    VelopackApp::build().set_auto_apply_on_startup(INSTALL_AT_LAUNCH).run();
}

/// How Velopack chooses an update: only a newer version than this one, so a feed can't take the app back to a
/// version with a bug that's since been fixed.
fn options() -> UpdateOptions {
    UpdateOptions { AllowVersionDowngrade: false, ..UpdateOptions::default() }
}

/// Starts looking for updates, if Velopack installed this app somewhere it can replace it, and has the newest it
/// downloads installed when the app quits.
pub fn start(cx: &mut App) {
    let manager = match UpdateManager::new(SignedFeed::new(FEED_URL, FEED_KEY), Some(options()), None) {
        Ok(manager) => manager,
        Err(velopack::Error::NotInstalled(_)) => return,
        Err(e) => return show(problem(Step::Check, &e), cx),
    };
    if let Some(problem) = bundle().and_then(|bundle| cant_install(&bundle)) {
        return show(problem, cx);
    }
    cx.set_global(Shown::new(Instant::now()));
    cx.set_global(Updater { manager: manager.clone(), ready: None });
    cx.on_app_quit(|cx| {
        install(cx);
        async {}
    })
    .detach();
    cx.spawn(async move |cx| {
        loop {
            let manager = manager.clone();
            // Blocking, and a download can take minutes.
            let found = cx.background_spawn(async move { newest(&manager) }).await;
            cx.update(|cx| record(found, cx));
            cx.background_executor().timer(CHECK_EVERY).await;
        }
    })
    .detach();
}

/// Has a view drawn again whenever what's shown about updating changes.
pub(crate) fn observe<V: 'static>(cx: &mut Context<V>) -> Subscription {
    cx.observe_global::<Shown>(|_, cx| cx.notify())
}

/// The problem updating, if there is one.
pub(crate) fn problem_shown(cx: &App) -> Option<&Problem> {
    cx.try_global::<Shown>().and_then(|shown| shown.problem.as_ref())
}

/// Shows a problem as if updating had failed, which only an installed app can do for real.
#[cfg(feature = "testing")]
pub fn show_problem(problem: Problem, cx: &mut App) {
    show(problem, cx);
}

/// The `.app` this binary runs from, which Velopack replaces whole. Only a Mac app has one.
fn bundle() -> Option<PathBuf> {
    let binary = std::env::current_exe().ok()?;
    binary.ancestors().find(|dir| dir.extension().is_some_and(|extension| extension == "app")).map(Path::to_path_buf)
}

/// Why the app can't replace itself where it is, if it can't. Velopack replaces the whole bundle, so it has to be able
/// to change the bundle and the folder it's in.
fn cant_install(bundle: &Path) -> Option<Problem> {
    if bundle.components().any(|part| part.as_os_str() == "AppTranslocation") {
        let detail = format!(
            "macOS runs this copy from {}, which can't be changed, because it was opened where it was downloaded. \
             Moved to Applications, it can update.",
            bundle.display()
        );
        return Some(Problem { kind: Kind::Translocated, detail });
    }
    let folder = bundle.parent()?;
    [folder, bundle].into_iter().find(|path| !writable(path)).map(|path| Problem {
        kind: Kind::ReadOnly,
        detail: format!("This user can't change {}, so Roundhouse can't replace itself there.", path.display()),
    })
}

fn writable(path: &Path) -> bool {
    let Ok(path) = CString::new(path.as_os_str().as_bytes()) else { return false };
    // SAFETY: `path` is a NUL-terminated string that outlives the call, which only reads it.
    unsafe { libc::access(path.as_ptr(), libc::W_OK) == 0 }
}

/// Checks the feed, and downloads whatever is newer than this app.
fn newest(manager: &UpdateManager) -> Result<Option<UpdateInfo>, Problem> {
    match manager.check_for_updates().map_err(|e| problem(Step::Check, &e))? {
        UpdateCheck::UpdateAvailable(info) => {
            manager.download_updates(&info, None).map_err(|e| problem(Step::Download, &e))?;
            Ok(Some(*info))
        }
        UpdateCheck::NoUpdateAvailable | UpdateCheck::RemoteIsEmpty => Ok(None),
    }
}

/// What a failure means for the person using the app. The feed's own errors come back from Velopack whole.
fn problem(step: Step, error: &velopack::Error) -> Problem {
    if let velopack::Error::Io(io) = error
        && let Some(feed) = io.get_ref().and_then(|inner| inner.downcast_ref::<FeedError>())
    {
        return Problem { kind: feed.kind(step), detail: feed.to_string() };
    }
    let kind = match error {
        velopack::Error::ChecksumInvalid(..) | velopack::Error::SizeInvalid(..) => {
            Kind::Refused("a download doesn't match the feed")
        }
        velopack::Error::Network(_) => Kind::Unreachable,
        _ => Kind::Failed(step),
    };
    Problem { kind, detail: error.to_string() }
}

fn record(found: Result<Option<UpdateInfo>, Problem>, cx: &mut App) {
    let outcome = found.map(|found| {
        if let Some(info) = found {
            cx.global_mut::<Updater>().ready = Some(info);
        }
    });
    cx.update_global::<Shown, _>(|shown, _| shown.record(outcome, Instant::now()));
}

/// Hands the downloaded update to Velopack's updater, which waits for this process to exit, then replaces the app.
/// Silently, since nobody is there to answer it, and without relaunching, since quitting was asked for.
fn install(cx: &mut App) {
    let Some(Updater { manager, ready: Some(info) }) = cx.try_global::<Updater>() else { return };
    if let Err(e) = manager.wait_exit_then_apply_updates(info, true, false, Vec::<String>::new()) {
        eprintln!("couldn't install the update: {e}");
    }
}

/// Shows a problem that no later look clears, since there won't be one.
fn show(problem: Problem, cx: &mut App) {
    cx.set_global(Shown { problem: Some(problem), last_worked: Instant::now() });
}

#[cfg(test)]
mod tests;
