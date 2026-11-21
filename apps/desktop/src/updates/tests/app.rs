use super::*;

/// The app's window, where an alert would show.
struct Blank;

impl Render for Blank {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui_kit::Empty
    }
}

/// An app that looks for updates at `base`, as `start` has it look, with its window open, and the directory where it
/// notes how long the server has been out of reach.
fn app_looking_at(
    base: &str,
    key: UpdateFeedKey,
    installed: &VelopackLocatorConfig,
    cx: &mut TestAppContext,
) -> tempfile::TempDir {
    app_keeping(base, key, installed, ToKeep::oldest_at(None), cx)
}

/// The same, keeping `workers` until the app says otherwise.
fn app_keeping(
    base: &str,
    key: UpdateFeedKey,
    installed: &VelopackLocatorConfig,
    workers: ToKeep,
    cx: &mut TestAppContext,
) -> tempfile::TempDir {
    // The look runs on a real thread, against a real server.
    cx.executor().allow_parking();
    cx.add_window(|_, _| Blank);
    let state = tempfile::tempdir().unwrap();
    let out_of_reach = OutOfReach::in_dir(state.path());
    let manager = manager_keeping(base, key, installed, &workers);
    cx.update(|cx| keep_up_to_date(manager, workers, out_of_reach, cx));
    state
}

/// Waits for the update a quit would install to be `version`, and checks the card says the same.
fn ready_to_be(version: Option<&str>, cx: &mut TestAppContext) {
    crate::executor_waits::until(cx, LOOK_TIMEOUT, format!("{version:?} never got ready"), |cx| {
        cx.read(|cx| installs(cx).map(str::to_string)).as_deref() == version
    });
    assert_eq!(cx.read(|cx| ready(cx).map(str::to_string)).as_deref(), version, "what the card says");
}

/// Checks for updates, and waits for the look that answers.
fn check(cx: &mut TestAppContext) {
    cx.update(check_now);
    answered_by_the_look(cx);
}

fn answered_by_the_look(cx: &mut TestAppContext) {
    crate::executor_waits::until(cx, LOOK_TIMEOUT, "no answer", |cx| !cx.read(checking));
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt(), "the sidebar answers, never an alert");
}

#[gpui_kit::test]
fn checking_looks_again_and_answers_with_the_updates_card(cx: &mut TestAppContext) {
    let (_dir, installed) = installed();
    let pair = key_pair();
    let new = b"0.1.6, packaged".as_slice();
    let server = publish(&[("0.1.6", new, Reply::file(new))], &pair);
    let _state = app_looking_at(&server.base, public(&pair), &installed, cx);
    check(cx);
    let looks_before = feeds_read(&server);

    cx.update(check_now);
    assert!(cx.read(checking));
    answered_by_the_look(cx);

    assert_eq!(cx.read(|cx| ready(cx).map(str::to_string)).as_deref(), Some("0.1.6"));
    assert_eq!(cx.read(answered), None);
    assert!(feeds_read(&server) > looks_before);
}

#[gpui_kit::test]
fn checking_says_its_up_to_date_for_a_few_seconds(cx: &mut TestAppContext) {
    let (_dir, installed) = installed();
    let pair = key_pair();
    let server = publish(&[("0.1.5", b"", Reply::file(""))], &pair);
    let _state = app_looking_at(&server.base, public(&pair), &installed, cx);

    check(cx);

    assert_eq!(cx.read(answered), Some(Answer::UpToDate));
    cx.executor().advance_clock(answer::SHOWN_FOR);
    assert_eq!(cx.read(answered), None);
}

/// Each answer is shown for its own few seconds, even when the one before hasn't gone yet.
#[gpui_kit::test]
fn checking_again_shows_the_new_answer_for_as_long(cx: &mut TestAppContext) {
    let (_dir, installed) = installed();
    let pair = key_pair();
    let server = publish(&[("0.1.5", b"", Reply::file(""))], &pair);
    let _state = app_looking_at(&server.base, public(&pair), &installed, cx);
    check(cx);
    cx.executor().advance_clock(answer::SHOWN_FOR / 2);

    check(cx);
    cx.executor().advance_clock(answer::SHOWN_FOR * 3 / 4);

    assert_eq!(cx.read(answered), Some(Answer::UpToDate));
}

/// The sidebar waits a day before it says so, but someone who asked hears at once.
#[gpui_kit::test]
fn checking_shows_at_once_that_the_update_server_is_out_of_reach(cx: &mut TestAppContext) {
    let (_dir, installed) = installed();
    let _state = app_looking_at(NOWHERE, public(&key_pair()), &installed, cx);

    check(cx);

    assert_eq!(cx.read(|cx| problem_shown(cx).map(|p| p.kind)), Some(Kind::Unreachable));
    assert_eq!(cx.read(answered), None);
}

/// Has the stand-in for Velopack's updater write down what it was asked to do, and returns where. It writes the note
/// beside that and renames it into place, so the note is there only once it's whole, and a test that waits for it
/// reads all of it.
fn updater_that_notes(installed: &VelopackLocatorConfig) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt as _;
    let noted = installed.RootAppDir.with_file_name("asked");
    let script = format!("#!/bin/sh\necho \"$@\" > '{0}.part' && mv '{0}.part' '{0}'\n", noted.display());
    std::fs::write(&installed.UpdateExePath, script).unwrap();
    std::fs::set_permissions(&installed.UpdateExePath, std::fs::Permissions::from_mode(0o755)).unwrap();
    noted
}

