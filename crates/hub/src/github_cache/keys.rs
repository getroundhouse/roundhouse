use protocol::model::{BranchName, GithubRepositoryName, IssueNumber};

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
}

/// A branch of a repository on GitHub, by its repository, in lower case, and its name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct BranchOf {
    repository: String,
    branch: BranchName,
}

impl BranchOf {
    pub(crate) fn new(repository: &GithubRepositoryName, branch: BranchName) -> Self {
        Self { repository: repository.as_str().to_ascii_lowercase(), branch }
    }

    pub(super) fn repository(&self) -> &str {
        &self.repository
    }

    pub(super) fn branch(&self) -> &BranchName {
        &self.branch
    }
}

/// What a row of the cache's file is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Subject {
    Item(Item),
    Branch(BranchOf),
}

/// What the file writes before a branch's name, which no number starts with.
const BRANCH: &str = "refs/heads/";

impl Subject {
    /// As the file keeps it, which only [`Item::new`] and [`BranchOf::new`] made, or `None` for what this build
    /// didn't write.
    pub(super) fn from_kept(repository: String, key: &str) -> Option<Self> {
        if let Some(branch) = key.strip_prefix(BRANCH) {
            return Some(Self::Branch(BranchOf { repository, branch: BranchName::from(branch) }));
        }
        let number = IssueNumber::try_from(key.parse::<u64>().ok()?).ok()?;
        Some(Self::Item(Item { repository, number }))
    }

    pub(super) fn repository(&self) -> &str {
        match self {
            Self::Item(item) => &item.repository,
            Self::Branch(branch) => &branch.repository,
        }
    }

    /// What tells it apart in its repository, as the file keeps it.
    pub(super) fn key(&self) -> String {
        match self {
            Self::Item(item) => item.number.to_string(),
            Self::Branch(branch) => format!("{BRANCH}{}", branch.branch.as_str()),
        }
    }
}

/// A part of what's kept of an issue or a pull request, read only when what asks needs it fresher than it is, with the
/// time it was read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Facet {
    /// What it is and where it stands, with who opened it, its labels and the start of what it says, as a chip and its
    /// card show it, and for a pull request, the commits it's at and whether it can merge.
    Summary,
    /// A branch's rules for the checks a pull request into it needs.
    Rules,
    /// When a review last asked for changes, and a pull request last left the merge queue.
    History,
    /// Where a pull request is in its stack.
    Stack,
    /// The checks reported for the commit a pull request's checks run on, with that commit.
    Checks,
}

impl Facet {
    /// Its name in the cache's file.
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Summary => "summary",
            Self::Rules => "rules",
            Self::History => "history",
            Self::Stack => "stack",
            Self::Checks => "checks",
        }
    }

    /// The facet [`Facet::name`] names `name`.
    pub(super) fn named(name: &str) -> Option<Self> {
        [Self::Summary, Self::Rules, Self::History, Self::Stack, Self::Checks]
            .into_iter()
            .find(|facet| facet.name() == name)
    }
}
