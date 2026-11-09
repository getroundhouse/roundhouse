//! Keeps the app up to date through Velopack, which packages and installs it. It looks for an update on launch and
//! every hour, downloads it in the background, and installs it when the app quits, without relaunching. This is the
//! one module that knows about Velopack, so replacing it touches nothing else. A build that Velopack didn't install,
//! such as `cargo run` or `tools/install.sh`, doesn't update.

mod location;
mod problem;
#[cfg(test)]
mod serving;
#[cfg(test)]
mod signing;
mod source;

use std::path::Path;
use std::time::{Duration, SystemTime};

use gpui_kit::{App, BorrowAppContext as _, Context, Global, Subscription};
use velopack::{UpdateCheck, UpdateInfo, UpdateManager, UpdateOptions, VelopackApp};

pub use problem::{Kind, Problem, Refusal, Step};
use problem::{OutOfReach, Shown, problem};
use source::{SignedSource, UpdateFeedKey};

/// Where updates come from. The release workflow publishes to the URL in `update-feed.url`. A build can name another
/// in `ROUNDHOUSE_UPDATE_URL`, such as a server on this Mac.
const UPDATE_URL: &str = match option_env!("ROUNDHOUSE_UPDATE_URL") {
    Some(url) => url,
    None => include_str!("updates/update-feed.url").trim_ascii(),
};

/// The update feed's public key, in the form `source` says. The release workflow signs with the private half of the
/// one in `update-feed.key`. A build can name another in `ROUNDHOUSE_UPDATE_KEY`, such as one made for a test.
const UPDATE_FEED_KEY: UpdateFeedKey = UpdateFeedKey::from_base64(match option_env!("ROUNDHOUSE_UPDATE_KEY") {
    Some(key) => key,
    None => include_str!("updates/update-feed.key").trim_ascii(),
});

/// How long to wait before looking again. An update only installs when the app quits, so looking more often would
/// mostly download versions that nobody runs.
const CHECK_EVERY: Duration = Duration::from_hours(1);

/// What a look found, and how long the update server had been out of reach then.
type Look = (Result<Option<UpdateInfo>, Problem>, Duration);

/// Velopack, and the update it has downloaded, if any.
struct Updater {
    manager: UpdateManager,
    ready: Option<UpdateInfo>,
}

impl Global for Updater {}

/// Velopack's hooks, which it runs through the app's own binary, so this comes first in `main`. An update installs
/// only when the app quits, never as it launches, which would quit and relaunch it as it opens.
pub fn run_hooks() {
    VelopackApp::build().set_auto_apply_on_startup(false).run();
}

/// How Velopack chooses an update: only a newer version than this one, so a feed can't take the app back to a
/// version with a bug that's since been fixed.
fn options() -> UpdateOptions {
    UpdateOptions { AllowVersionDowngrade: false, ..UpdateOptions::default() }
}

/// Starts looking for updates, if Velopack installed this app somewhere it can replace it, and has the newest it
/// downloads installed when the app quits. `data_dir` is this user's, where it notes how long the update server has
/// been out of reach.
pub fn start(data_dir: &Path, cx: &mut App) {
    let manager = match UpdateManager::new(SignedSource::new(UPDATE_URL, UPDATE_FEED_KEY), Some(options()), None) {
        Ok(manager) => manager,
        Err(velopack::Error::NotInstalled(_)) => return,
        Err(e) => return show(problem(Step::Check, &e), cx),
    };
    if let Some(problem) = location::bundle().and_then(|bundle| location::cant_install(&bundle)) {
        return show(problem, cx);
    }
    cx.set_global(Shown::default());
    cx.set_global(Updater { manager: manager.clone(), ready: None });
    cx.on_app_quit(|cx| {
        install(cx);
        async {}
    })
    .detach();
    // A thread of its own, since a look blocks, a download can take minutes, and GPUI's background threads are the
    // ones the app's calls to its hub go through.
    let (looks, looked) = flume::unbounded();
    let out_of_reach = OutOfReach::in_dir(data_dir);
    std::thread::spawn(move || {
        loop {
            let found = newest(&manager);
            let reached = !matches!(&found, Err(problem) if problem.kind == Kind::Unreachable);
            if looks.send((found, out_of_reach.note(reached, SystemTime::now()))).is_err() {
                return;
            }
            std::thread::sleep(CHECK_EVERY);
        }
    });
    cx.spawn(async move |cx| {
        while let Ok(look) = looked.recv_async().await {
            cx.update(|cx| record(look, cx));
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

fn record((found, out_of_reach): Look, cx: &mut App) {
    let outcome = found.map(|found| {
        if let Some(info) = found {
            cx.global_mut::<Updater>().ready = Some(info);
        }
    });
    cx.update_global::<Shown, _>(|shown, _| shown.record(outcome, out_of_reach));
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
    cx.set_global(Shown { problem: Some(problem) });
}

#[cfg(test)]
mod tests;
