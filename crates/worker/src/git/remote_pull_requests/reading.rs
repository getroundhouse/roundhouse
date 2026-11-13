use std::collections::{BTreeMap, BTreeSet};

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{GitHubRepository, PrState, PullRequestNumber, PullRequestSelection, PullRequestsSnapshot};
use serde_json::Value;

use super::parse::{Metadata, Policy};
use super::{parse, query};
use crate::git::pull_request::{Unran, run_gh};

pub(crate) async fn read(
    path: &str,
    repository: Option<GitHubRepository>,
    requested: &PullRequestSelection,
) -> protocol::worker::RemotePullRequestsReply {
    let mut reader = Reader {
        path,
        source: Gh,
        repository,
        rate_limit: None,
        cost: 0,
        failed_pr: None,
        dependents: BTreeMap::new(),
    };
    let result = reader.read(requested).await;
    reader.finish(result)
}

pub(super) trait Query {
    async fn run(&mut self, path: &str, args: &[&str]) -> Result<Value, ApiError>;
}
pub(super) struct Gh;
impl Query for Gh {
    async fn run(&mut self, path: &str, args: &[&str]) -> Result<Value, ApiError> {
        gh(path, args).await
    }
}

pub(super) struct Reader<'a, Q> {
    pub(super) source: Q,
    pub(super) path: &'a str,
    pub(super) repository: Option<GitHubRepository>,
    pub(super) rate_limit: Option<protocol::worker::RateLimit>,
    pub(super) cost: u32,
    pub(super) failed_pr: Option<PullRequestNumber>,
    pub(super) dependents: BTreeMap<PullRequestNumber, BTreeSet<PullRequestNumber>>,
}

