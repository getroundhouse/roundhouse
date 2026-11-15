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
    // The look runs on a real thread, against a real server.
    cx.executor().allow_parking();
    cx.add_window(|_, _| Blank);
    let state = tempfile::tempdir().unwrap();
    let out_of_reach = OutOfReach::in_dir(state.path());
    let manager = manager(base, key, installed);
    cx.update(|cx| keep_up_to_date(manager, out_of_reach, cx));
    state
}

/// Checks for updates, and waits for the look that answers.
fn check(cx: &mut TestAppContext) {
    cx.update(check_now);
    answered_by_the_look(cx);
}

fn answered_by_the_look(cx: &mut TestAppContext) {
    let deadline = Instant::now() + LOOK_TIMEOUT;
    while cx.read(checking) {
        assert!(Instant::now() < deadline, "no answer");
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(10));
    }
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
    // Bound and closed again, so nothing listens there.
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let _state = app_looking_at(&format!("http://127.0.0.1:{port}"), public(&key_pair()), &installed, cx);

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

    let deadline = Instant::now() + LOOK_TIMEOUT;
    while !noted.exists() {
        assert!(Instant::now() < deadline, "the updater wasn't started");
        std::thread::sleep(Duration::from_millis(10));
    }
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
