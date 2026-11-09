use std::fmt::Write as _;
use std::path::Path;
use std::time::Instant;

use gpui_kit::{IntoElement, Render, TestAppContext, Window};
use ring::digest::{SHA1_FOR_LEGACY_USE_ONLY, SHA256, digest};
use ring::signature::Ed25519KeyPair;
use velopack::locator::VelopackLocatorConfig;

use super::*;
use crate::updates::serving::{Reply, Server, serve};
use crate::updates::signing::{key_pair, public};

/// Long enough for a look at a server on this Mac.
const LOOK_TIMEOUT: Duration = Duration::from_secs(10);

/// What `vpk` puts in an app it packages, as this test's app at 0.1.5.
const MANIFEST: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<package><metadata><id>Roundhouse</id><version>0.1.5</version><channel>osx</channel><mainExe>roundhouse-desktop</mainExe></metadata></package>"#;

/// An app Velopack installed at 0.1.5, in a directory of its own, and where it keeps downloaded packages.
fn installed() -> (tempfile::TempDir, VelopackLocatorConfig) {
    let dir = tempfile::tempdir().unwrap();
    let bundle = dir.path().join("Roundhouse.app");
    let binaries = bundle.join("Contents/MacOS");
    std::fs::create_dir_all(&binaries).unwrap();
    std::fs::write(binaries.join("UpdateMac"), "").unwrap();
    std::fs::write(binaries.join("sq.version"), MANIFEST).unwrap();
    let config = VelopackLocatorConfig {
        RootAppDir: bundle,
        UpdateExePath: binaries.join("UpdateMac"),
        PackagesDir: dir.path().join("packages"),
        ManifestPath: binaries.join("sq.version"),
        CurrentBinaryDir: binaries,
        IsPortable: true,
    };
    (dir, config)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut text, byte| {
        write!(text, "{byte:02X}").unwrap();
        text
    })
}

fn package_name(version: &str) -> String {
    format!("Roundhouse-{version}-osx-full.nupkg")
}

/// Publishes a full package for each release, in a feed `signer` signs, and serves each package as its reply.
fn publish(releases: &[(&str, &[u8], Reply)], signer: &Ed25519KeyPair) -> Server {
    let assets: Vec<serde_json::Value> = releases
        .iter()
        .map(|(version, package, _)| {
            serde_json::json!({
                "PackageId": "Roundhouse",
                "Version": version,
                "Type": "Full",
                "FileName": package_name(version),
                "SHA1": hex(digest(&SHA1_FOR_LEGACY_USE_ONLY, package).as_ref()),
                "SHA256": hex(digest(&SHA256, package).as_ref()),
                "Size": package.len(),
            })
        })
        .collect();
    let feed = serde_json::to_vec(&serde_json::json!({ "Assets": assets })).unwrap();
    let signature = signer.sign(&feed).as_ref().to_vec();
    let mut paths = vec![
        ("releases.osx.json".to_string(), vec![Reply::file(feed)]),
        ("releases.osx.json.sig".to_string(), vec![Reply::file(signature)]),
    ];
    paths.extend(releases.iter().map(|(version, _, reply)| (package_name(version), vec![reply.clone()])));
    serve(paths)
}

/// The app's updater, set up as `start` sets it up, reading from `base`.
fn manager(base: &str, key: UpdateFeedKey, installed: &VelopackLocatorConfig) -> UpdateManager {
    let source = SignedSource::waiting(base, key, Duration::ZERO);
    UpdateManager::new(source, Some(options()), Some(installed.clone())).unwrap()
}

fn downloaded(installed: &VelopackLocatorConfig) -> Vec<String> {
    let found = std::fs::read_dir(&installed.PackagesDir).into_iter().flatten();
    let mut names: Vec<String> = found.map(|entry| entry.unwrap().file_name().to_string_lossy().to_string()).collect();
    names.retain(|name| Path::new(name).extension().is_some_and(|extension| extension == "nupkg"));
    names
}

#[test]
fn a_newer_release_in_a_signed_feed_downloads_and_passes_its_checks() {
    let (_dir, installed) = installed();
    let pair = key_pair();
    let new = b"0.1.6, packaged".as_slice();
    let server = publish(&[("0.1.5", b"", Reply::file("")), ("0.1.6", new, Reply::file(new))], &pair);

    let found = newest(&manager(&server.base, public(&pair), &installed)).unwrap().unwrap();

    assert_eq!(found.TargetFullRelease.Version, "0.1.6");
    assert_eq!(std::fs::read(installed.PackagesDir.join(package_name("0.1.6"))).unwrap(), new);
}

