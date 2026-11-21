//! One ask's read, as it goes from round to round: which parts it asks GitHub for next, what it makes of each answer,
//! and what it found once nothing's left to ask.

use std::collections::{BTreeMap, BTreeSet};

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{
    GitHubRepository, GithubRepositoryName, ObservedPullRequest, PrState, PullRequestNumber, PullRequestsSnapshot,
    Reference,
};
use serde_json::Value;

use super::asked::{PullRequestsAsk, Unread};
use super::facets::{self, Checks, Reviews, Stack, Status};
use super::parse::{self, Metadata, Policy, ReportedCheck};
use super::query::Part;
use super::said::{array, changed, incomplete, next, string};
use crate::references::pull_request_reference;

pub(super) struct Reading {
    pub(super) ask: PullRequestsAsk,
    /// When it began, by the wall clock, which is when what it finds was read, since none of it is older.
    began_ms: i64,
    repository: Option<GitHubRepository>,
    /// Those to look at next round: the ones asked for, then those found under them.
    to_look: BTreeSet<PullRequestNumber>,
    looked: BTreeMap<PullRequestNumber, Looked>,
    /// Each branch the open pull requests go into, with its rules for checks as far as they're read.
    branches: Vec<Branch>,
    /// Those above each, which a failure to read it fails too.
    dependents: BTreeMap<PullRequestNumber, BTreeSet<PullRequestNumber>>,
    /// The summary of each looked at, or why it couldn't be read, which fails nothing else.
    summaries: Vec<(PullRequestNumber, Result<Reference, ApiError>)>,
    failed: Option<Unread>,
}

/// What's been read of one pull request.
struct Looked {
    pr: Value,
    entries: Vec<Value>,
    stack_after: Option<String>,
    metadata: Option<Metadata>,
    checks: Vec<ReportedCheck>,
    checks_after: Option<String>,
    /// What it was when looked at again, once every other part of the read was in.
    confirmed: Option<Value>,
}

struct Branch {
    name: String,
    policy: Policy,
    after: Option<String>,
    read: bool,
}

impl Reading {
    pub(super) fn new(ask: PullRequestsAsk) -> Self {
        let to_look = ask.selection.numbers().iter().copied().collect();
        Self {
            ask,
            began_ms: jiff::Timestamp::now().as_millisecond(),
            repository: None,
            to_look,
            looked: BTreeMap::new(),
            branches: Vec::new(),
            dependents: BTreeMap::new(),
            summaries: Vec::new(),
            failed: None,
        }
    }

    /// What to ask GitHub for next: each pull request not looked at yet and each page past the first of what has been,
    /// then once those are in, the rules of each branch they go into, with each pull request looked at again to
    /// confirm it hasn't moved, then the rest of those rules. None once it's done, or failed.
    pub(super) fn parts(&self) -> Vec<Part> {
        if self.failed.is_some() {
            return Vec::new();
        }
        let mut parts: Vec<Part> = self.to_look.iter().copied().map(Part::Look).collect();
        for (number, looked) in &self.looked {
            if let Some(after) = &looked.stack_after {
                parts.push(Part::Stack(*number, after.clone()));
            }
            if let Some(after) = &looked.checks_after {
                parts.push(Part::Checks { number: *number, after: after.clone() });
            }
        }
        if !parts.is_empty() {
            return parts;
        }
        for (index, branch) in self.branches.iter().enumerate().filter(|(_, branch)| !branch.read) {
            parts.push(Part::Branch { index, branch: branch.name.clone(), after: branch.after.clone() });
        }
        for (number, looked) in self.looked.iter().filter(|(_, looked)| looked.confirmed.is_none()) {
            let metadata = looked.metadata.as_ref();
            let lower = metadata.is_some_and(|metadata| !metadata.dependencies.is_empty());
            let head = metadata.filter(|_| lower).map(|metadata| metadata.head.as_str().to_owned());
            parts.push(Part::Confirm { number: *number, head });
        }
        parts
    }

    /// Takes the repository's own part of an answer: which repository on GitHub it is, as GitHub writes it.
    pub(super) fn take_repository(&mut self, repository: &Value) {
        if repository.is_null() {
            let message = "the repository isn't there, or the token the pull requests are read with can't see it";
            return self.fail(ApiError::new(ErrorCode::Unavailable, message), None);
        }
        match (string(repository, "url"), string(repository, "nameWithOwner")) {
            (Ok(url), Ok(name_with_owner)) => self.repository = Some(GitHubRepository { url, name_with_owner }),
            _ => self.fail(incomplete(), None),
        }
    }

