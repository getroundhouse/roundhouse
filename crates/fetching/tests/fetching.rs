//! Fetching from a local server that answers as each test tells it.

#![cfg(test)]

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use fetching::serving::{Event, Reply, serve};
use fetching::{CheckError, Expected, FetchError, Fetched, Fetcher, FolderLock};
use ring::digest::{SHA256, digest};

fn fetcher() -> Fetcher {
    Fetcher::new(Duration::from_secs(10))
}

#[test]
fn a_body_comes_decompressed() {
    let server = serve(vec![("feed".to_string(), vec![Reply::gzipped(b"the feed").unwrap()])]).unwrap();

    assert_eq!(fetcher().bytes(&format!("{}/feed", server.base), 100).unwrap(), b"the feed");
}

#[test]
fn a_body_longer_than_the_limit_once_decompressed_is_refused() {
    let server = serve(vec![("feed".to_string(), vec![Reply::gzipped(&vec![b' '; 1001]).unwrap()])]).unwrap();

    let fetched = fetcher().bytes(&format!("{}/feed", server.base), 1000);

    assert!(matches!(fetched, Err(FetchError::TooLong { limit_bytes: 1000, .. })), "{fetched:?}");
}

/// Were the rest read, the connection's end would be what stopped it.
#[test]
fn a_body_past_the_limit_is_refused_without_reading_the_rest() {
    let server = serve(vec![("feed".to_string(), vec![Reply::cut_off(vec![b' '; 1001], 1_000_000)])]).unwrap();

    let fetched = fetcher().bytes(&format!("{}/feed", server.base), 1000);

    assert!(matches!(fetched, Err(FetchError::TooLong { limit_bytes: 1000, .. })), "{fetched:?}");
}

#[test]
fn a_body_cut_off_in_transit_cant_be_read() {
    let server = serve(vec![("feed".to_string(), vec![Reply::cut_off("12345", 10)])]).unwrap();

    let fetched = fetcher().bytes(&format!("{}/feed", server.base), 100);

    assert!(matches!(fetched, Err(FetchError::Read { .. })), "{fetched:?}");
}

#[test]
fn an_answer_other_than_the_whole_file_is_refused() {
    let server = serve(vec![("feed".to_string(), vec![Reply::partial("the feed")])]).unwrap();

    let fetched = fetcher().bytes(&format!("{}/feed", server.base), 100);

    assert!(matches!(fetched, Err(FetchError::Status { status: 206, .. })), "{fetched:?}");
}

#[test]
fn a_body_as_long_as_the_limit_is_taken() {
    let server = serve(vec![("feed".to_string(), vec![Reply::file(vec![b' '; 1000])])]).unwrap();

    assert_eq!(fetcher().bytes(&format!("{}/feed", server.base), 1000).unwrap().len(), 1000);
}

#[test]
fn a_redirect_is_an_answer_and_not_followed() {
    let elsewhere = serve(vec![("feed".to_string(), vec![Reply::file("the feed")])]).unwrap();
    let server = serve(vec![("feed".to_string(), vec![Reply::redirect(&format!("{}/feed", elsewhere.base))])]).unwrap();

    let fetched = fetcher().bytes(&format!("{}/feed", server.base), 100);

    assert!(matches!(fetched, Err(FetchError::Status { status: 302, .. })), "{fetched:?}");
    assert!(elsewhere.asked().is_empty());
}

#[test]
fn a_file_that_isnt_there_says_so_and_names_its_address() {
    let server = serve(vec![]).unwrap();
    let url = format!("{}/feed", server.base);

    let fetched = fetcher().bytes(&url, 100);

    assert!(matches!(&fetched, Err(FetchError::Status { url: named, status: 404 }) if *named == url), "{fetched:?}");
}

#[test]
fn a_server_that_isnt_there_says_so_and_names_its_address() {
    let gone = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
    let url = format!("http://{gone}/feed");

    let fetched = fetcher().bytes(&url, 100);

    assert!(matches!(&fetched, Err(FetchError::Request { url: named, .. }) if *named == url), "{fetched:?}");
}

