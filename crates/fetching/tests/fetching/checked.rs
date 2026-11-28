//! A big file checked against what it should be: fetched into its part, gone on with from there, thrown away when
//! it isn't the file, stopped, held to its folder's lock, and timed out.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use fetching::serving::{Event, Reply, serve};
use fetching::{CheckError, Expected, FetchError, Fetched, Fetcher, FolderLock};
use ring::digest::{SHA256, digest};

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

/// What a fetch on a thread of its own is told, and what it says.
#[derive(Default)]
struct Watched {
    /// Whether it's to stop, which it asks after each part of the file it writes.
    stop: AtomicBool,
    /// How much of the file it had written when it last asked and was told to go on.
    went_on_at: AtomicU64,
}

/// A fetch of `url` into `dir`'s `model/weights.bin` on a thread of its own, holding `dir`'s lock, until it ends or
/// it's told to stop.
fn fetch_on_a_thread(
    dir: &Path,
    url: String,
    expected: Expected,
    watched: Arc<Watched>,
) -> std::thread::JoinHandle<Result<Fetched, CheckError>> {
    let dir = dir.to_path_buf();
    std::thread::spawn(move || {
        let lock = FolderLock::take(&dir).unwrap();
        let mut on = |bytes| {
            if watched.stop.load(Ordering::Relaxed) {
                return ControlFlow::Break(());
            }
            // Stored once the stop is read, so a stop set after a test sees it is read no sooner than the next ask.
            watched.went_on_at.store(bytes, Ordering::Release);
            ControlFlow::Continue(())
        };
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

    let watched = Arc::new(Watched::default());
    let stopping = fetch_on_a_thread(dir.path(), url.clone(), expected, Arc::clone(&watched));
    // Until it has gone on past what it was sent, rather than until that's in the file, since it writes before it asks
    // whether to stop, and a stop it read then would end it before the server lets the rest go.
    wait_for("the first fetch to go on past what it was sent", || watched.went_on_at.load(Ordering::Acquire) == 50_000);
    watched.stop.store(true, Ordering::Relaxed);
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