    /// Takes what GitHub answered for `part`.
    pub(super) fn take(&mut self, part: &Part, answered: &Value) {
        if self.failed.is_some() {
            return;
        }
        let taken = match part {
            Part::Look(number) => self.take_look(*number, answered),
            Part::Stack(number, _) => self.take_stack_page(*number, answered),
            Part::Checks { number, .. } => self.take_checks_page(*number, answered),
            Part::Branch { index, .. } => self.take_branch(*index, answered),
            Part::Confirm { number, .. } => self.take_confirmed(*number, answered),
        };
        if let Err(error) = taken {
            self.fail(error, part.about());
        }
    }

    /// Fails the read, about the pull request `about`, and those above it, if it's about one.
    pub(super) fn fail(&mut self, error: ApiError, about: Option<PullRequestNumber>) {
        if self.failed.is_some() {
            return;
        }
        let mut affected = BTreeSet::new();
        if let Some(number) = about {
            affected.insert(number);
            affected.extend(self.dependents.get(&number).into_iter().flatten().copied());
        }
        self.failed = Some(Unread { error, affected: affected.into_iter().collect() });
    }

    fn take_look(&mut self, number: PullRequestNumber, pr: &Value) -> Result<(), ApiError> {
        self.to_look.remove(&number);
        if pr.is_null() {
            let message = format!("pull request #{} isn't there, or can't be seen", number.get());
            return Err(ApiError::new(ErrorCode::Unavailable, message));
        }
        let mut looked = Looked {
            pr: pr.clone(),
            entries: Vec::new(),
            stack_after: None,
            metadata: None,
            checks: Vec::new(),
            checks_after: None,
            confirmed: None,
        };
        if !pr.get("stack").ok_or_else(incomplete)?.is_null() {
            let connection = &pr["stack"]["entries"];
            looked.entries.extend(array(connection, "nodes")?.iter().cloned());
            looked.stack_after = next(connection)?;
        }
        let summary = self.repository.as_ref().ok_or_else(incomplete).and_then(|repository| {
            let name = GithubRepositoryName::try_from(repository.name_with_owner.clone()).map_err(|_| incomplete())?;
            pull_request_reference(name, pr)
        });
        self.summaries.push((number, summary));
        // Only an open one's checks can still matter.
        let rollup = &pr["statusCheckRollup"];
        if !rollup.is_null() && pr["state"] == "OPEN" {
            parse::checks(rollup, pr, &mut looked.checks)?;
            looked.checks_after = next(&rollup["contexts"])?;
        }
        self.looked.insert(number, looked);
        self.stack_read(number)
    }

    fn take_stack_page(&mut self, number: PullRequestNumber, pr: &Value) -> Result<(), ApiError> {
        let looked = self.looked.get_mut(&number).ok_or_else(incomplete)?;
        if string(pr, "headRefOid")? != string(&looked.pr, "headRefOid")? {
            return Err(changed());
        }
        let connection = &pr["stack"]["entries"];
        looked.entries.extend(array(connection, "nodes")?.iter().cloned());
        looked.stack_after = next(connection)?;
        self.stack_read(number)
    }

    /// Once its stack is read whole, what it is and which open pull requests are under it, each to be looked at too.
    fn stack_read(&mut self, number: PullRequestNumber) -> Result<(), ApiError> {
        let looked = self.looked.get_mut(&number).ok_or_else(incomplete)?;
        if looked.stack_after.is_some() {
            return Ok(());
        }
        let total = looked.pr["stack"]["entries"]["totalCount"].as_u64();
        if !looked.pr["stack"].is_null() && total != u64::try_from(looked.entries.len()).ok() {
            return Err(changed());
        }
        let mut metadata = Metadata::read(&looked.pr)?;
        metadata.include(std::mem::take(&mut looked.entries))?;
        for lower in &metadata.dependencies {
            self.dependents.entry(*lower).or_default().insert(number);
            if !self.looked.contains_key(lower) {
                self.to_look.insert(*lower);
            }
        }
        let open = metadata.state == PrState::Open;
        if open && !self.branches.iter().any(|branch| branch.name == metadata.destination) {
            let name = metadata.destination.clone();
            self.branches.push(Branch { name, policy: Policy::default(), after: None, read: false });
        }
        if let Some(looked) = self.looked.get_mut(&number) {
            looked.metadata = Some(metadata);
        }
        Ok(())
    }