#[test]
fn a_file_is_written_whole() {
    let server = serve(vec![("package".to_string(), vec![Reply::gzipped(b"12345").unwrap()])]).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let to = dir.path().join("package");

    fetcher().to_file(&format!("{}/package", server.base), &to, 5).unwrap();

    assert_eq!(std::fs::read(to).unwrap(), b"12345");
}

#[test]
fn a_file_shorter_or_longer_than_it_should_be_is_refused() {
    let server = serve(vec![
        ("short".to_string(), vec![Reply::file("12345")]),
        ("long".to_string(), vec![Reply::file("123456")]),
    ])
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let to = dir.path().join("package");

    let short = fetcher().to_file(&format!("{}/short", server.base), &to, 6);
    let long = fetcher().to_file(&format!("{}/long", server.base), &to, 5);

    assert!(matches!(short, Err(FetchError::CutShort { expected_bytes: 6, got_bytes: 5, .. })), "{short:?}");
    assert!(matches!(long, Err(FetchError::TooLong { limit_bytes: 5, .. })), "{long:?}");
}

#[test]
fn a_file_that_cant_be_saved_names_where() {
    let server = serve(vec![("package".to_string(), vec![Reply::file("12345")])]).unwrap();
    let to = tempfile::tempdir().unwrap().path().join("gone").join("package");

    let fetched = fetcher().to_file(&format!("{}/package", server.base), &to, 5);

    assert!(matches!(&fetched, Err(FetchError::Save { path, .. }) if *path == to), "{fetched:?}");
}

/// A file made up for a test: `bytes` of a pattern, and what it should be.
fn made_up(bytes: usize) -> (Vec<u8>, Expected) {
    let body: Vec<u8> = (0..bytes).map(|i| u8::try_from(i % 251).unwrap()).collect();
    let sha256 = digest(&SHA256, &body).as_ref().try_into().unwrap();
    let expected = Expected { size_bytes: body.len() as u64, sha256 };
    (body, expected)
}

fn part(dir: &Path) -> PathBuf {
    dir.join("model/weights.bin.part")
}

/// Fetches `url` into `model/weights.bin` in `dir`, going on to the end.
fn checked(dir: &Path, url: &str, expected: &Expected) -> Result<Fetched, CheckError> {
    let lock = FolderLock::take(dir).unwrap();
    let mut on = |_| ControlFlow::Continue(());
    Fetcher::for_big_files(Duration::from_secs(10)).checked_file(
        &lock,
        url,
        Path::new("model/weights.bin"),
        expected,
        &mut on,
    )
}

#[test]
fn a_checked_file_is_fetched_and_put_in_place() {
    let (body, expected) = made_up(100_000);
    let server = serve(vec![("weights.bin".to_string(), vec![Reply::file(body.clone())])]).unwrap();
    let dir = tempfile::tempdir().unwrap();

    let fetched = checked(dir.path(), &format!("{}/weights.bin", server.base), &expected);

    assert_eq!(fetched.unwrap(), Fetched::Done);
    assert_eq!(std::fs::read(dir.path().join("model/weights.bin")).unwrap(), body);
    assert!(!part(dir.path()).exists());
}

#[test]
fn a_checked_file_goes_on_from_its_part_through_a_redirect() {
    let (body, expected) = made_up(50_000);
    let storage = serve(vec![("stored".to_string(), vec![Reply::partial(body[20_000..].to_vec())])]).unwrap();
    let server =
        serve(vec![("weights.bin".to_string(), vec![Reply::redirect(&format!("{}/stored", storage.base))])]).unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("model")).unwrap();
    std::fs::write(part(dir.path()), &body[..20_000]).unwrap();

    let fetched = checked(dir.path(), &format!("{}/weights.bin", server.base), &expected);

    assert_eq!(fetched.unwrap(), Fetched::Done);
    assert!(storage.requests()[0].to_lowercase().contains("range: bytes=20000-"), "{:?}", storage.requests());
    assert_eq!(std::fs::read(dir.path().join("model/weights.bin")).unwrap(), body);
}