/// An app that has downloaded 0.1.6, and the server it came from.
fn with_an_update_ready(installed: &VelopackLocatorConfig, cx: &mut TestAppContext) -> (Server, tempfile::TempDir) {
    let pair = key_pair();
    let new = b"0.1.6, packaged".as_slice();
    let server = publish(&[("0.1.6", new, Reply::file(new))], &pair);
    let state = app_looking_at(&server.base, public(&pair), installed, cx);
    check(cx);
    assert_eq!(cx.read(|cx| ready(cx).map(str::to_string)).as_deref(), Some("0.1.6"));
    (server, state)
}

#[gpui_kit::test]
fn restarting_to_update_has_velopack_install_it_and_open_the_app_again(cx: &mut TestAppContext) {
    let (_dir, installed) = installed();
    let noted = updater_that_notes(&installed);
    let _app = with_an_update_ready(&installed, cx);

    cx.update(restart_to_update);

    waiting::thread::until(LOOK_TIMEOUT, "the updater wasn't started", || noted.exists());
    let asked = std::fs::read_to_string(&noted).unwrap();
    assert!(asked.starts_with("apply "), "{asked}");
    assert!(asked.contains(&format!("--waitPid {}", std::process::id())), "{asked}");
    assert!(!asked.contains("--norestart"), "{asked}");
}

#[gpui_kit::test]
fn after_restarting_to_update_quitting_has_nothing_to_install(cx: &mut TestAppContext) {
    let (_dir, installed) = installed();
    let _noted = updater_that_notes(&installed);
    let _app = with_an_update_ready(&installed, cx);

    cx.update(restart_to_update);

    assert!(cx.read(|cx| apply(false, cx)).is_none(), "quitting would hand it to a second updater");
    assert!(cx.read(|cx| ready(cx).is_none()), "the card goes as the app does");
}

#[gpui_kit::test]
fn an_update_that_cant_install_says_so_and_stays_ready(cx: &mut TestAppContext) {
    // The stand-in for Velopack's updater is an empty file, which can't be run.
    let (_dir, installed) = installed();
    let _app = with_an_update_ready(&installed, cx);

    cx.update(restart_to_update);

    assert_eq!(cx.read(|cx| problem_shown(cx).map(|p| p.kind)), Some(Kind::Failed(Step::Install)));
    assert_eq!(cx.read(|cx| ready(cx).map(str::to_string)).as_deref(), Some("0.1.6"));
}

/// Until the app hears from its hub, its update crosses no break in the worker protocol, and as soon as the hub says no
/// worker as old is at work, it looks again and takes the newest. Should it come to keep older workers again, it drops
/// the update at once, before the look that finds it can't keep them, and takes nothing.
#[gpui_kit::test]
fn the_update_ready_is_the_newest_whose_hub_speaks_the_workers_to_keep(cx: &mut TestAppContext) {
    let (_dir, installed) = installed();
    let pair = key_pair();
    let (before, after) = (b"0.1.6, packaged".as_slice(), b"0.1.7, packaged".as_slice());
    let server = publish_speaking(
        &[
            ("0.1.6", before, Reply::file(before), Some(Versions { oldest: 3, newest: 4 })),
            ("0.1.7", after, Reply::file(after), Some(Versions { oldest: 4, newest: 5 })),
        ],
        &pair,
    );
    let _state = app_keeping(&server.base, public(&pair), &installed, ToKeep::oldest_at(Some(3)), cx);
    ready_to_be(Some("0.1.6"), cx);

    cx.update(|cx| keep(Keeping { workers: Some(4), hub: None }, cx));
    ready_to_be(Some("0.1.7"), cx);

    cx.update(|cx| keep(Keeping { workers: Some(2), hub: None }, cx));
    assert_eq!(cx.read(|cx| installs(cx).map(str::to_string)), None, "before the look");
    ready_to_be(None, cx);
}

/// A look reads which workers to keep as it reads the feed, so one that changes while the look downloads what it found
/// has the look followed by another at once, rather than at the end of the hour, though the look takes the asks that
/// came while it ran as answered.
#[gpui_kit::test]
fn workers_to_keep_that_change_during_a_look_are_looked_for_again_at_once(cx: &mut TestAppContext) {
    let (_dir, installed) = installed();
    let pair = key_pair();
    let (before, after) = (b"0.1.6, packaged".as_slice(), b"0.1.7, packaged".as_slice());
    let (held, release) = Reply::file(before).held_after(0);
    let server = publish_speaking(
        &[
            ("0.1.6", before, held, Some(Versions { oldest: 3, newest: 4 })),
            ("0.1.7", after, Reply::file(after), Some(Versions { oldest: 4, newest: 5 })),
        ],
        &pair,
    );
    let _state = app_keeping(&server.base, public(&pair), &installed, ToKeep::oldest_at(Some(3)), cx);
    waiting::thread::until(LOOK_TIMEOUT, "the first look never downloaded 0.1.6", || {
        server.asked().contains(&package_name("0.1.6"))
    });

    cx.update(|cx| keep(Keeping { workers: Some(4), hub: None }, cx));
    release.now();

    ready_to_be(Some("0.1.7"), cx);
}