impl<Q: Query> Reader<'_, Q> {
    pub(super) fn finish(
        self,
        result: Result<PullRequestsSnapshot, ApiError>,
    ) -> protocol::worker::RemotePullRequestsReply {
        let mut affected = BTreeSet::new();
        if result.is_err()
            && let Some(number) = self.failed_pr
        {
            affected.insert(number);
            affected.extend(self.dependents.get(&number).into_iter().flatten().copied());
        }
        protocol::worker::RemotePullRequestsReply {
            observation: result,
            rate_limit: self.rate_limit,
            cost: self.cost,
            affected: affected.into_iter().collect(),
        }
    }

    pub(super) async fn read(&mut self, requested: &PullRequestSelection) -> Result<PullRequestsSnapshot, ApiError> {
        if self.repository.is_none() {
            self.cost += 1;
            let value = self.source.run(self.path, &["repo", "view", "--json", "nameWithOwner,url"]).await?;
            self.repository = Some(GitHubRepository {
                url: string(&value, "url")?,
                name_with_owner: string(&value, "nameWithOwner")?,
            });
        }
        let mut remaining: BTreeSet<_> = requested.numbers().iter().copied().collect();
        let mut metadata = BTreeMap::new();
        while let Some(number) = remaining.pop_first() {
            if metadata.contains_key(&number) {
                continue;
            }
            let found = self.metadata(number).await?;
            for dependency in &found.dependencies {
                self.dependents.entry(*dependency).or_default().insert(number);
            }
            remaining.extend(found.dependencies.iter().filter(|n| !metadata.contains_key(*n)).copied());
            metadata.insert(number, found);
        }
        let mut policies = BTreeMap::new();
        let mut observations = Vec::new();
        for (number, found) in &metadata {
            let destination = &found.destination;
            if !policies.contains_key(destination) && found.state == PrState::Open {
                policies.insert(destination.clone(), self.policy(destination).await?);
            }
            // Merged or closed PRs may name a deleted base, whose protection and checks no longer matter.
            let empty = Policy::default();
            let policy = policies.get(destination).unwrap_or(&empty);
            observations.push(self.observation(*number, found, policy).await?);
        }
        // A lower branch can move while an upper branch's checks are read. Never judge that mixed stack.
        for (number, found) in &metadata {
            if self.metadata(*number).await? != *found {
                return Err(changed());
            }
        }
        Ok(PullRequestsSnapshot {
            repository: self.repository.clone().ok_or_else(incomplete)?,
            observed_at_ms: chrono::Utc::now().timestamp_millis(),
            pull_requests: observations,
        })
    }

    async fn query(&mut self, fields: &str) -> Result<Value, ApiError> {
        if self.rate_limit.is_some_and(|limit| limit.remaining <= limit.limit / 5) {
            return Err(ApiError::new(ErrorCode::Worker, "GitHub's budget is temporarily exhausted"));
        }
        let repository = self.repository.as_ref().ok_or_else(incomplete)?;
        let (owner, name) = repository.name_with_owner.split_once('/').ok_or_else(incomplete)?;
        let host = repository.url.split('/').nth(2).filter(|host| !host.is_empty()).ok_or_else(incomplete)?;
        let query = format!(
            "query {{ repository(owner:{},name:{}) {{ {fields} }} rateLimit {{ cost limit remaining resetAt }} }}",
            query::quote(owner),
            query::quote(name)
        );
        // A request with no readable response still costs at least one point. A response replaces that estimate
        // with GitHub's cost, even when GraphQL returned errors and gh exited unsuccessfully.
        self.cost = self.cost.saturating_add(1);
        if let Some(limit) = &mut self.rate_limit {
            limit.remaining = limit.remaining.saturating_sub(1);
        }
        let value = self
            .source
            .run(self.path, &["api", "--hostname", host, "graphql", "-f", &format!("query={query}")])
            .await?;
        self.received(&value)
    }

    pub(super) fn received(&mut self, value: &Value) -> Result<Value, ApiError> {
        let usage = value.pointer("/data/rateLimit").filter(|value| value.is_object());
        if let Some(usage) = usage {
            let cost = count(usage, "cost")?;
            self.cost = self.cost.saturating_sub(1).saturating_add(cost);
            self.rate_limit = Some(protocol::worker::RateLimit {
                limit: count(usage, "limit")?,
                remaining: count(usage, "remaining")?,
                resets_at_ms: chrono::DateTime::parse_from_rfc3339(&string(usage, "resetAt")?)
                    .map_err(|_| incomplete())?
                    .timestamp_millis(),
            });
        }
        if let Some(errors) = value.get("errors") {
            return Err(failed(&errors.to_string(), None));
        }
        if usage.is_none() {
            return Err(incomplete());
        }
        value
            .pointer("/data/repository")
            .filter(|v| v.is_object())
            .cloned()
            .ok_or_else(|| ApiError::new(ErrorCode::Unavailable, "the watched repository is no longer accessible"))
    }

    async fn metadata(&mut self, number: PullRequestNumber) -> Result<Metadata, ApiError> {
        self.failed_pr = Some(number);
        let mut after = None;
        let mut found = None;
        let mut entries = Vec::new();
        let mut pages = BTreeSet::new();
        loop {
            let value = self.query(&query::metadata(number, after.as_deref())).await?;
            let pr = value.get("pullRequest").filter(|v| v.is_object()).ok_or_else(|| {
                ApiError::new(ErrorCode::Unavailable, format!("pull request #{} is not accessible", number.get()))
            })?;
            let now = Metadata::read(pr)?;
            if found.as_ref().is_some_and(|before| before != &now) {
                return Err(changed());
            }
            found = Some(now);
            let connection = &pr["stack"]["entries"];
            if pr["stack"].is_null() {
                break;
            }
            entries.extend(array(connection, "nodes")?.iter().cloned());
            after = next(connection)?;
            if let Some(cursor) = &after {
                if !pages.insert(cursor.clone()) {
                    return Err(incomplete());
                }
            } else if connection["totalCount"].as_u64() != u64::try_from(entries.len()).ok() {
                return Err(changed());
            }
            if after.is_none() {
                break;
            }
        }
        let mut found = found.ok_or_else(incomplete)?;
        found.include(entries)?;
        Ok(found)
    }

    async fn policy(&mut self, branch: &str) -> Result<Policy, ApiError> {
        self.failed_pr = None;
        let mut after = None;
        let mut policy = Policy::default();
        loop {
            let value = self.query(&query::requirements(branch, after.as_deref())).await?;
            let reference = value.get("ref").filter(|v| v.is_object()).ok_or_else(incomplete)?;
            policy.include(reference)?;
            after = next(&reference["rules"])?;
            if after.is_none() {
                return Ok(policy);
            }
        }
    }

    async fn observation(
        &mut self,
        number: PullRequestNumber,
        metadata: &Metadata,
        policy: &Policy,
    ) -> Result<protocol::model::ObservedPullRequest, ApiError> {
        self.failed_pr = Some(number);
        let mut after = None;
        let mut checks = Vec::new();
        let mut observed = loop {
            let value = self.query(&query::observation(number, metadata.head.as_str(), after.as_deref())).await?;
            let pr = value.get("pullRequest").filter(|v| v.is_object()).ok_or_else(incomplete)?;
            if string(pr, "headRefOid")? != metadata.head.as_str()
                || string(pr, "baseRefOid")? != metadata.base.as_str()
            {
                return Err(changed());
            }
            let row = parse::observation(pr, metadata)?;
            if matches!(row.pull_request.state, PrState::Merged | PrState::Closed) {
                break row;
            }
            let rollup = &pr["statusCheckRollup"];
            after = None;
            if !rollup.is_null() {
                parse::checks(rollup, pr, &mut checks)?;
                after = next(&rollup["contexts"])?;
            }
            if after.is_none() {
                break row;
            }
        };
        observed.checks = policy.checks(checks);
        for check in &observed.checks {
            use protocol::model::CheckState;
            let counts = &mut observed.pull_request.checks;
            match check.state {
                CheckState::Passed => counts.passed += 1,
                CheckState::Failed => counts.failed += 1,
                CheckState::Pending | CheckState::Unknown => counts.pending += 1,
            }
        }
        Ok(observed)
    }
}

