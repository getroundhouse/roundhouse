//! Builds the guide, `docs/guide`, as the pages the site serves under `/docs/`: a page for each Markdown file, in the
//! order `README.md`'s contents list them. A link to a page, a heading or a file that isn't there stops the build, and
//! so does a page the contents leave out, so what's published can't point nowhere. `tools/guide/README.md` says how
//! the guide is written, and how it's published.

#![warn(clippy::wildcard_enum_match_arm)]

mod build;
mod contents;
mod draw;
mod error;
mod links;
mod page;
mod picture;
mod shell;
mod slug;
mod source;

pub use build::{Built, build};
pub use error::{Error, NotDrawn};
pub use page::PagePath;
pub use source::{RepoPath, Source};
