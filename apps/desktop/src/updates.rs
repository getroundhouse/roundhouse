//! Keeps the app up to date through Velopack, which packages and installs it. It looks for an update on launch and
//! every hour, downloads it in the background, and installs it when the app quits, without relaunching. This is the
//! one module that knows about Velopack, so replacing it touches nothing else. A build that Velopack didn't install,
//! such as `cargo run` or `tools/install.sh`, doesn't update.

mod feed;

use std::time::Duration;

use gpui_kit::{App, AppContext as _, Context, Global, Subscription};
use velopack::{UpdateCheck, UpdateInfo, UpdateManager, VelopackApp};

use feed::{FeedKey, SignedFeed};

/// Where updates come from. A build can name another in `ROUNDHOUSE_UPDATE_URL`, such as a server on this Mac.
const FEED_URL: &str = match option_env!("ROUNDHOUSE_UPDATE_URL") {
    Some(url) => url,
    None => "https://f001.backblazeb2.com/file/getroundhouse-releases",
};

/// The public half of the key the release workflow signs the feed with, the raw 32 bytes in base64. A build can name
/// another in `ROUNDHOUSE_UPDATE_KEY`, such as one made for a test.
const FEED_KEY: FeedKey = FeedKey::from_base64(match option_env!("ROUNDHOUSE_UPDATE_KEY") {
    Some(key) => key,
    None => "qF43VUP56iLRGkrpvzwDStkQtj+34JyWp5SgpUeisdc=",
});

/// How long to wait before looking again. An update only installs when the app quits, so looking more often would
/// mostly download versions that nobody runs.
const CHECK_EVERY: Duration = Duration::from_hours(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Check,
    Download,
}

/// Why the app couldn't update. The sidebar says so until a later look succeeds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub step: Step,
    /// Velopack's or the feed's own words, for whoever wants the detail.
    pub reason: String,
}

impl Problem {
    pub fn summary(&self) -> &'static str {
        match self.step {
            Step::Check => "Couldn't check for updates",
            Step::Download => "Couldn't download an update",
        }
    }
}

/// The problem updating, if there is one, which is all the sidebar shows about updating.
struct Shown(Option<Problem>);

impl Global for Shown {}

/// Velopack, and the update it has downloaded, if any.
struct Updater {
    manager: UpdateManager,
    ready: Option<UpdateInfo>,
}

impl Global for Updater {}

/// Velopack's hooks, which it runs through the app's own binary, so this comes first in `main`. It doesn't install a
/// downloaded update at launch, which would quit and relaunch the app as it opens.
pub fn run_hooks() {
    VelopackApp::build().set_auto_apply_on_startup(false).run();
}

/// Starts looking for updates, if Velopack installed this app, and has the newest it downloads installed when the app
/// quits.
pub fn start(cx: &mut App) {
    let manager = match UpdateManager::new(SignedFeed::new(FEED_URL, FEED_KEY), None, None) {
        Ok(manager) => manager,
        Err(velopack::Error::NotInstalled(_)) => return,
        Err(e) => return show(Some(Problem { step: Step::Check, reason: e.to_string() }), cx),
    };
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
pub(crate) fn problem(cx: &App) -> Option<&Problem> {
    cx.try_global::<Shown>().and_then(|shown| shown.0.as_ref())
}

/// Shows a problem as if updating had failed, which only an installed app can do for real.
#[cfg(feature = "testing")]
pub fn show_problem(problem: Problem, cx: &mut App) {
    show(Some(problem), cx);
}

/// Checks the feed, and downloads whatever is newer than this app.
fn newest(manager: &UpdateManager) -> Result<Option<UpdateInfo>, Problem> {
    let failed = |step| move |e: velopack::Error| Problem { step, reason: e.to_string() };
    match manager.check_for_updates().map_err(failed(Step::Check))? {
        UpdateCheck::UpdateAvailable(info) => {
            manager.download_updates(&info, None).map_err(failed(Step::Download))?;
            Ok(Some(*info))
        }
        UpdateCheck::NoUpdateAvailable | UpdateCheck::RemoteIsEmpty => Ok(None),
    }
}

fn record(found: Result<Option<UpdateInfo>, Problem>, cx: &mut App) {
    match found {
        Ok(found) => {
            if let Some(info) = found {
                cx.global_mut::<Updater>().ready = Some(info);
            }
            show(None, cx);
        }
        Err(problem) => show(Some(problem), cx),
    }
}

/// Hands the downloaded update to Velopack's updater, which waits for this process to exit, then replaces the app.
/// Silently, since nobody is there to answer it, and without relaunching, since quitting was asked for.
fn install(cx: &mut App) {
    let Some(Updater { manager, ready: Some(info) }) = cx.try_global::<Updater>() else { return };
    if let Err(e) = manager.wait_exit_then_apply_updates(info, true, false, Vec::<String>::new()) {
        eprintln!("couldn't install the update: {e}");
    }
}

fn show(problem: Option<Problem>, cx: &mut App) {
    cx.set_global(Shown(problem));
}
