//! Clients watching a terminal, each sent what it lacks of the hub's copy, which pictures from a stand-in for the
//! terminal's worker make. On a paused clock, as `stand_in::settled` says.

use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::terminals::{TerminalRows, TerminalWatch};
use protocol::client::{ApiError, ClientRequest, HubFrame};
use protocol::terminal::{
    Numbering, PictureSending, RowJson, RowNumber, ScreenState, Shown, Span, TerminalExit, TerminalPicture,
    TerminalRow, TerminalRowsReply, TerminalSize, TerminalUpdate,
};
use protocol::worker::{WorkerRequest, WorkerResponse};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::timeout;

use super::super::watching::Watching;
use super::stand_in::{Connected, WAIT, settled};
use crate::hub::tests::{hub_with_worktree, serving, terminal_in_w};
use crate::hub::{Followed, Following, Hub};
use crate::server::Outgoing;
use crate::worker_link::WorkerLink;

fn row(text: &str) -> RowJson {
    RowJson::new(&TerminalRow { spans: vec![Span { text: text.into(), ..Span::default() }], ..TerminalRow::default() })
        .unwrap()
}

/// A screen of `rows`, its first numbered 200.
fn shown(rows: &[&str]) -> Shown {
    Shown {
        numbering: Numbering::new(1),
        size: TerminalSize { cols: 80, rows: u16::try_from(rows.len()).unwrap() },
        top: RowNumber::new(200),
        first: RowNumber::new(100),
        rows: rows.iter().map(|text| row(text)).collect(),
        cursor: None,
        state: ScreenState::default(),
    }
}

fn texts(picture: &TerminalPicture) -> Vec<(u64, String)> {
    let each = |run: &protocol::terminal::RowRun| {
        let numbers = run.from.get()..;
        numbers.zip(&run.rows).map(|(number, row)| (number, row.row().spans[0].text.clone())).collect::<Vec<_>>()
    };
    picture.rows.iter().flat_map(each).collect()
}

/// A hub with `t` open on a stand-in worker, which says what it's told of whether the terminal is watched, and sends
/// pictures of what the tests say it shows.
struct Worker {
    hub: Arc<Hub>,
    link: Arc<WorkerLink>,
    told: Arc<Mutex<Vec<bool>>>,
    sending: PictureSending,
}

impl Worker {
    async fn open(dir: &std::path::Path) -> Self {
        Self::answering_watch(dir, || Ok(WorkerResponse::Ok)).await
    }

    async fn answering_watch(
        dir: &std::path::Path,
        watch: impl Fn() -> Result<WorkerResponse, ApiError> + Send + 'static,
    ) -> Self {
        let hub = hub_with_worktree(dir);
        let told = Arc::new(Mutex::new(Vec::new()));
        let telling = told.clone();
        let link = WorkerLink::answering(move |request| {
            if let WorkerRequest::TerminalWatch(asked) = &request {
                telling.lock().push(asked.watching);
                return Some(watch());
            }
            if let WorkerRequest::TerminalRows(asked) = request {
                return Some(Ok(WorkerResponse::TerminalRows(TerminalRowsReply {
                    numbering: asked.numbering,
                    first: RowNumber::new(100),
                    top: RowNumber::new(200),
                    from: asked.from,
                    rows: vec![TerminalRow::default()],
                })));
            }
            Some(Ok(WorkerResponse::Ok))
        });
        serving(&hub, &link);
        hub.terminal_open(terminal_in_w("t")).await.unwrap();
        Self { hub, link, told, sending: PictureSending::default() }
    }

    /// Sends the hub every frame of the next picture of `now`, as the worker's sender would make it.
    fn shows(&mut self, now: Shown) {
        let first = self.sending.next(now, |_| None);
        for frame in first.into_iter().chain(std::iter::from_fn(|| self.sending.rest())) {
            self.hub.terminal_picture(&self.link, &"t".into(), frame);
        }
    }

    fn told(&self) -> Vec<bool> {
        self.told.lock().clone()
    }
}

/// A client watching `t`, and what it's sent, which it takes only when it reads it.
struct Viewer {
    watching: JoinHandle<()>,
    /// Its entry in the hub's list of who follows what, as its subscription's is.
    following: Following,
    sent: mpsc::Receiver<Outgoing>,
}

impl Viewer {
    fn watch(hub: &Arc<Hub>) -> Self {
        let watcher = hub.terminal_watch(&"t".into()).unwrap();
        let (frames, sent) = mpsc::channel(1024);
        let watching = tokio::spawn(Watching { subscription_id: 5, frames }.watch(watcher));
        Self { watching, following: hub.follow([Followed::Terminal("t".into())]), sent }
    }

    async fn next(&mut self) -> HubFrame {
        timeout(WAIT, self.sent.recv()).await.expect("a frame").expect("the watching to go on").frame
    }

    async fn picture(&mut self) -> TerminalPicture {
        let HubFrame::TerminalPicture { subscription_id: 5, picture } = self.next().await else {
            panic!("not a picture")
        };
        picture
    }

