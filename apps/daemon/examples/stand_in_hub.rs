//! Stands in for a hub of another build, which a test can't otherwise get. As a client sees it across a real
//! protocol break, it refuses every hello, and all that still works is seeing it and stopping it. With
//! `--speaks-this-protocol` it takes a hello, as an older build would, and serves a hub with nothing in it, so the
//! app can use it, refuses `agent_list` as unsupported, as a build from before hubs described their agents would, and
//! refuses anything else. It holds the same lock as a real hub. A test can change how many agents
//! it says are running, as if some had started or finished, by writing the number to [`RUNNING`] in its data directory,
//! and how many clients are connected to it, by writing to [`CLIENTS`]. Asked to stop if it's idle, it goes by what it
//! says then, as a hub does, unless it's told to be from before a hub could decide that, or to turn out busy. With
//! `--keeps-workers` it says it keeps its workers, and asked to restart, it goes once no setup script is running, as a
//! hub does, as many as it says are until a test writes another number to [`SETUPS`], unless they're told to end as it's
//! asked, and leaves [`RESTARTED`] to say it restarted rather than stopped.

#![expect(clippy::unwrap_used, clippy::expect_used, reason = "a test program, so failing loudly is what it should do")]

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use parking_lot::Mutex;
use protocol::client::{
    ApiError, ClientFrame, ClientRequest, ClientResponse, ErrorCode, HubFrame, HubNotice, HubStatus, ReplyOutcome,
};
use protocol::model::{Records, Repo, RepoId, Rev};
use protocol::paths::Paths;

