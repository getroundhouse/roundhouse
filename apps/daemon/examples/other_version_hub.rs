//! A hub of another version, as a client sees one: it refuses every hello, so all a client can do is
//! stop it, by signal. It keeps the same lock and record as a real hub, and dies on `SIGTERM`.

#![expect(clippy::unwrap_used, clippy::expect_used, reason = "a test program, so failing loudly is what it should do")]

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::UnixListener;

use protocol::client::{ApiError, ClientFrame, ErrorCode, HubFrame, ReplyOutcome};

fn main() {
    let paths = protocol::paths::resolve(|name| std::env::var(name).ok()).expect("find the hub's paths");
    std::fs::create_dir_all(&paths.data_dir).unwrap();
    let lock = std::fs::File::create(&paths.lock).unwrap();
    lock.try_lock().unwrap();
    process::ProcessRecord::write(&paths.pid, std::process::id()).unwrap();
    let _ = std::fs::remove_file(&paths.socket);
    let listener = UnixListener::bind(&paths.socket).unwrap();
    for stream in listener.incoming() {
        let mut stream = stream.unwrap();
        let mut line = String::new();
        if BufReader::new(&stream).read_line(&mut line).is_err() || line.is_empty() {
            continue;
        }
        let request: ClientFrame = protocol::frame::decode(line.as_bytes()).unwrap();
        let refused = ApiError::new(ErrorCode::VersionMismatch, "this hub is another version");
        let reply = HubFrame::Reply { id: request.id, outcome: ReplyOutcome::Error(refused) };
        let _ = stream.write_all(&protocol::frame::encode(&reply).unwrap());
    }
}