pub(super) fn incomplete() -> ApiError {
    ApiError::new(ErrorCode::Worker, "GitHub's response was incomplete; no successful observation was recorded")
}

pub(super) fn changed() -> ApiError {
    ApiError::new(ErrorCode::Conflict, "the pull requests changed while being read; their checks will be read again")
}

pub(super) fn string(value: &Value, key: &str) -> Result<String, ApiError> {
    value[key].as_str().map(str::to_owned).ok_or_else(incomplete)
}

pub(super) fn array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>, ApiError> {
    value[key].as_array().ok_or_else(incomplete)
}

pub(super) fn next(connection: &Value) -> Result<Option<String>, ApiError> {
    match connection["pageInfo"]["hasNextPage"].as_bool() {
        Some(false) => Ok(None),
        Some(true) => string(&connection["pageInfo"], "endCursor").map(Some),
        None => Err(incomplete()),
    }
}

pub(super) async fn gh(path: &str, args: &[&str]) -> Result<Value, ApiError> {
    let output = run_gh(path, args).await.map_err(|e| match e {
        Unran::Missing => ApiError::new(ErrorCode::Unavailable, "gh isn't installed"),
        Unran::Failed(why) => ApiError::new(ErrorCode::Worker, why),
    })?;
    if !output.status.success() {
        let why = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        // gh returns nonzero for partial GraphQL responses. Preserve their usage before returning their errors.
        if let Ok(value) = serde_json::from_slice::<Value>(&output.stdout)
            && value.get("errors").is_some()
        {
            return Ok(value);
        }
        return Err(failed(&why, output.status.code()));
    }
    serde_json::from_slice(&output.stdout).map_err(|_| incomplete())
}

pub(super) fn count(value: &Value, key: &str) -> Result<u32, ApiError> {
    value[key].as_u64().and_then(|n| u32::try_from(n).ok()).ok_or_else(incomplete)
}

pub(super) fn failed(why: &str, exit_code: Option<i32>) -> ApiError {
    let rate_limited = why.to_lowercase().contains("rate limit");
    let denied = exit_code == Some(4)
        || [
            "HTTP 401",
            "HTTP 403",
            "HTTP 404",
            "Could not resolve to a Repository",
            "Could not resolve to a PullRequest",
            "Resource not accessible by integration",
            "FORBIDDEN",
            "NOT_FOUND",
            "UNAUTHORIZED",
        ]
        .iter()
        .any(|word| why.contains(word));
    let unsupported =
        ["Field 'stack' doesn't exist", "Field 'stackEntry' doesn't exist", "Field 'isRequired' doesn't exist"]
            .iter()
            .any(|word| why.contains(word));
    let code = if unsupported {
        ErrorCode::Unsupported
    } else if denied && !rate_limited {
        ErrorCode::Unavailable
    } else {
        ErrorCode::Worker
    };
    ApiError::new(code, format!("GitHub couldn't be read: {why}"))
}