    /// How many frames are queued for it that it hasn't read.
    fn queued(&self) -> usize {
        self.sent.len()
    }

    /// Stops watching, as a client that unsubscribes does.
    async fn stop(self) {
        self.watching.abort();
        let _ = self.watching.await;
        drop(self.following);
        settled().await;
    }
}

/// The worker is told only as the first client begins to watch and the last stops, and a client that begins while
/// another watches is sent the hub's copy, with nothing asked of the worker.
#[tokio::test(start_paused = true)]
async fn the_worker_is_told_as_the_first_client_watches_and_the_last_stops() {
    let dir = tempfile::tempdir().unwrap();
    let mut worker = Worker::open(dir.path()).await;
    let first = Viewer::watch(&worker.hub);
    settled().await;
    worker.shows(shown(&["a", "b"]));
    settled().await;

    let mut second = Viewer::watch(&worker.hub);
    let sent_second = second.picture().await;
    let told_while_two = worker.told();
    first.stop().await;
    let told_while_one = worker.told();
    second.stop().await;

    assert!(sent_second.whole && sent_second.fresh, "{sent_second:?}");
    assert_eq!(texts(&sent_second), [(200, "a".into()), (201, "b".into())]);
    assert_eq!((told_while_two, told_while_one), (vec![true], vec![true]));
    assert_eq!(worker.told(), [true, false]);
}

/// Once no client watches, the hub lets go of its copy, which the worker stops keeping up, so a client that watches
/// later is shown nothing until the worker's fresh picture.
#[tokio::test(start_paused = true)]
async fn a_client_that_watches_after_all_stopped_waits_for_a_fresh_picture() {
    let dir = tempfile::tempdir().unwrap();
    let mut worker = Worker::open(dir.path()).await;
    let first = Viewer::watch(&worker.hub);
    settled().await;
    worker.shows(shown(&["old"]));
    settled().await;
    first.stop().await;

    let mut later = Viewer::watch(&worker.hub);
    settled().await;
    let queued = later.queued();
    worker.sending.reset();
    worker.shows(shown(&["new"]));
    let sent = later.picture().await;

    assert_eq!((queued, worker.told()), (0, vec![true, false, true]));
    assert!(sent.fresh, "{sent:?}");
    assert_eq!(texts(&sent), [(200, "new".into())]);
}

#[tokio::test(start_paused = true)]
async fn a_client_watching_is_sent_the_screen_whole_and_then_only_what_changes() {
    let dir = tempfile::tempdir().unwrap();
    let mut worker = Worker::open(dir.path()).await;
    let mut viewer = Viewer::watch(&worker.hub);
    settled().await;

    worker.shows(shown(&["a", "b", "c"]));
    let whole = viewer.picture().await;
    worker.shows(shown(&["a", "B", "c"]));
    let changed = viewer.picture().await;

    assert!(whole.whole && whole.fresh, "{whole:?}");
    assert_eq!(texts(&whole).len(), 3);
    assert!(changed.whole && !changed.fresh, "{changed:?}");
    assert_eq!(texts(&changed), [(201, "B".into())]);
}

/// However many pictures come while a client hasn't read the last frame it was sent, it has one frame queued, and is
/// sent the newest once it reads it, rather than any that came between.
#[tokio::test(start_paused = true)]
async fn a_slow_client_is_sent_one_frame_at_a_time_and_then_the_newest() {
    let dir = tempfile::tempdir().unwrap();
    let mut worker = Worker::open(dir.path()).await;
    let mut viewer = Viewer::watch(&worker.hub);
    settled().await;

    for n in 0..100 {
        worker.shows(shown(&["a", &n.to_string()]));
        settled().await;
    }
    let queued = viewer.queued();
    let first = viewer.picture().await;
    let newest = viewer.picture().await;
    settled().await;

    assert_eq!(queued, 1);
    assert_eq!(texts(&first), [(200, "a".into()), (201, "0".into())]);
    assert_eq!(texts(&newest), [(201, "99".into())]);
    assert_eq!(viewer.queued(), 0, "nothing of what came between");
}

/// Over a connection to a client that has stopped reading, a reply waits behind only what's being written and a frame
/// of the terminal, rather than every picture of a flood.
#[tokio::test(start_paused = true)]
async fn a_reply_to_a_slow_client_waits_behind_little_of_a_watched_terminal() {
    let dir = tempfile::tempdir().unwrap();
    let mut worker = Worker::open(dir.path()).await;
    let mut connected = Connected::to(&worker.hub, 16 * 1024).await;
    connected.send(1, ClientRequest::TerminalWatch(TerminalWatch { id: "t".into(), subscription_id: 7 })).await;
    settled().await;
    let wide = "x".repeat(1024);

    for n in 0..500 {
        let rows: Vec<String> = (0..24).map(|at| format!("{n} {at} {wide}")).collect();
        worker.shows(shown(&rows.iter().map(String::as_str).collect::<Vec<_>>()));
        if n % 50 == 0 {
            settled().await;
        }
    }
    connected.send(2, ClientRequest::HubStatus).await;
    settled().await;

    let mut before = 0;
    loop {
        match connected.next().await {
            HubFrame::Reply { id: 2, .. } => break,
            HubFrame::TerminalPicture { .. } => before += 1,
            HubFrame::Reply { .. }
            | HubFrame::Terminal { .. }
            | HubFrame::Event { .. }
            | HubFrame::Tail { .. }
            | HubFrame::Delta { .. }
            | HubFrame::Notice { .. }
            | HubFrame::Github { .. }
            | HubFrame::Listed { .. } => {}
        }
    }
    assert!(before <= 3, "{before} frames of pictures came first");
}

