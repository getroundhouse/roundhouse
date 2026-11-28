//! Stands in for a worker that misbehaves once its hub has admitted it, which a test can't otherwise get. A test puts
//! it where the hub starts its workers from, and the local provider runs it as it runs a worker, as
//! `worker --ticket - --detach …`. Until it's admitted it does what a worker does: it starts itself again apart from
//! what ran it and returns, reads its ticket from its stdin, records itself in its state directory as the worker serving
//! the instance, so its provider ends it as it ends a worker, and dials the hub. Then it does what `MISBEHAVING_WORKER`
//! says, which it has from its hub's environment:
//!
//! - `another-version`: answers the hub's hello as a worker that speaks another version of the worker protocol.
//! - `silent`: answers nothing.
//!
//! It holds its connection until it's ended, or its test has gone.

#![expect(clippy::unwrap_used, reason = "a test program, so failing loudly is what it should do")]

use std::ffi::OsString;
use std::io::{Read as _, Write as _};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use dialing::{WorkerEndpoint, read_frame, write_frame};
use process::ProcessRecord;
use process::testing::TestProcess;
use protocol::client::{ApiError, ErrorCode};
use protocol::dialing::WorkerTicket;
use protocol::worker::{WorkerCall, WorkerFrame, WorkerOutcome};

/// What says how it misbehaves.
const MISBEHAVES: &str = "MISBEHAVING_WORKER";

fn main() {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let mut ticket = String::new();
    std::io::stdin().read_to_string(&mut ticket).unwrap();
    if args.iter().any(|arg| arg == "--detach") {
        start_apart(&args, &ticket);
        return;
    }
    let state_dir = PathBuf::from(after(&args, "--state-dir"));
    std::fs::create_dir_all(&state_dir).unwrap();
    ProcessRecord::write(&state_dir.join(protocol::paths::WORKER_RECORD_FILE), std::process::id()).unwrap();
    let misbehaves = std::env::var(MISBEHAVES).unwrap();
    let test = TestProcess::from_env(|name| std::env::var(name).ok());
    tokio::runtime::Runtime::new().unwrap().block_on(async move {
        tokio::select! {
            () = misbehave(&ticket, &misbehaves) => {}
            () = TestProcess::until_gone(test) => {}
        }
    });
}

/// Itself again, without `--detach`, apart from what ran it, as a worker starts itself, given `ticket` on its stdin.
fn start_apart(args: &[OsString], ticket: &str) {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(args.iter().filter(|arg| *arg != "--detach"))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    process::detach(&mut command);
    #[expect(clippy::zombie_processes, reason = "never waited on, since the point is that it outlives this process")]
    let mut started = command.spawn().unwrap();
    started.stdin.take().unwrap().write_all(ticket.as_bytes()).unwrap();
}

/// The argument after `flag`.
fn after<'a>(args: &'a [OsString], flag: &str) -> &'a OsString {
    let at = args.iter().position(|arg| arg == flag).unwrap();
    &args[at + 1]
}

/// Dials the hub with `ticket`, and once it's admitted, misbehaves as `how` says, holding its connection after.
async fn misbehave(ticket: &str, how: &str) {
    let ticket = WorkerTicket::from(ticket.to_string()).read().unwrap();
    let endpoint = WorkerEndpoint::bind(&ticket.hub, None, None).await.unwrap();
    let mut admitted = endpoint.dial_admitted(&ticket.secret).await.unwrap();
    match how {
        "another-version" => {
            let hello: WorkerCall = read_frame(&mut admitted.recv).await.unwrap();
            let refused = ApiError::new(ErrorCode::VersionMismatch, "stub speaks another version");
            let reply = WorkerFrame::Reply { id: hello.id, outcome: WorkerOutcome::Error(refused) };
            write_frame(&mut admitted.send, &reply).await.unwrap();
        }
        "silent" => {}
        other => panic!("{MISBEHAVES} is {other:?}, which isn't a way it misbehaves"),
    }
    std::future::pending::<()>().await;
}