#[test]
fn a_feed_signed_with_another_key_is_refused_and_nothing_is_downloaded() {
    let (_dir, installed) = installed();
    let new = b"0.1.6, packaged".as_slice();
    let server = publish(&[("0.1.6", new, Reply::file(new))], &key_pair());

    let problem = newest(&manager(&server.base, public(&key_pair()), &installed)).unwrap_err();

    assert_eq!(problem.kind, Kind::Refused(Refusal::Unsigned));
    assert!(server.asked().iter().all(|path| !path.contains(".nupkg")), "{:?}", server.asked());
    assert!(downloaded(&installed).is_empty());
}

#[test]
fn a_feed_of_older_releases_offers_nothing() {
    let (_dir, installed) = installed();
    let pair = key_pair();
    let server = publish(&[("0.1.4", b"0.1.4, packaged", Reply::file("0.1.4, packaged"))], &pair);

    assert!(newest(&manager(&server.base, public(&pair), &installed)).unwrap().is_none());
    assert!(downloaded(&installed).is_empty());
}

#[test]
fn a_package_changed_from_the_one_the_feed_names_is_refused() {
    let (_dir, installed) = installed();
    let pair = key_pair();
    let server = publish(&[("0.1.6", b"0.1.6, packaged", Reply::file("0.1.6, changed!"))], &pair);

    let problem = newest(&manager(&server.base, public(&pair), &installed)).unwrap_err();

    assert_eq!(problem.kind, Kind::Refused(Refusal::NotAsListed));
    assert!(downloaded(&installed).is_empty());
}

#[test]
fn a_package_longer_than_the_feed_says_is_refused() {
    let (_dir, installed) = installed();
    let pair = key_pair();
    let server = publish(&[("0.1.6", b"0.1.6, packaged", Reply::file("0.1.6, packaged, and then some"))], &pair);

    let problem = newest(&manager(&server.base, public(&pair), &installed)).unwrap_err();

    assert_eq!(problem.kind, Kind::Refused(Refusal::TooLong));
    assert!(downloaded(&installed).is_empty());
}

#[test]
fn a_server_that_isnt_there_is_out_of_reach() {
    let (_dir, installed) = installed();
    // Bound and closed again, so nothing listens there.
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();

    let problem = newest(&manager(&format!("http://127.0.0.1:{port}"), public(&key_pair()), &installed)).unwrap_err();

    assert_eq!(problem.kind, Kind::Unreachable);
}

/// How many times the server was asked for the update feed, which is once a look.
fn feeds_read(server: &Server) -> usize {
    server.asked().iter().filter(|path| *path == "releases.osx.json").count()
}

#[test]
fn asking_looks_again_without_waiting_for_the_hour() {
    let (_dir, installed) = installed();
    let pair = key_pair();
    let server = publish(&[("0.1.5", b"", Reply::file(""))], &pair);
    let manager = manager(&server.base, public(&pair), &installed);
    let state = tempfile::tempdir().unwrap();
    let (look_now, asked) = flume::bounded(1);
    let (looks, looked) = flume::unbounded();
    std::thread::spawn(move || keep_looking(&manager, &asked, &looks, &OutOfReach::in_dir(state.path())));
    let (on_launch, _) = looked.recv_timeout(LOOK_TIMEOUT).unwrap();
    assert!(on_launch.unwrap().is_none());

    look_now.send(()).unwrap();

    let (asked_for, _) = looked.recv_timeout(LOOK_TIMEOUT).unwrap();
    assert!(asked_for.unwrap().is_none());
    assert_eq!(feeds_read(&server), 2);
}

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

/// Has the stand-in for Velopack's updater write down what it was asked to do, and returns where.
fn updater_that_notes(installed: &VelopackLocatorConfig) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt as _;
    let noted = installed.RootAppDir.with_file_name("asked");
    std::fs::write(&installed.UpdateExePath, format!("#!/bin/sh\necho \"$@\" > '{}'\n", noted.display())).unwrap();
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