/// A client watching is told the shell ended, with how, after which nothing more comes.
#[tokio::test(start_paused = true)]
async fn a_client_watching_hears_its_shell_end() {
    let dir = tempfile::tempdir().unwrap();
    let mut worker = Worker::open(dir.path()).await;
    let mut viewer = Viewer::watch(&worker.hub);
    settled().await;
    worker.shows(shown(&["a"]));
    viewer.picture().await;

    let exited = TerminalUpdate::Exited(TerminalExit { code: Some(3) });
    worker.hub.terminal_update(&worker.link, "t".into(), &exited);

    let HubFrame::Terminal { subscription_id: 5, update } = viewer.next().await else { panic!("not its end") };
    assert_eq!(update, exited);
    timeout(WAIT, viewer.watching).await.unwrap().unwrap();
}

/// The shell's end comes after what it last showed, even to a client that hadn't read what came before.
#[tokio::test(start_paused = true)]
async fn a_client_watching_is_sent_what_the_shell_last_showed_before_its_end() {
    let dir = tempfile::tempdir().unwrap();
    let mut worker = Worker::open(dir.path()).await;
    let mut viewer = Viewer::watch(&worker.hub);
    settled().await;

    worker.shows(shown(&["a"]));
    settled().await;
    worker.shows(shown(&["last"]));
    let exited = TerminalUpdate::Exited(TerminalExit { code: Some(0) });
    worker.hub.terminal_update(&worker.link, "t".into(), &exited);
    settled().await;
    viewer.picture().await;
    let last = viewer.picture().await;

    assert_eq!(texts(&last), [(200, "last".into())]);
    let HubFrame::Terminal { update, .. } = viewer.next().await else { panic!("not its end") };
    assert_eq!(update, exited);
}

/// A client watching a terminal whose worker has gone, and its shell with it, is told it ended, with no code.
#[tokio::test(start_paused = true)]
async fn a_client_watching_hears_its_shell_end_with_its_worker() {
    let dir = tempfile::tempdir().unwrap();
    let mut worker = Worker::open(dir.path()).await;
    let mut viewer = Viewer::watch(&worker.hub);
    settled().await;
    worker.shows(shown(&["a"]));
    viewer.picture().await;

    worker.hub.terminals_lost(worker.link.id).await;

    let HubFrame::Terminal { update, .. } = viewer.next().await else { panic!("not its end") };
    assert_eq!(update, TerminalUpdate::Exited(TerminalExit { code: None }));
    timeout(WAIT, viewer.watching).await.unwrap().unwrap();
}

/// A picture the hub couldn't take in, as one made against a screen it didn't have, has the worker asked to begin
/// again, after which a client watching is shown what its fresh picture shows.
#[tokio::test(start_paused = true)]
async fn a_picture_the_hub_couldnt_take_in_has_the_worker_begin_again() {
    let dir = tempfile::tempdir().unwrap();
    let mut worker = Worker::open(dir.path()).await;
    let mut viewer = Viewer::watch(&worker.hub);
    settled().await;
    worker.shows(shown(&["a", "b"]));
    viewer.picture().await;

    let mut other = PictureSending::default();
    let _ = other.next(shown(&["a", "b", "c"]), |_| None);
    let lacking = other.next(shown(&["a", "b", "C"]), |_| None).unwrap();
    worker.hub.terminal_picture(&worker.link, &"t".into(), lacking);
    settled().await;
    let told = worker.told();
    worker.sending.reset();
    worker.shows(shown(&["new", "screen"]));
    let sent = viewer.picture().await;

    assert_eq!(told, [true, true]);
    assert_eq!(texts(&sent), [(200, "new".into()), (201, "screen".into())]);
}

/// Rows of history come from the worker, which the hub keeps none of.
#[tokio::test(start_paused = true)]
async fn rows_of_history_are_asked_of_the_worker() {
    let dir = tempfile::tempdir().unwrap();
    let worker = Worker::open(dir.path()).await;

    let asked = TerminalRows { id: "t".into(), numbering: Numbering::new(4), from: RowNumber::new(150), count: 10 };
    let rows = worker.hub.terminal_rows(asked).await.unwrap();

    assert_eq!((rows.numbering, rows.from, rows.rows.len()), (Numbering::new(4), RowNumber::new(150), 1));
}
