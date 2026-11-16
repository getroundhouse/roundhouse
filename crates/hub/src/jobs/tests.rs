use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::sync::mpsc;

use super::*;

const LIMITS: Limits = Limits { work: Duration::from_secs(60), after: Duration::from_secs(10) };
const KIND: JobKind = JobKind::ReadPullRequestInTurn;

/// Sets its flag once it's dropped, as a job's work is when it's stopped.
struct Dropped(Arc<AtomicBool>);

impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// An after-step that says how the work ended.
fn telling(ends: &mpsc::UnboundedSender<Ended>) -> impl FnOnce(Ended) -> std::future::Ready<()> + Send + 'static {
    let ends = ends.clone();
    move |ended| {
        ends.send(ended).unwrap();
        std::future::ready(())
    }
}

/// The next way a job ended, failing rather than hanging if none does.
async fn next(ended: &mut mpsc::UnboundedReceiver<Ended>) -> Ended {
    tokio::time::timeout(Duration::from_secs(600), ended.recv()).await.unwrap().unwrap()
}

/// Waits until `done`, on the test's paused clock, which moves on while it does.
async fn until(done: impl Fn() -> bool) {
    tokio::time::timeout(Duration::from_secs(600), async {
        while !done() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test(start_paused = true)]
async fn work_still_running_at_its_limit_is_stopped_where_it_said_it_was_and_its_key_is_free_again() {
    let jobs = Jobs::new();
    let (ends, mut ended) = mpsc::unbounded_channel();
    let dropped = Arc::new(AtomicBool::new(false));
    let flag = dropped.clone();

    jobs.start(
        KIND,
        "w",
        LIMITS,
        async move {
            let _dropped = Dropped(flag);
            step("waiting on a worker that never answers");
            std::future::pending::<()>().await;
        },
        move |ended| {
            // As the after-step finds it.
            ends.send((ended, dropped.load(Ordering::SeqCst))).unwrap();
            std::future::ready(())
        },
    )
    .unwrap();

    let (how, gone) = tokio::time::timeout(Duration::from_secs(600), ended.recv()).await.unwrap().unwrap();
    assert_eq!(how, Ended::Overran { at: "waiting on a worker that never answers".into(), began_uncut: false });
    assert!(gone, "the work was gone before its after-step ran");
    until(|| !jobs.busy().contains(&"w")).await;
    assert!(jobs.has_room(KIND));
}

#[tokio::test(start_paused = true)]
async fn what_a_job_runs_uncut_goes_on_to_its_end_and_its_after_step_runs_after_it() {
    let jobs = Jobs::new();
    let (ends, mut ended) = mpsc::unbounded_channel();
    let finished = Arc::new(AtomicBool::new(false));
    let (flag, seen) = (finished.clone(), finished.clone());

    jobs.start(
        KIND,
        "w",
        LIMITS,
        async move {
            uncut(async move {
                // Past the work's limit, and within the after-step's.
                tokio::time::sleep(LIMITS.work + LIMITS.after / 2).await;
                flag.store(true, Ordering::SeqCst);
            })
            .await;
        },
        move |ended| {
            // As the after-step finds it.
            ends.send((ended, seen.load(Ordering::SeqCst))).unwrap();
            std::future::ready(())
        },
    )
    .unwrap();

    let (how, uncut_ended) = tokio::time::timeout(Duration::from_secs(600), ended.recv()).await.unwrap().unwrap();
    assert!(matches!(how, Ended::Overran { began_uncut: true, .. }), "{how:?}");
    assert!(how.finished_or_ran_uncut());
    assert!(uncut_ended, "the after-step ran once what the work ran uncut had ended");
    assert!(finished.load(Ordering::SeqCst));
}

#[tokio::test(start_paused = true)]
async fn a_job_about_a_busy_key_or_past_what_its_kind_may_run_at_once_isnt_started() {
    let jobs = Jobs::new();
    let (ends, mut ended) = mpsc::unbounded_channel();
    let (finish, finished) = tokio::sync::oneshot::channel::<()>();
    jobs.start(
        KIND,
        "w",
        LIMITS,
        async move {
            let _ = finished.await;
        },
        telling(&ends),
    )
    .unwrap();

    let same_key = jobs.start(JobKind::ReadWaitedPullRequests, "w", LIMITS, async {}, telling(&ends));
    let same_kind = jobs.start(KIND, "v", LIMITS, async {}, telling(&ends));
    let other_kind = jobs.start(JobKind::ReadWaitedPullRequests, "v", LIMITS, async {}, telling(&ends));

    assert_eq!(same_key, Err(NotStarted::KeyBusy));
    assert_eq!(same_kind, Err(NotStarted::KindBusy));
    assert_eq!(other_kind, Ok(()));
    finish.send(()).unwrap();
    assert_eq!([next(&mut ended).await, next(&mut ended).await], [Ended::Finished, Ended::Finished]);
    until(|| !jobs.busy().contains(&"w")).await;
    assert!(jobs.has_room(KIND));
}

#[tokio::test(start_paused = true)]
async fn a_job_that_panics_ends_alone_and_its_after_step_still_runs() {
    let jobs = Jobs::new();
    let (ends, mut ended) = mpsc::unbounded_channel();

    jobs.start(
        KIND,
        "w",
        LIMITS,
        async move {
            step("reading");
            panic!("a bug in a job");
        },
        telling(&ends),
    )
    .unwrap();
    jobs.start(JobKind::ReadWaitedPullRequests, "v", LIMITS, async {}, telling(&ends)).unwrap();

    let mut how = [next(&mut ended).await, next(&mut ended).await];
    how.sort_by_key(|ended| matches!(ended, Ended::Finished));
    assert_eq!(how, [Ended::Panicked { at: "reading".into() }, Ended::Finished]);
    until(|| !jobs.busy().contains(&"w")).await;
}

#[tokio::test(start_paused = true)]
async fn stopping_ends_every_job_where_it_is_without_its_after_step_and_starts_none_after() {
    let jobs = Jobs::new();
    let (ends, mut ended) = mpsc::unbounded_channel();
    let dropped = Arc::new(AtomicBool::new(false));
    let flag = dropped.clone();
    jobs.start(
        KIND,
        "w",
        LIMITS,
        async move {
            let _dropped = Dropped(flag);
            std::future::pending::<()>().await;
        },
        telling(&ends),
    )
    .unwrap();
    tokio::task::yield_now().await;

    jobs.stop();

    until(|| dropped.load(Ordering::SeqCst)).await;
    let after = jobs.start(KIND, "v", LIMITS, async {}, telling(&ends));
    drop(ends);
    assert_eq!(ended.recv().await, None, "no after-step ran");
    assert_eq!(after, Err(NotStarted::Stopped));
    assert!(jobs.busy().is_empty());
}

#[tokio::test(start_paused = true)]
async fn an_after_step_still_running_at_its_own_limit_is_stopped_and_frees_the_key() {
    let jobs = Jobs::<&'static str>::new();
    let dropped = Arc::new(AtomicBool::new(false));
    let flag = dropped.clone();

    jobs.start(KIND, "w", LIMITS, async {}, move |_| async move {
        let _dropped = Dropped(flag);
        std::future::pending::<()>().await;
    })
    .unwrap();

    until(|| !jobs.busy().contains(&"w")).await;
    assert!(dropped.load(Ordering::SeqCst));
}

/// Called off as it waits, and then as what it runs uncut runs, which goes on to its end before the after-step.
#[tokio::test(start_paused = true)]
async fn a_job_called_off_ends_where_it_is_and_its_after_step_runs_once_what_it_runs_uncut_has_ended() {
    let jobs = Jobs::new();
    let (ends, mut ended) = mpsc::unbounded_channel();
    let finished = Arc::new(AtomicBool::new(false));
    let (flag, seen) = (finished.clone(), finished.clone());
    let (began, beginning) = tokio::sync::oneshot::channel();
    let waiting = async {
        step("waiting on GitHub");
        std::future::pending::<()>().await;
    };
    jobs.start(KIND, "w", LIMITS, waiting, telling(&ends)).unwrap();
    let keeping = async move {
        uncut(async move {
            began.send(()).unwrap();
            tokio::time::sleep(Duration::from_secs(1)).await;
            flag.store(true, Ordering::SeqCst);
        })
        .await;
    };
    let tell = telling(&ends);
    let after = move |ended: Ended| {
        // As the after-step finds it.
        assert!(seen.load(Ordering::SeqCst), "{ended:?} ran before what was uncut ended");
        tell(ended)
    };
    jobs.start(JobKind::ReadWaitedPullRequests, "v", LIMITS, keeping, after).unwrap();
    tokio::task::yield_now().await;

    jobs.call_off(&"w");
    beginning.await.unwrap();
    jobs.call_off(&"v");
    jobs.call_off(&"nobody");

    assert_eq!(next(&mut ended).await, Ended::CalledOff { at: "waiting on GitHub".into(), began_uncut: false });
    let how = next(&mut ended).await;
    assert!(matches!(how, Ended::CalledOff { began_uncut: true, .. }), "{how:?}");
    assert!(how.finished_or_ran_uncut());
    until(|| jobs.busy().is_empty()).await;
}
