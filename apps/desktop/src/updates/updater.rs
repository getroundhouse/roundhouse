use super::{answer, location, problem, source};
use crate::appimage;
use answer::Answer;
use flume::RecvTimeoutError;
use gpui_kit::{App, BorrowAppContext as _, Context, Global, Subscription};
use problem::{Kind, OutOfReach, Problem, Said, Shown, Step};
use source::{SignedSource, UpdateFeedKey};
use std::path::Path;
use std::time::{Duration, SystemTime};
use velopack::{UpdateCheck, UpdateInfo, UpdateManager, UpdateOptions, VelopackApp};

/// Where updates come from. The release workflow publishes to the URL in `update-feed.url`. A build can name another
/// in `ROUNDHOUSE_UPDATE_URL`, such as a server on this Mac.
const UPDATE_URL: &str = match option_env!("ROUNDHOUSE_UPDATE_URL") {
    Some(url) => url,
    None => include_str!("update-feed.url").trim_ascii(),
};

/// The update feed's public key, in the form `source` says. The release workflow signs with the private half of the
/// one in `update-feed.key`. A build can name another in `ROUNDHOUSE_UPDATE_KEY`, such as one made for a test.
const UPDATE_FEED_KEY: UpdateFeedKey = UpdateFeedKey::from_base64(match option_env!("ROUNDHOUSE_UPDATE_KEY") {
    Some(key) => key,
    None => include_str!("update-feed.key").trim_ascii(),
});

/// How long to wait before looking again. An update only installs when someone restarts the app for it or quits it, so
/// looking more often would mostly download versions that nobody runs.
const CHECK_EVERY: Duration = Duration::from_hours(1);

/// What a look found, and how long the update server had been out of reach then.
type Look = (Result<Option<UpdateInfo>, Problem>, Duration);

/// Velopack, the update it has downloaded, if any, and a way to have it look now.
struct Updater {
    pub(super) manager: UpdateManager,
    pub(super) ready: Option<UpdateInfo>,
    /// Wakes the thread that looks, so it looks now rather than at the end of its hour.
    pub(super) look_now: flume::Sender<()>,
}

impl Global for Updater {}

/// Velopack's hooks, which it runs through the app's own binary, so this comes first in `main`, once
/// `appimage::leave_behind` has said where the `AppImage` is. An update installs only as the app quits or restarts for
/// it, never as it launches, which would quit and relaunch it as it opens.
pub fn run_hooks() {
    let mut app = VelopackApp::build().set_auto_apply_on_startup(false);
    // The paths `start` gives Velopack, since this is where it clears out old downloads.
    let data_dir = protocol::paths::resolve(|name| std::env::var(name).ok()).map(|paths| paths.data_dir);
    if let Some(paths) = data_dir.ok().and_then(|data_dir| location::paths(&data_dir)) {
        app = app.set_locator(paths);
    }
    app.run();
}

/// How Velopack chooses an update: only a newer version than this one, so a feed can't take the app back to a
/// version with a bug that's since been fixed.
pub(super) fn options() -> UpdateOptions {
    UpdateOptions { AllowVersionDowngrade: false, ..UpdateOptions::default() }
}

/// Starts looking for updates, if Velopack installed this app somewhere it can replace it, and has the newest it
/// downloads installed when the app quits. `data_dir` is this user's, where it notes how long the update server has
/// been out of reach, and where Velopack keeps downloads on Linux.
pub fn start(data_dir: &Path, cx: &mut App) {
    let paths = location::paths(data_dir);
    let manager =
        match UpdateManager::new(SignedSource::new(UPDATE_URL, UPDATE_FEED_KEY), Some(options()), paths.clone()) {
            Ok(manager) => manager,
            Err(velopack::Error::NotInstalled(_)) => return,
            Err(e) => return show(problem(Step::Check, &e), cx),
        };
    if let Some(problem) = location::installed(paths.as_ref()).and_then(|installed| location::cant_install(&installed))
    {
        return show(problem, cx);
    }
    cx.on_app_quit(|cx| {
        install(cx);
        async {}
    })
    .detach();
    keep_up_to_date(manager, OutOfReach::in_dir(data_dir), cx);
}