#[derive(Parser)]
#[expect(clippy::struct_excessive_bools, reason = "each is a flag of its own, which a test passes or leaves out")]
struct Args {
    /// How many agents it says are running, until a test writes another number to [`RUNNING`].
    #[arg(long, default_value_t = 0)]
    running: u32,
    /// How many clients it says are connected, until a test writes another number to [`CLIENTS`].
    #[arg(long, default_value_t = 1)]
    clients: u32,
    /// Say it was started by hand, so a client mustn't restart it.
    #[arg(long)]
    not_on_demand: bool,
    /// Be on the far side of the break from this build, rather than the near side.
    #[arg(long)]
    newer: bool,
    /// Take a hello, rather than being across a real break.
    #[arg(long)]
    speaks_this_protocol: bool,
    /// Which build it says it is.
    #[arg(long)]
    build: Option<u64>,
    /// What it does when asked to stop only if it's idle.
    #[arg(long, value_enum, default_value_t = IfIdle::Decides)]
    asked_to_stop_if_idle: IfIdle,
    /// Say it keeps its workers, so restarting it leaves its agents running, and take a restart.
    #[arg(long)]
    keeps_workers: bool,
    /// How many setup scripts it says are running, until a test writes another number to [`SETUPS`].
    #[arg(long, default_value_t = 0)]
    setups: u32,
    /// Have its setup scripts end just as it's asked to restart, after a client saw them running, so it goes at once.
    #[arg(long)]
    setups_end_as_it_restarts: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum IfIdle {
    /// Goes by what it says is running then, as a hub does.
    Decides,
    /// Doesn't know the request, as a hub from before one could decide.
    Unknown,
    /// Has an agent start first, as a prompt sent after it last said it was idle would, so it refuses, and says one
    /// is running from then on.
    Busy,
}

/// The files in its data directory that say how many agents are running and how many clients are connected, once a
/// test has written them.
const RUNNING: &str = "stand-in-running";
const CLIENTS: &str = "stand-in-clients";
const SETUPS: &str = "stand-in-setups";

/// What it leaves in its data directory as it goes for a restart.
const RESTARTED: &str = "stand-in-restarted";

/// What a test has written to this file, or else `otherwise`.
fn written(paths: &Paths, file: &str, otherwise: u32) -> u32 {
    let written = std::fs::read_to_string(paths.data_dir.join(file)).ok();
    written.and_then(|n| n.trim().parse().ok()).unwrap_or(otherwise)
}

/// One connection's way back to its client, which replies and notices share.
type Writer = Arc<Mutex<UnixStream>>;

fn send(writer: &Writer, frame: &HubFrame) -> bool {
    writer.lock().write_all(&protocol::frame::encode(frame).unwrap()).is_ok()
}

fn reply(writer: &Writer, id: u64, outcome: ReplyOutcome) {
    send(writer, &HubFrame::Reply { id, outcome });
}

/// Tells a client following its records that one has changed whenever a test changes how many agents are running, as
/// a real hub sends the session that started or finished. The record is a repository, which is simpler to make.
fn notice_agents(writer: &Writer, subscription_id: u64, args: &Args, paths: &Paths) {
    let mut said = written(paths, RUNNING, args.running);
    for rev in 1.. {
        let running = loop {
            std::thread::sleep(Duration::from_millis(10));
            let running = written(paths, RUNNING, args.running);
            if running != said {
                break running;
            }
        };
        said = running;
        let repo = Repo {
            id: RepoId::from("stand-in"),
            name: "stand-in".to_string(),
            path: "/stand-in".to_string(),
            created_at_ms: 0,
            rev: Rev::new(rev),
        };
        if !send(writer, &HubFrame::Notice { subscription_id, notice: HubNotice::Repo(repo) }) {
            return;
        }
    }
}

fn status(args: &Args, paths: &Paths) -> HubStatus {
    let running = written(paths, RUNNING, args.running);
    HubStatus {
        pid: std::process::id(),
        hub_version: "from-before-the-break".to_string(),
        protocol_version: if args.speaks_this_protocol {
            protocol::client::CLIENT_PROTOCOL_VERSION
        } else if args.newer {
            protocol::client::CLIENT_PROTOCOL_VERSION + 1
        } else {
            protocol::client::CLIENT_PROTOCOL_VERSION - 1
        },
        started_at_ms: 0,
        workers: running,
        running_sessions: running,
        clients: written(paths, CLIENTS, args.clients),
        on_demand: !args.not_on_demand,
        build_number: args.build,
        running_setups: written(paths, SETUPS, args.setups),
        requests: 1,
        keeping_awake: false,
        keeps_workers: args.keeps_workers,
        oldest_protocol_version: None,
    }
}

fn serve(stream: UnixStream, args: &Arc<Args>, paths: &Arc<Paths>) {
    let writer = Arc::new(Mutex::new(stream.try_clone().unwrap()));
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { return };
        let request: ClientFrame = protocol::frame::decode(line.as_bytes()).unwrap();
        match request.request {
            ClientRequest::Hello { .. } if args.speaks_this_protocol => {
                let hello = ClientResponse::Hello {
                    protocol_version: protocol::client::CLIENT_PROTOCOL_VERSION,
                    hub_version: "stand-in".to_string(),
                };
                reply(&writer, request.id, ReplyOutcome::Response(hello));
            }
            ClientRequest::FeedSubscribe { subscription_id } if args.speaks_this_protocol => {
                reply(&writer, request.id, ReplyOutcome::Response(ClientResponse::Ok));
                let (writer, args, paths) = (writer.clone(), args.clone(), paths.clone());
                std::thread::spawn(move || notice_agents(&writer, subscription_id, &args, &paths));
            }
            ClientRequest::RecordsList if args.speaks_this_protocol => {
                let nothing = ClientResponse::Records(Records::default());
                reply(&writer, request.id, ReplyOutcome::Response(nothing));
            }
            // As a hub from before hubs described their agents: one that speaks this protocol refuses a request it
            // doesn't know and carries on.
            ClientRequest::AgentList if args.speaks_this_protocol => {
                let unknown = ApiError::new(ErrorCode::Unsupported, "this hub doesn't know that request");
                reply(&writer, request.id, ReplyOutcome::Error(unknown));
            }
            ClientRequest::HubStatus => {
                let status = ClientResponse::HubStatus(status(args, paths));
                reply(&writer, request.id, ReplyOutcome::Response(status));
            }
            ClientRequest::HubShutdown => stop(&writer, request.id, paths),
            ClientRequest::HubRestart if args.keeps_workers => {
                reply(&writer, request.id, ReplyOutcome::Response(ClientResponse::Ok));
                let (args, paths) = (args.clone(), paths.clone());
                std::thread::spawn(move || restart(&args, &paths));
            }
            ClientRequest::HubShutdownIfIdle if args.speaks_this_protocol => {
                if args.asked_to_stop_if_idle == IfIdle::Unknown {
                    let unknown = ApiError::new(ErrorCode::Unsupported, "this hub doesn't know that request");
                    reply(&writer, request.id, ReplyOutcome::Error(unknown));
                    continue;
                }
                if args.asked_to_stop_if_idle == IfIdle::Busy {
                    std::fs::write(paths.data_dir.join(RUNNING), "1").unwrap();
                }
                if status(args, paths).is_idle() {
                    stop(&writer, request.id, paths);
                }
                let busy = ApiError::new(ErrorCode::Conflict, "the hub is busy, so it wasn't stopped");
                reply(&writer, request.id, ReplyOutcome::Error(busy));
            }
            _ => {
                let refused = ApiError::new(ErrorCode::VersionMismatch, "this hub is from before a real break");
                reply(&writer, request.id, ReplyOutcome::Error(refused));
                return;
            }
        }
    }
}

/// Goes once no setup script is running, as a hub asked to restart does, leaving [`RESTARTED`] behind.
fn restart(args: &Args, paths: &Paths) {
    while !args.setups_end_as_it_restarts && written(paths, SETUPS, args.setups) > 0 {
        std::thread::sleep(Duration::from_millis(10));
    }
    std::fs::write(paths.data_dir.join(RESTARTED), "").unwrap();
    let _ = std::fs::remove_file(&paths.socket);
    std::process::exit(0);
}

/// Says it's stopping, lets go of the socket, and exits.
fn stop(writer: &Writer, id: u64, paths: &Paths) -> ! {
    reply(writer, id, ReplyOutcome::Response(ClientResponse::Ok));
    let _ = std::fs::remove_file(&paths.socket);
    std::process::exit(0);
}

fn main() {
    let args = Arc::new(Args::parse());
    let paths = Arc::new(protocol::paths::resolve(|name| std::env::var(name).ok()).expect("find the hub's paths"));
    std::fs::create_dir_all(&paths.data_dir).unwrap();
    let lock = std::fs::File::create(&paths.lock).unwrap();
    lock.try_lock().unwrap();
    let _ = std::fs::remove_file(&paths.socket);
    let listener = UnixListener::bind(&paths.socket).unwrap();
    for stream in listener.incoming() {
        let (stream, args, paths) = (stream.unwrap(), args.clone(), paths.clone());
        std::thread::spawn(move || serve(stream, &args, &paths));
    }
}
