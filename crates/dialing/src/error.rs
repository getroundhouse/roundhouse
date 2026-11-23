use std::path::PathBuf;

/// Why an endpoint couldn't be bound, or a hub dialed.
#[derive(thiserror::Error, Debug)]
pub enum DialError {
    #[error("reading or writing {path}: {source}")]
    KeyFile { path: PathBuf, source: std::io::Error },
    #[error("{path} doesn't hold an endpoint's key")]
    NotAKey { path: PathBuf },
    #[error("binding the endpoint: {0}")]
    Bind(String),
    #[error("the ticket names no hub iroh can read: {0}")]
    Ticket(String),
    #[error("dialing the hub: {0}")]
    Dial(String),
    #[error("reading or writing what's said before the worker protocol: {0}")]
    Said(String),
    #[error("the hub refused the worker: {0}")]
    Refused(String),
}
