use std::io;
use std::path::PathBuf;

use crate::files::CanvasPath;

/// Why the design couldn't be read, or its index written.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("couldn't read {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{0} has no status badge at its top: a copy of one of `Board/Status`'s masters, named \"Status\"")]
    NoBadge(CanvasPath),
    #[error("{0} has more than one copy of `Board/Status`'s masters named \"Status\", and only its own badge may be")]
    Badges(CanvasPath),
    #[error("{0} isn't in one of the design's levels: Foundations, Components, Patterns, Screens or Board")]
    NoLevel(CanvasPath),
    #[error("{0} has a quote or a backslash in its name, which the index can't write")]
    Unwritable(CanvasPath),
}
