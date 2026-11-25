//! The app's log, which is a file beside the hub's, and stderr, for whoever opened the app from a checkout.

use std::fs::File;
use std::io;
use std::sync::Arc;

use logging::{Levels, Log};
use protocol::paths::{ENV_LOG, Paths};

/// Starts the app's log. With no file to write, it's on stderr alone, which it says.
pub(crate) fn start(paths: Option<&Paths>) {
    let directives = std::env::var(ENV_LOG).ok();
    let log = Log::new(Levels::new(directives.as_deref())).text_to(io::stderr);
    let (log, unopened) = match paths.map(open) {
        Some(Ok(file)) => (log.json_to(Arc::new(file)), None),
        Some(Err(e)) => (log, Some(e)),
        None => (log, None),
    };
    if let Err(e) = log.start() {
        eprintln!("couldn't start the log: {e}");
    }
    if let Some(error) = unopened {
        tracing::warn!(name: "log.unopened", %error, "Couldn't open the log's file, so it's on stderr alone");
    }
}

fn open(paths: &Paths) -> io::Result<File> {
    private_files::create_dir_all(&paths.data_dir)?;
    logging::move_aside_if_long(&paths.app_log);
    private_files::file_options().create(true).append(true).open(&paths.app_log)
}