#[test]
fn a_server_that_sends_the_whole_file_rather_than_the_rest_starts_it_again() {
    let (body, expected) = made_up(50_000);
    let server = serve(vec![("weights.bin".to_string(), vec![Reply::file(body.clone())])]).unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("model")).unwrap();
    std::fs::write(part(dir.path()), &body[..20_000]).unwrap();

    let fetched = checked(dir.path(), &format!("{}/weights.bin", server.base), &expected);

    assert_eq!(fetched.unwrap(), Fetched::Done);
    assert_eq!(std::fs::read(dir.path().join("model/weights.bin")).unwrap(), body);
}

#[test]
fn a_file_that_isnt_the_one_expected_is_thrown_away() {
    let (mut body, expected) = made_up(50_000);
    body[100] ^= 1;
    let server = serve(vec![("weights.bin".to_string(), vec![Reply::file(body)])]).unwrap();
    let dir = tempfile::tempdir().unwrap();

    let fetched = checked(dir.path(), &format!("{}/weights.bin", server.base), &expected);

    assert!(matches!(fetched, Err(CheckError::NotTheFile { .. })), "{fetched:?}");
    assert!(!dir.path().join("model/weights.bin").exists());
    assert!(!part(dir.path()).exists(), "so the next fetch starts it again");
}

#[test]
fn a_file_longer_than_it_should_be_isnt_the_file_and_is_thrown_away() {
    let (body, expected) = made_up(50_000);
    let server = serve(vec![("weights.bin".to_string(), vec![Reply::file([body, vec![0; 10]].concat())])]).unwrap();
    let dir = tempfile::tempdir().unwrap();

    let fetched = checked(dir.path(), &format!("{}/weights.bin", server.base), &expected);

    assert!(matches!(fetched, Err(CheckError::NotTheFile { .. })), "{fetched:?}");
    assert!(!dir.path().join("model/weights.bin").exists());
    assert!(!part(dir.path()).exists(), "so the next fetch starts it again");
}

#[test]
fn a_fetch_cut_short_keeps_what_came() {
    let (body, expected) = made_up(50_000);
    let server = serve(vec![("weights.bin".to_string(), vec![Reply::file(body[..30_000].to_vec())])]).unwrap();
    let dir = tempfile::tempdir().unwrap();

    let fetched = checked(dir.path(), &format!("{}/weights.bin", server.base), &expected);

    assert!(matches!(fetched, Err(CheckError::Fetch(FetchError::CutShort { got_bytes: 30_000, .. }))), "{fetched:?}");
    assert_eq!(std::fs::read(part(dir.path())).unwrap(), body[..30_000]);
}

#[test]
fn a_fetch_stopped_while_hashing_what_was_there_asks_for_nothing_and_keeps_it() {
    let (body, expected) = made_up(50_000);
    let server = serve(vec![("weights.bin".to_string(), vec![Reply::file(body.clone())])]).unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("model")).unwrap();
    std::fs::write(part(dir.path()), &body[..20_000]).unwrap();
    let lock = FolderLock::take(dir.path()).unwrap();

    let mut stop = |_| ControlFlow::Break(());
    let url = format!("{}/weights.bin", server.base);
    let fetched = Fetcher::for_big_files(Duration::from_secs(10)).checked_file(
        &lock,
        &url,
        Path::new("model/weights.bin"),
        &expected,
        &mut stop,
    );

    assert_eq!(fetched.unwrap(), Fetched::Stopped);
    assert!(server.asked().is_empty());
    assert_eq!(std::fs::read(part(dir.path())).unwrap(), body[..20_000]);
}

/// Waits until `done`, failing after a while.
fn wait_for(what: &str, done: impl Fn() -> bool) {
    waiting::thread::until(Duration::from_secs(10), format!("timed out waiting for {what}"), done);
}