/// Looks for an update now, rather than at the end of the hour, and says what it found in the sidebar, never in an
/// alert. A build that doesn't update says so at once, and one that can't update where it is already says why.
pub fn check_now(cx: &mut App) {
    let Some(updater) = cx.try_global::<Updater>() else {
        if problem_shown(cx).is_none() {
            say(Answer::NotUpdating, cx);
        }
        return;
    };
    // Full only when a look is already waiting to start, which answers this too.
    let _ = updater.look_now.try_send(());
    cx.update_global::<Shown, _>(|shown, _| {
        shown.checking = true;
        shown.answer = None;
    });
}

/// Looks on launch, every hour and whenever someone checks, and keeps what each look finds.
pub(super) fn keep_up_to_date(manager: UpdateManager, out_of_reach: OutOfReach, cx: &mut App) {
    // Asking again before a look starts adds nothing, so one is enough.
    let (look_now, asked) = flume::bounded(1);
    cx.set_global(Shown::default());
    cx.set_global(Updater { manager: manager.clone(), ready: None, look_now });
    // A thread of its own, since a look blocks, a download can take minutes, and GPUI's background threads are the
    // ones the app's calls to its hub go through.
    let (looks, looked) = flume::unbounded();
    std::thread::spawn(move || keep_looking(&manager, &asked, &looks, &out_of_reach));
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

/// Whether someone checked for updates and the look that answers them hasn't finished.
#[cfg(test)]
pub(crate) fn checking(cx: &App) -> bool {
    cx.try_global::<Shown>().is_some_and(|shown| shown.checking)
}

/// What the sidebar says about updating in the version's place, if anything, as [`Said`] says.
pub(crate) fn said(cx: &App) -> Option<Said<'_>> {
    cx.try_global::<Shown>().and_then(Shown::said)
}

/// The version of the update that's downloaded, if there is one.
pub(crate) fn ready(cx: &App) -> Option<&str> {
    cx.try_global::<Shown>().and_then(|shown| shown.ready.as_deref())
}

/// What a check someone asked for found, while it's shown, when it isn't an update or a problem.
#[cfg(test)]
pub(crate) fn answered(cx: &App) -> Option<Answer> {
    cx.try_global::<Shown>().and_then(|shown| shown.answer)
}

/// Shows what a check found for a few seconds, or until the next check.
fn say(found: Answer, cx: &mut App) {
    let shown = cx.default_global::<Shown>();
    shown.answer = Some(found);
    shown.answers += 1;
    let this = shown.answers;
    cx.spawn(async move |cx| {
        cx.background_executor().timer(answer::SHOWN_FOR).await;
        cx.update(|cx| {
            if cx.global::<Shown>().answers == this {
                cx.global_mut::<Shown>().answer = None;
            }
        });
    })
    .detach();
}

/// Shows a problem as if updating had failed, which only an installed app can do for real.
#[cfg(feature = "testing")]
pub fn show_problem(problem: Problem, cx: &mut App) {
    show(problem, cx);
}

/// Shows a check as under way, which only an installed app can start for real.
#[cfg(feature = "testing")]
pub fn show_checking(cx: &mut App) {
    cx.set_global(Shown { checking: true, ..Shown::default() });
}

/// Shows a check as having found nothing newer, which only an installed app can do for real.
#[cfg(feature = "testing")]
pub fn show_up_to_date(cx: &mut App) {
    cx.set_global(Shown { answer: Some(Answer::UpToDate), ..Shown::default() });
}

/// Shows an update as downloaded, which only an installed app can do for real.
#[cfg(feature = "testing")]
pub fn show_ready(version: &str, cx: &mut App) {
    cx.set_global(Shown { ready: Some(version.to_string()), ..Shown::default() });
}