    fn take_checks_page(&mut self, number: PullRequestNumber, pr: &Value) -> Result<(), ApiError> {
        let looked = self.looked.get_mut(&number).ok_or_else(incomplete)?;
        if string(pr, "headRefOid")? != string(&looked.pr, "headRefOid")? {
            return Err(changed());
        }
        let rollup = &pr["statusCheckRollup"];
        if rollup.is_null() {
            return Err(changed());
        }
        parse::checks(rollup, pr, &mut looked.checks)?;
        looked.checks_after = next(&rollup["contexts"])?;
        Ok(())
    }

    fn take_branch(&mut self, index: usize, reference: &Value) -> Result<(), ApiError> {
        let branch = self.branches.get_mut(index).ok_or_else(incomplete)?;
        let reference = Some(reference).filter(|reference| reference.is_object()).ok_or_else(incomplete)?;
        branch.policy.include(reference)?;
        branch.after = next(&reference["rules"])?;
        branch.read = branch.after.is_none();
        Ok(())
    }

    /// What it is now, which has to be what it was when looked at first, in its place in the same stack.
    fn take_confirmed(&mut self, number: PullRequestNumber, pr: &Value) -> Result<(), ApiError> {
        let looked = self.looked.get_mut(&number).ok_or_else(incomplete)?;
        let earlier = looked.metadata.as_ref().ok_or_else(incomplete)?;
        if pr.is_null() {
            return Err(changed());
        }
        let mut now = Metadata::read(pr)?;
        now.dependencies.clone_from(&earlier.dependencies);
        let entries =
            |pr: &Value| (pr["stack"]["entries"]["totalCount"].clone(), pr["stack"]["entries"]["nodes"].clone());
        if now != *earlier || entries(pr) != entries(&looked.pr) {
            return Err(changed());
        }
        looked.confirmed = Some(pr.clone());
        Ok(())
    }

    /// The summary of each pull request it looked at, or why it couldn't be read, apart from whether the rest could.
    pub(super) fn take_summaries(&mut self) -> Vec<(PullRequestNumber, Result<Reference, ApiError>)> {
        std::mem::take(&mut self.summaries)
    }

    /// What it found, once [`Reading::parts`] has nothing left to ask, or why it couldn't be read.
    pub(super) fn finish(mut self) -> Result<PullRequestsSnapshot, Unread> {
        if let Some(failed) = self.failed.take() {
            return Err(failed);
        }
        let observed = std::mem::take(&mut self.looked)
            .into_iter()
            .map(|(number, looked)| self.observed(looked).map_err(|e| (e, number)));
        let observed: Result<Vec<_>, _> = observed.collect();
        let pull_requests = match observed {
            Ok(observed) => observed,
            Err((error, number)) => {
                self.fail(error, Some(number));
                return Err(self.failed.take().unwrap_or_else(|| Unread { error: incomplete(), affected: Vec::new() }));
            }
        };
        let repository = self.repository.take().ok_or_else(|| Unread { error: incomplete(), affected: Vec::new() })?;
        Ok(PullRequestsSnapshot { repository, observed_at_ms: self.began_ms, pull_requests })
    }

    /// What one pull request is: what it was confirmed to be, with the checks read of it and which of them its
    /// branch's rules require.
    fn observed(&self, looked: Looked) -> Result<ObservedPullRequest, ApiError> {
        let Looked { pr, metadata, checks, confirmed, .. } = looked;
        let (metadata, confirmed) = (metadata.ok_or_else(incomplete)?, confirmed.ok_or_else(incomplete)?);
        let revision = pr["statusCheckRollup"]["commit"]["oid"].as_str().map(Into::into);
        // A merged or closed one may go into a branch since deleted, whose rules no longer matter.
        let rules = self
            .branches
            .iter()
            .find(|branch| metadata.state == PrState::Open && branch.name == metadata.destination)
            .map(|branch| &branch.policy);
        Ok(facets::observe(
            Status::read(&confirmed)?,
            Reviews::read(&confirmed)?,
            Stack::read(&confirmed, &metadata)?,
            Checks { revision, reported: checks },
            rules,
        ))
    }
}
