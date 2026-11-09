//! Stands in for a hub of another build, which a test can't otherwise get. As a client sees it across a real
//! protocol break, it refuses every hello, and all that still works is seeing it and stopping it. With
//! `--speaks-this-protocol` it takes a hello, as an older build would, and serves a hub with nothing in it, so the
//! app can use it, and refuses anything else. It holds the same lock as a real hub. A test can change how many agents
//! it says are running, as if some had started or finished, by writing the number to [`RUNNING`] in its data directory.

#![expect(clippy::unwrap_used, clippy::expect_used, reason = "a test program, so failing loudly is what it should do")]

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::Arc;

use clap::Parser;
use protocol::client::{
    ApiError, ClientFrame, ClientRequest, ClientResponse, ErrorCode, HubFrame, HubStatus, ReplyOutcome,
};
use protocol::model::Records;
use protocol::paths::Paths;

#[derive(Parser)]
struct Args {
    /// How many agents it says are running, until a test writes another number to [`RUNNING`].
    #[arg(long, default_value_t = 0)]
    running: u32,
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
}

fn reply(stream: &mut UnixStream, id: u64, outcome: ReplyOutcome) {
    let _ = stream.write_all(&protocol::frame::encode(&HubFrame::Reply { id, outcome }).unwrap());
}

/// The file in its data directory that says how many agents are running, once a test has written it.
const RUNNING: &str = "stand-in-running";

fn running(args: &Args, paths: &Paths) -> u32 {
    let written = std::fs::read_to_string(paths.data_dir.join(RUNNING)).ok();
    written.and_then(|n| n.trim().parse().ok()).unwrap_or(args.running)
}

fn serve(mut stream: UnixStream, args: &Args, paths: &Paths) {
    for line in BufReader::new(stream.try_clone().unwrap()).lines() {
        let Ok(line) = line else { return };
        let request: ClientFrame = protocol::frame::decode(line.as_bytes()).unwrap();
        match request.request {
            ClientRequest::Hello { .. } if args.speaks_this_protocol => {
                let hello = ClientResponse::Hello {
                    protocol_version: protocol::PROTOCOL_VERSION,
                    hub_version: "stand-in".to_string(),
                };
                reply(&mut stream, request.id, ReplyOutcome::Response(hello));
            }
            ClientRequest::FeedSubscribe { .. } if args.speaks_this_protocol => {
                reply(&mut stream, request.id, ReplyOutcome::Response(ClientResponse::Ok));
            }
            ClientRequest::RecordsList if args.speaks_this_protocol => {
                let nothing = ClientResponse::Records(Records::default());
                reply(&mut stream, request.id, ReplyOutcome::Response(nothing));
            }
            ClientRequest::HubStatus => {
                let running = running(args, paths);
                let status = HubStatus {
                    pid: std::process::id(),
                    hub_version: "from-before-the-break".to_string(),
                    protocol_version: if args.speaks_this_protocol {
                        protocol::PROTOCOL_VERSION
                    } else if args.newer {
                        protocol::PROTOCOL_VERSION + 1
                    } else {
                        protocol::PROTOCOL_VERSION - 1
                    },
                    started_at_ms: 0,
                    workers: running,
                    running_sessions: running,
                    clients: 1,
                    on_demand: !args.not_on_demand,
                    build_number: args.build,
                    running_setups: 0,
                    requests: 1,
                };
                reply(&mut stream, request.id, ReplyOutcome::Response(ClientResponse::HubStatus(status)));
            }
            ClientRequest::HubShutdown => {
                reply(&mut stream, request.id, ReplyOutcome::Response(ClientResponse::Ok));
                let _ = std::fs::remove_file(&paths.socket);
                std::process::exit(0);
            }
            _ => {
                let refused = ApiError::new(ErrorCode::VersionMismatch, "this hub is from before a real break");
                reply(&mut stream, request.id, ReplyOutcome::Error(refused));
                return;
            }
        }
    }
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