/// A fetch of `url` into `dir`'s `model/weights.bin` on a thread of its own, holding `dir`'s lock, until it ends or
/// `stop` is set.
fn fetch_on_a_thread(
    dir: &Path,
    url: String,
    expected: Expected,
    stop: Arc<AtomicBool>,
) -> std::thread::JoinHandle<Result<Fetched, CheckError>> {
    let dir = dir.to_path_buf();
    std::thread::spawn(move || {
        let lock = FolderLock::take(&dir).unwrap();
        let mut on = |_| if stop.load(Ordering::Relaxed) { ControlFlow::Break(()) } else { ControlFlow::Continue(()) };
        let to = Path::new("model/weights.bin");
        Fetcher::for_big_files(Duration::from_secs(10)).checked_file(&lock, &url, to, &expected, &mut on)
    })
}

#[test]
fn a_stopped_fetch_keeps_what_came_and_the_next_waits_for_it_to_let_go() {
    let (body, expected) = made_up(200_000);
    let (first, release) = Reply::file(body.clone()).held_after(50_000);
    let server = serve(vec![("weights.bin".to_string(), vec![first, Reply::file(body.clone())])]).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let url = format!("{}/weights.bin", server.base);
    let size = || std::fs::metadata(part(dir.path())).map_or(0, |found| found.len());

    let stop = Arc::new(AtomicBool::new(false));
    let stopping = fetch_on_a_thread(dir.path(), url.clone(), expected, Arc::clone(&stop));
    wait_for("the first fetch to write what it was sent", || size() == 50_000);
    stop.store(true, Ordering::Relaxed);
    let next = fetch_on_a_thread(dir.path(), url, expected, Arc::default());
    release.now();

    assert_eq!(stopping.join().unwrap().unwrap(), Fetched::Stopped);
    assert_eq!(next.join().unwrap().unwrap(), Fetched::Done);
    let weights = Event::Asked("weights.bin".to_string());
    assert_eq!(server.events(), [weights.clone(), Event::Released("weights.bin".to_string()), weights]);
    assert!(server.requests()[1].to_lowercase().contains("range: bytes="), "the stopped fetch kept what came");
    assert_eq!(std::fs::read(dir.path().join("model/weights.bin")).unwrap(), body);
}

#[test]
fn a_folder_lock_keeps_any_other_out_until_it_goes() {
    let dir = tempfile::tempdir().unwrap();
    let lock = FolderLock::take(&dir.path().join("models")).unwrap();
    let other = std::fs::File::open(dir.path().join("models/.lock")).unwrap();

    assert!(matches!(other.try_lock(), Err(std::fs::TryLockError::WouldBlock)));
    drop(lock);
    other.try_lock().unwrap();
}

#[test]
fn a_fetch_that_stops_coming_times_out_and_keeps_what_came() {
    let (body, expected) = made_up(200_000);
    let (held, _held_until_the_test_ends) = Reply::file(body).held_after(50_000);
    let server = serve(vec![("weights.bin".to_string(), vec![held])]).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let lock = FolderLock::take(dir.path()).unwrap();
    let url = format!("{}/weights.bin", server.base);

    let stalled = Fetcher::for_big_files(Duration::from_millis(300));
    let fetched = stalled
        .checked_file(&lock, &url, Path::new("model/weights.bin"), &expected, &mut |_| ControlFlow::Continue(()));
    let Err(CheckError::Fetch(error)) = fetched else { panic!("{fetched:?}") };
    assert!(matches!(error, FetchError::Read { .. }) && error.timed_out(), "{error:?}");
    assert_eq!(std::fs::metadata(part(dir.path())).unwrap().len(), 50_000);
}

#[test]
fn only_a_wait_that_ran_out_timed_out() {
    let server = serve(vec![("there".to_string(), vec![Reply::file("not")])]).unwrap();
    let missing = fetcher().bytes(&format!("{}/elsewhere", server.base), 10).unwrap_err();
    let gone = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
    let refused = fetcher().bytes(&format!("http://{gone}/feed"), 10).unwrap_err();
    assert!(!missing.timed_out() && !refused.timed_out(), "{missing:?} {refused:?}");
}
