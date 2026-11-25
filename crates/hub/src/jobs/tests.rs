use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use logging::testing::Logs;
use serde_json::json;
use tokio::sync::mpsc;
use tracing::info;

use super::*;

const LIMITS: Limits = Limits { work: Duration::from_secs(60), after: Duration::from_secs(10) };
const KIND: JobKind = JobKind::ReadOwnPullRequests;

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

/// Waits for a job's work to say it has got to where a test needs it, failing with `what` rather than hanging if it
/// never does.
async fn reached(said: tokio::sync::oneshot::Receiver<()>, what: &str) {
    let reached = tokio::time::timeout(Duration::from_secs(600), said).await;
    assert!(matches!(reached, Ok(Ok(()))), "{what}");
}

/// Waits until `done`, on the test's paused clock, which moves on while it does.
async fn until(done: impl Fn() -> bool) {
    waiting::task::until(Duration::from_secs(600), "timed out waiting for the jobs", done).await;
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
#[expect(
    clippy::disallowed_methods,
    reason = "tokio's clock is paused, so the work sleeping moves it past the work's limit"
)]
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
    let (finish, _) = tokio::sync::broadcast::channel::<()>(1);
    // As many as the kind may run at once, the first about `w`.
    let keys: Vec<String> = (0..KIND.at_once()).map(|n| if n == 0 { "w".into() } else { format!("k{n}") }).collect();
    for key in &keys {
        let mut finished = finish.subscribe();
        jobs.start(KIND, key.clone(), LIMITS, async move { drop(finished.recv().await) }, telling(&ends)).unwrap();
    }

    let same_key = jobs.start(JobKind::LookUpModels, "w".into(), LIMITS, async {}, telling(&ends));
    let same_kind = jobs.start(KIND, "v".into(), LIMITS, async {}, telling(&ends));
    let other_kind = jobs.start(JobKind::LookUpModels, "v".into(), LIMITS, async {}, telling(&ends));

    assert_eq!(same_key, Err(NotStarted::KeyBusy));
    assert_eq!(same_kind, Err(NotStarted::KindBusy));
    assert_eq!(other_kind, Ok(()));
    finish.send(()).unwrap();
    for _ in 0..=keys.len() {
        assert_eq!(next(&mut ended).await, Ended::Finished);
    }
    until(|| !jobs.busy().contains(&"w".to_string())).await;
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
    jobs.start(JobKind::LookUpModels, "v", LIMITS, async {}, telling(&ends)).unwrap();

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
    let (began, beginning) = tokio::sync::oneshot::channel();
    jobs.start(
        KIND,
        "w",
        LIMITS,
        async move {
            let _dropped = Dropped(flag);
            began.send(()).unwrap();
            std::future::pending::<()>().await;
        },
        telling(&ends),
    )
    .unwrap();
    reached(beginning, "the job never began").await;

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
#[expect(
    clippy::disallowed_methods,
    reason = "tokio's clock is paused, so the work sleeping moves it on while the job is called off"
)]
async fn a_job_called_off_ends_where_it_is_and_its_after_step_runs_once_what_it_runs_uncut_has_ended() {
    let jobs = Jobs::new();
    let (ends, mut ended) = mpsc::unbounded_channel();
    let finished = Arc::new(AtomicBool::new(false));
    let (flag, seen) = (finished.clone(), finished.clone());
    let (began, beginning) = tokio::sync::oneshot::channel();
    let (stepped, at_its_step) = tokio::sync::oneshot::channel();
    let waiting = async move {
        step("waiting on GitHub");
        stepped.send(()).unwrap();
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
    jobs.start(JobKind::LookUpModels, "v", LIMITS, keeping, after).unwrap();
    reached(at_its_step, "the job never got to its step").await;

    jobs.call_off(&"w");
    reached(beginning, "what the job runs uncut never began").await;
    jobs.call_off(&"v");
    jobs.call_off(&"nobody");

    assert_eq!(next(&mut ended).await, Ended::CalledOff { at: "waiting on GitHub".into(), began_uncut: false });
    let how = next(&mut ended).await;
    assert!(matches!(how, Ended::CalledOff { began_uncut: true, .. }), "{how:?}");
    assert!(how.finished_or_ran_uncut());
    until(|| jobs.busy().is_empty()).await;
}

/// Asked for several times while one of its kind about its key runs, a job of a kind that runs once more does, once,
/// as the last asked for, once the one running and its after-step have ended, while a job of another kind about the
/// key is refused. One asked for again goes with the job about its key when that's called off.
#[tokio::test(start_paused = true)]
async fn a_job_asked_for_again_while_one_about_its_key_runs_runs_once_more_after_it() {
    const ONCE_MORE: JobKind = JobKind::ReadWorkspaceChanges;
    let jobs = Jobs::new();
    let (ends, mut ended) = mpsc::unbounded_channel();
    let happened = Arc::new(parking_lot::Mutex::new(Vec::<String>::new()));
    let saying = |what: String| {
        let happened = happened.clone();
        async move { happened.lock().push(what) }
    };
    let (finish, finished) = tokio::sync::oneshot::channel::<()>();
    let (tell, after_step) = (telling(&ends), saying("the first's after-step".into()));
    let after = move |ended| async move {
        after_step.await;
        tell(ended).await;
    };
    jobs.start(ONCE_MORE, "w", LIMITS, async move { drop(finished.await) }, after).unwrap();

    let asked: Vec<_> =
        (1..=3).map(|n| jobs.start(ONCE_MORE, "w", LIMITS, saying(format!("asked {n}")), telling(&ends))).collect();
    let other_kind = jobs.start(KIND, "w", LIMITS, async {}, telling(&ends));
    finish.send(()).unwrap();

    assert_eq!(asked, [Ok(()); 3]);
    assert_eq!(other_kind, Err(NotStarted::KeyBusy));
    assert_eq!([next(&mut ended).await, next(&mut ended).await], [Ended::Finished, Ended::Finished]);
    until(|| jobs.busy().is_empty()).await;
    assert_eq!(*happened.lock(), ["the first's after-step", "asked 3"]);

    let (began, beginning) = tokio::sync::oneshot::channel();
    let waiting = async move {
        began.send(()).unwrap();
        std::future::pending::<()>().await;
    };
    jobs.start(ONCE_MORE, "v", LIMITS, waiting, telling(&ends)).unwrap();
    jobs.start(ONCE_MORE, "v", LIMITS, saying("asked again".into()), telling(&ends)).unwrap();
    reached(beginning, "the job never began").await;

    jobs.call_off(&"v");

    let how = next(&mut ended).await;
    assert!(matches!(how, Ended::CalledOff { .. }), "{how:?}");
    until(|| jobs.busy().is_empty()).await;
    drop(ends);
    assert_eq!(ended.recv().await, None, "nothing else ran");
    assert_eq!(*happened.lock(), ["the first's after-step", "asked 3"]);
}

#[tokio::test(start_paused = true)]
async fn what_a_job_logs_says_which_it_is_and_so_does_what_it_runs_uncut() {
    let logs = Logs::capture();
    let jobs = Jobs::new();
    let (ends, mut ended) = mpsc::unbounded_channel();

    let work = async {
        info!(name: "a.job", "Its work");
        uncut(async { info!(name: "a.job", "What it runs uncut") }).await;
    };
    jobs.start(KIND, "w", LIMITS, work, telling(&ends)).unwrap();
    next(&mut ended).await;

    let job = json!({ "kind": KIND.to_string(), "key": "w" });
    let contexts: Vec<_> = logs.events().into_iter().map(|event| event["context"].clone()).collect();
    assert_eq!(contexts, [job.clone(), job]);
}