/// Looks, then waits an hour or until someone asks, for as long as the app takes what it finds.
pub(super) fn keep_looking(
    manager: &UpdateManager,
    asked: &flume::Receiver<()>,
    looks: &flume::Sender<Look>,
    out_of_reach: &OutOfReach,
) {
    loop {
        let found = newest(manager);
        let reached = !matches!(&found, Err(problem) if problem.kind == Kind::Unreachable);
        // Whoever has asked by now hears what this look found, since it reaches the app after they asked, so it
        // needn't look again for them. Whoever asks from here on leaves their ask for the next look.
        let _ = asked.drain();
        if looks.send((found, out_of_reach.note(reached, SystemTime::now()))).is_err() {
            return;
        }
        if matches!(asked.recv_timeout(CHECK_EVERY), Err(RecvTimeoutError::Disconnected)) {
            return;
        }
    }
}

/// Checks the feed, and downloads whatever is newer than this app.
pub(super) fn newest(manager: &UpdateManager) -> Result<Option<UpdateInfo>, Problem> {
    match manager.check_for_updates().map_err(|e| problem(Step::Check, &e))? {
        UpdateCheck::UpdateAvailable(info) => {
            manager.download_updates(&info, None).map_err(|e| problem(Step::Download, &e))?;
            Ok(Some(*info))
        }
        UpdateCheck::NoUpdateAvailable | UpdateCheck::RemoteIsEmpty => Ok(None),
    }
}

fn record((found, out_of_reach): Look, cx: &mut App) {
    let mut downloaded = None;
    let outcome = found.map(|found| {
        if let Some(info) = found {
            downloaded = Some(info.TargetFullRelease.Version.clone());
            cx.global_mut::<Updater>().ready = Some(info);
        }
    });
    // An update an earlier look downloaded answers too, as its card, since it's what installs. So does a problem, as
    // itself.
    let up_to_date = cx.global::<Shown>().checking && outcome.is_ok() && cx.global::<Updater>().ready.is_none();
    cx.update_global::<Shown, _>(|shown, _| {
        shown.record(outcome, out_of_reach);
        if downloaded.is_some() {
            shown.ready = downloaded;
        }
    });
    if up_to_date {
        say(Answer::UpToDate, cx);
    }
}

/// Installs the downloaded update and opens the app again. Velopack's updater waits for this process to exit, so the
/// app quits first. Agents keep running, since the hub doesn't belong to the app.
pub(crate) fn restart_to_update(cx: &mut App) {
    match apply(true, cx) {
        Some(Ok(())) => {
            // So quitting doesn't hand it to a second updater, and the card goes as the app does.
            cx.global_mut::<Updater>().ready = None;
            cx.update_global::<Shown, _>(|shown, _| shown.ready = None);
            cx.quit();
        }
        Some(Err(e)) => cx.update_global::<Shown, _>(|shown, _| shown.problem = Some(problem(Step::Install, &e))),
        // The card showed an update that Velopack doesn't have, so pressing it says so rather than do nothing.
        None => {
            let nothing = Problem { kind: Kind::Failed(Step::Install), detail: "nothing is downloaded".to_string() };
            cx.default_global::<Shown>().problem = Some(nothing);
        }
    }
}

/// Hands the downloaded update to Velopack's updater, which waits for this process to exit, then replaces the app.
/// Silently, since nobody is there to answer it, and without relaunching, since quitting was asked for.
fn install(cx: &mut App) {
    if let Some(Err(e)) = apply(false, cx) {
        eprintln!("couldn't install the update: {e}");
    }
}

/// Hands the downloaded update, if there is one, to Velopack's updater, which installs it once this process exits,
/// without asking anything, and opens the app again if `restart`. On Linux the updater runs from inside the
/// `AppImage`'s mount, and goes on after the app quits, so it holds the mount up.
pub(super) fn apply(restart: bool, cx: &App) -> Option<Result<(), velopack::Error>> {
    let Some(Updater { manager, ready: Some(info), .. }) = cx.try_global::<Updater>() else { return None };
    Some(appimage::sharing_mount(|| manager.wait_exit_then_apply_updates(info, true, restart, Vec::<String>::new())))
}

/// Shows a problem that no later look clears, since there won't be one.
fn show(problem: Problem, cx: &mut App) {
    cx.set_global(Shown { problem: Some(problem), ..Shown::default() });
}
