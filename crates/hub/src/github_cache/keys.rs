use protocol::model::{GithubRepositoryName, IssueNumber};

/// An issue or a pull request on GitHub, which GitHub numbers together in each repository, by its repository, as
/// GitHub writes it in lower case, since GitHub's names ignore case, and its number.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Item {
    repository: String,
    number: IssueNumber,
}

impl Item {
    pub(crate) fn new(repository: &GithubRepositoryName, number: IssueNumber) -> Self {
        Self { repository: repository.as_str().to_ascii_lowercase(), number }
    }

    /// As the cache's file keeps it, which only [`Item::new`] made.
    pub(super) fn from_kept(repository: String, number: IssueNumber) -> Self {
        Self { repository, number }
    }

    pub(super) fn repository(&self) -> &str {
        &self.repository
    }

    pub(super) fn number(&self) -> IssueNumber {
        self.number
    }
}

/// A part of what's kept of an issue or a pull request, read only when what asks needs it fresher than it is, with the
/// time it was read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Facet {
    /// What it is and where it stands, with who opened it, its labels and the start of what it says, as a chip and its
    /// card show it.
    Summary,
}

impl Facet {
    /// Its name in the cache's file.
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Summary => "summary",
        }
    }

    /// The facet [`Facet::name`] names `name`.
    pub(super) fn named(name: &str) -> Option<Self> {
        [Self::Summary].into_iter().find(|facet| facet.name() == name)
    }
}
