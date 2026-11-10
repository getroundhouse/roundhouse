//! The program that keeps the machine awake, started, watched and stopped for real, with a stand-in for it. When it's
//! started and stopped is [`Timing`]'s, which is tested without one.

use std::path::Path;

use process::testing::is_running;

use super::*;

const TIMEOUT: Duration = Duration::from_secs(10);
/// Longer than any test runs, so nothing here happens by the time passing.
const NEVER: Delays = Delays { linger: Duration::from_secs(3600), retry: Duration::from_secs(3600) };
const AT_ONCE: Delays = Delays { linger: Duration::ZERO, retry: Duration::ZERO };

/// Something to keep the machine awake with that says which process it is, in `pid_file`, each time it starts.
fn stand_in(pid_file: &Path) -> Stimulant {
    let script = r#"echo $$ > "$0"; exec sleep 3600"#;
    let args = ["-c", script, &pid_file.display().to_string()].map(str::to_string).to_vec();
    Stimulant { program: PathBuf::from("/bin/sh"), args }
}

async fn wait_for<T>(what: &str, mut found: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(found) = found() {
            return found;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// The stand-in a keeper runs, and whether there's work for it, as a test says.
struct Kept {
    awake: Arc<Awake>,
    at_work: Arc<AtomicBool>,
    pid_file: PathBuf,
    keeper: JoinHandle<()>,
    _dir: tempfile::TempDir,
}

impl Kept {
    fn start(delays: Delays) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("pid");
        let (awake, at_work) = (Arc::new(Awake::default()), Arc::new(AtomicBool::new(false)));
        let asked = at_work.clone();
        let keeper =
            tokio::spawn(hold_while(awake.clone(), move || asked.load(Ordering::SeqCst), stand_in(&pid_file), delays));
        Self { awake, at_work, pid_file, keeper, _dir: dir }
    }

    fn work(&self, going: bool) {
        self.at_work.store(going, Ordering::SeqCst);
        self.awake.work_changed();
    }

    /// The stand-in's PID, once one other than `before` is running and the keeper says it's keeping the machine awake.
    async fn started_after(&self, before: Option<u32>) -> u32 {
        wait_for("it to start", || {
            let pid: u32 = std::fs::read_to_string(&self.pid_file).ok()?.trim().parse().ok()?;
            (Some(pid) != before && is_running(pid) && self.awake.holding()).then_some(pid)
        })
        .await
    }

    async fn stopped(&self, pid: u32) {
        wait_for("it to stop", || (!is_running(pid) && !self.awake.holding()).then_some(())).await;
    }
}

#[tokio::test]
async fn its_started_for_work_and_stopped_once_the_work_is_over() {
    let kept = Kept::start(AT_ONCE);
    assert!(!kept.awake.holding(), "nothing is at work yet");

    kept.work(true);
    let pid = kept.started_after(None).await;

    kept.work(false);
    kept.stopped(pid).await;
}

#[tokio::test]
async fn its_stopped_as_soon_as_its_not_wanted_and_started_again_once_it_is() {
    let kept = Kept::start(NEVER);
    kept.work(true);
    let pid = kept.started_after(None).await;

    kept.awake.want(false);
    kept.stopped(pid).await;

    kept.awake.want(true);
    kept.started_after(Some(pid)).await;
}

#[tokio::test]
async fn one_that_something_else_stops_is_started_again_without_anything_changing() {
    let kept = Kept::start(AT_ONCE);
    kept.work(true);
    let first = kept.started_after(None).await;

    std::process::Command::new("kill").args(["-9", &first.to_string()]).status().unwrap();

    kept.started_after(Some(first)).await;
    assert!(!is_running(first));
}

#[tokio::test]
async fn nothing_outlives_whatever_was_keeping_it() {
    let kept = Kept::start(NEVER);
    kept.work(true);
    let pid = kept.started_after(None).await;

    kept.keeper.abort();

    wait_for("it to stop", || (!is_running(pid)).then_some(())).await;
}

#[test]
fn only_a_mac_has_something_to_keep_it_awake_with() {
    let stimulant = Stimulant::for_this_machine();

    assert_eq!(stimulant.is_some(), cfg!(target_os = "macos"));
    if let Some(stimulant) = stimulant {
        assert_eq!(stimulant.program, Path::new("/usr/bin/caffeinate"));
        assert_eq!(stimulant.args, ["-i", "-w", &std::process::id().to_string()]);
    }
}
