//! Why the guide couldn't be built.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::page::PagePath;
use crate::source::RepoPath;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{} is already there. The guide is built into a directory of its own", .0.display())]
    OutExists(PathBuf),
    #[error("{} has a name that isn't UTF-8, which a page's address has to be", .0.display())]
    NotUtf8(PathBuf),
    #[error("the guide has no README.md, which holds its contents")]
    NoContents,
    #[error("{page}: couldn't be read as Markdown: {message}")]
    Markdown { page: PagePath, message: String },
    #[error("{0}: doesn't start with its title, a `# ` heading")]
    NoTitle(PagePath),
    #[error("README.md: the contents list `{0}`, which isn't a link to another of the guide's pages")]
    NotAPage(String),
    #[error("{0} is in the guide and not in README.md's contents")]
    NotInContents(PagePath),
    #[error("{0} is in README.md's contents twice")]
    ListedTwice(PagePath),
    #[error("{page}: links to {target}, which isn't there")]
    Missing { page: PagePath, target: String },
    #[error("{page}: links to {target}, which is outside the repository")]
    Outside { page: PagePath, target: String },
    #[error("{page}: links to `#{fragment}` in {target}, which has no heading that makes it")]
    NoHeading { page: PagePath, target: PagePath, fragment: String },
    #[error("{page}: has {what}, which the guide's pages aren't drawn with")]
    Unsupported { page: PagePath, what: NotDrawn },
    #[error("{page}: shows {picture}, which isn't a PNG")]
    NotAPng { page: PagePath, picture: RepoPath },
}

/// What a page can hold that the guide's pages aren't drawn with. Each is refused rather than left out, so a page
/// reads the same on the site as it does on GitHub.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotDrawn {
    Html,
    /// A link or a picture whose address is given elsewhere on the page, as `[words][name]`.
    NamedApart,
    Footnote,
    /// A list's item with a box to tick.
    Task,
    /// A second `# ` heading, when a page has one title.
    SecondTitle,
    /// A heading that makes no slug, such as one of only punctuation, so nothing could link to it.
    HeadingWithoutWords,
    /// A link in a heading, which links to itself already.
    LinkInHeading,
    /// A picture that isn't a file in the repository, such as one on the web.
    PictureElsewhere,
    /// An address that's neither a path nor on the web, such as `javascript:`.
    AddressKind,
    /// What Markdown, as the guide reads it, doesn't make: math, front matter, MDX, or part of a list or a table
    /// outside one.
    NotMade,
}

impl fmt::Display for NotDrawn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Html => "HTML",
            Self::NamedApart => "a link or a picture named apart from where it's used",
            Self::Footnote => "a footnote",
            Self::Task => "a list item with a box to tick",
            Self::SecondTitle => "a second `# ` heading",
            Self::HeadingWithoutWords => "a heading with no letters or digits",
            Self::LinkInHeading => "a link in a heading",
            Self::PictureElsewhere => "a picture that isn't a file in the repository",
            Self::AddressKind => "an address that's neither a path nor on the web",
            Self::NotMade => "something Markdown, as the guide reads it, doesn't make",
        })
    }
}

/// What turns an I/O error into one that says which file it was about.
pub(crate) fn io_error(path: &Path) -> impl FnOnce(io::Error) -> Error {
    let path = path.to_path_buf();
    move |source| Error::Io { path, source }
}
