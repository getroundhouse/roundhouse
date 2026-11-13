//! Converts complete GitHub responses into revision-bound observations and effective check requirements.

use std::collections::{BTreeMap, BTreeSet};

use protocol::client::ApiError;
use protocol::model::{
    BranchName, CheckState, Checks, CommitId, LeftQueue, Local, Mergeability, ObservedCheck, ObservedPullRequest,
    PrState, PullRequest, PullRequestNumber,
};
use serde_json::Value;

use super::reading::{array, changed, incomplete, string};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Metadata {
    pub(super) head: CommitId,
    pub(super) base: CommitId,
    pub(super) destination: String,
    pub(super) dependencies: Vec<PullRequestNumber>,
    pub(super) state: PrState,
    position: Option<u64>,
    number: u64,
    merge_commit: Option<String>,
}

impl Metadata {
    pub(super) fn read(pr: &Value) -> Result<Self, ApiError> {
        let stacked = !pr.get("stack").ok_or_else(incomplete)?.is_null();
        let destination = if stacked { string(&pr["stack"], "baseRefName")? } else { string(pr, "baseRefName")? };
        let position = if stacked { Some(pr["stackEntry"]["position"].as_u64().ok_or_else(incomplete)?) } else { None };
        Ok(Self {
            head: string(pr, "headRefOid")?.into(),
            base: string(pr, "baseRefOid")?.into(),
            destination,
            dependencies: Vec::new(),
            state: serde_json::from_value(pr["state"].clone()).map_err(|_| incomplete())?,
            position,
            number: pr["number"].as_u64().ok_or_else(incomplete)?,
            merge_commit: pr["potentialMergeCommit"]["oid"].as_str().map(str::to_owned),
        })
    }

    pub(super) fn include(&mut self, entries: Vec<Value>) -> Result<(), ApiError> {
        let mut lower = BTreeMap::new();
        let mut positions = BTreeSet::new();
        let mut found_self = self.position.is_none();
        for entry in entries {
            let position = entry["position"].as_u64().ok_or_else(incomplete)?;
            if !positions.insert(position) {
                return Err(changed());
            }
            let number = entry["pullRequest"]["number"].as_u64().ok_or_else(incomplete)?;
            let number = PullRequestNumber::try_from(number).map_err(|_| incomplete())?;
            let open = match entry["pullRequest"]["state"].as_str() {
                Some("OPEN") => true,
                Some("MERGED" | "CLOSED") => false,
                _ => return Err(incomplete()),
            };
            found_self |= self.position == Some(position) && self.number == number.get();
            if self.position.is_some_and(|at| position < at) && open {
                lower.insert(position, number);
            }
        }
        if !found_self {
            return Err(incomplete());
        }
        self.dependencies = lower.into_values().collect();
        Ok(())
    }
}

#[derive(Default)]
pub(super) struct Policy {
    required: BTreeSet<(String, Option<i64>)>,
    visible_contexts: BTreeSet<String>,
}

impl Policy {
    pub(super) fn include(&mut self, reference: &Value) -> Result<(), ApiError> {
        // The full classic protection can be hidden to non-admins. RefUpdateRule exposes enforced names,
        // but not provider restrictions; only a reported isRequired check can satisfy those fallback names.
        let visible = reference.get("refUpdateRule").ok_or_else(incomplete)?;
        if !visible.is_null() {
            for context in array(visible, "requiredStatusCheckContexts")? {
                self.visible_contexts.insert(context.as_str().ok_or_else(incomplete)?.to_owned());
            }
        }
        let protection = reference.get("branchProtectionRule").ok_or_else(incomplete)?;
        if !protection.is_null() && protection["requiresStatusChecks"].as_bool().ok_or_else(incomplete)? {
            for check in array(protection, "requiredStatusChecks")? {
                self.required.insert((string(check, "context")?, check["app"]["databaseId"].as_i64()));
            }
        }
        for rule in array(&reference["rules"], "nodes")? {
            let enforcement = string(&rule["repositoryRuleset"], "enforcement")?;
            match enforcement.as_str() {
                "DISABLED" | "EVALUATE" => continue,
                "ACTIVE" => {}
                _ => return Err(incomplete()),
            }
            if rule["type"] == "REQUIRED_STATUS_CHECKS" {
                for check in array(&rule["parameters"], "requiredStatusChecks")? {
                    self.required
                        .insert((string(check, "context")?, check["integrationId"].as_i64().filter(|id| *id > 0)));
                }
            }
        }
        Ok(())
    }

    pub(super) fn checks(&self, checks: Vec<ReportedCheck>) -> Vec<ObservedCheck> {
        let mut latest: BTreeMap<(String, String), ReportedCheck> = BTreeMap::new();
        for check in checks {
            let key = (check.check.name.clone(), check.check.source.clone());
            if let Some(old) = latest.get_mut(&key)
                && old.started == check.started
                && old.check.state != check.check.state
            {
                old.check.state = CheckState::Unknown;
                continue;
            }
            if latest.get(&key).is_none_or(|old| old.started < check.started) {
                latest.insert(key, check);
            }
        }
        let mut out = Vec::new();
        let mut missing = self.required.clone();
        // A visible rule can allow any provider while hidden classic protection is stricter for the same name.
        // Only GitHub's original isRequired answer proves that fallback requirement, before local promotion.
        let mut missing_visible = self.visible_contexts.clone();
        for (_, mut reported) in latest {
            if reported.check.required {
                missing_visible.remove(&reported.check.name);
            }
            for (name, app) in &self.required {
                if *name == reported.check.name
                    && app.is_none_or(|app| {
                        reported.app == Some(app) || (reported.check.source == "status" && reported.check.required)
                    })
                {
                    reported.check.required = true;
                    missing.remove(&(name.clone(), *app));
                }
            }
            out.push(reported.check);
        }
        missing.extend(missing_visible.into_iter().map(|name| (name, None)));
        out.extend(missing.into_iter().map(|(name, app)| ObservedCheck {
            name,
            source: app.map_or_else(|| "required".to_owned(), |app| format!("app:{app}")),
            url: None,
            state: CheckState::Pending,
            required: true,
        }));
        out
    }
}

pub(super) struct ReportedCheck {
    check: ObservedCheck,
    app: Option<i64>,
    started: String,
}

pub(super) fn checks(rollup: &Value, pr: &Value, out: &mut Vec<ReportedCheck>) -> Result<(), ApiError> {
    let revision = string(&rollup["commit"], "oid")?;
    if revision != string(pr, "headRefOid")? && pr["potentialMergeCommit"]["oid"].as_str() != Some(&revision) {
        return Err(changed());
    }
    for check in array(&rollup["contexts"], "nodes")? {
        out.push(check_read(check, &revision)?);
    }
    Ok(())
}

fn check_read(check: &Value, revision: &str) -> Result<ReportedCheck, ApiError> {
    let run = match check["__typename"].as_str() {
        Some("CheckRun") => true,
        Some("StatusContext") => false,
        _ => return Err(incomplete()),
    };
    let commit = if run { &check["checkSuite"]["commit"] } else { &check["commit"] };
    if string(commit, "oid")? != revision {
        return Err(changed());
    }
    let name = string(check, if run { "name" } else { "context" })?;
    // GitHub gives a queued CheckRun no start time. Within its suite keep it pending ahead of older runs,
    // then use its start time once it starts; another suite's creation time orders pushes and workflow reruns.
    let started = if run {
        format!("{}|{}", string(&check["checkSuite"], "createdAt")?, check["startedAt"].as_str().unwrap_or("~"))
    } else {
        string(check, "createdAt")?
    };
    let url = check[if run { "url" } else { "targetUrl" }].as_str().map(str::to_owned);
    let app = check["checkSuite"]["app"]["databaseId"].as_i64();
    let workflow = check["checkSuite"]["workflowRun"]["workflow"]["id"].as_str().unwrap_or("");
    let source = if run { format!("app:{app:?}/workflow:{workflow}") } else { "status".to_owned() };
    let state = if run {
        match check["status"].as_str() {
            Some("COMPLETED") => conclusion(check["conclusion"].as_str()),
            Some("QUEUED" | "IN_PROGRESS" | "REQUESTED" | "WAITING" | "PENDING") => CheckState::Pending,
            _ => CheckState::Unknown,
        }
    } else {
        match check["state"].as_str() {
            Some("SUCCESS") => CheckState::Passed,
            Some("ERROR" | "FAILURE") => CheckState::Failed,
            Some("PENDING" | "EXPECTED") => CheckState::Pending,
            _ => CheckState::Unknown,
        }
    };
    let required = check["isRequired"].as_bool().ok_or_else(incomplete)?;
    Ok(ReportedCheck { check: ObservedCheck { name, source, url, state, required }, started, app })
}

fn conclusion(conclusion: Option<&str>) -> CheckState {
    match conclusion {
        Some("SUCCESS" | "NEUTRAL" | "SKIPPED") => CheckState::Passed,
        Some("FAILURE" | "CANCELLED" | "TIMED_OUT" | "ACTION_REQUIRED" | "STALE" | "STARTUP_FAILURE") => {
            CheckState::Failed
        }
        _ => CheckState::Unknown,
    }
}

pub(super) fn observation(pr: &Value, metadata: &Metadata) -> Result<ObservedPullRequest, ApiError> {
    let parse = |key: &str| pr.get(key).cloned().ok_or_else(incomplete);
    let mut pull_request = PullRequest {
        number: pr["number"].as_u64().ok_or_else(incomplete)?,
        url: string(pr, "url")?,
        title: string(pr, "title")?,
        state: serde_json::from_value(parse("state")?).map_err(|_| incomplete())?,
        is_draft: pr["isDraft"].as_bool().ok_or_else(incomplete)?,
        base: Some(BranchName::from(string(pr, "baseRefName")?)),
        merge_state: serde_json::from_value(parse("mergeStateStatus")?).map_err(|_| incomplete())?,
        review: serde_json::from_value(pr["reviewDecision"].as_str().unwrap_or("").into()).map_err(|_| incomplete())?,
        checks: Checks::default(),
        queue: None,
        left_queue: None,
        changes_requested_at_ms: None,
        local: Local::default(),
    };
    if !pr.get("mergeQueueEntry").ok_or_else(incomplete)?.is_null() {
        pull_request.queue =
            Some(serde_json::from_value(pr["mergeQueueEntry"]["state"].clone()).map_err(|_| incomplete())?);
    }
    if let Some(event) = array(&pr["timelineItems"], "nodes")?.last()
        && event["reason"] != "merged"
    {
        pull_request.left_queue =
            Some(LeftQueue { at_ms: time(&event["createdAt"])?, reason: string(event, "reason")? });
    }
    if let Some(review) = array(&pr["reviews"], "nodes")?.last() {
        pull_request.changes_requested_at_ms = Some(time(&review["submittedAt"])?);
    }
    let mergeability = match pr["mergeable"].as_str() {
        Some("MERGEABLE") => Mergeability::Mergeable,
        Some("CONFLICTING") => Mergeability::Conflicting,
        _ => Mergeability::Unknown,
    };
    Ok(ObservedPullRequest {
        pull_request,
        head: metadata.head.clone(),
        base: metadata.base.clone(),
        destination: metadata.destination.as_str().into(),
        dependencies: metadata.dependencies.clone(),
        mergeability,
        linear: linear(pr, metadata)?,
        checks: Vec::new(),
        checks_revision: pr["statusCheckRollup"]["commit"]["oid"].as_str().map(Into::into),
        requirements_known: true,
        checks_known: !pr["statusCheckRollup"].is_null(),
    })
}

fn linear(pr: &Value, metadata: &Metadata) -> Result<Option<bool>, ApiError> {
    if metadata.dependencies.is_empty() {
        return Ok(Some(true));
    }
    let compare = &pr["baseRef"]["compare"];
    if compare.is_null() {
        return Ok(None);
    }
    if string(&compare["baseTarget"], "oid")? != metadata.base.as_str()
        || string(&compare["headTarget"], "oid")? != metadata.head.as_str()
    {
        return Err(changed());
    }
    Ok(Some(compare["behindBy"].as_u64().ok_or_else(incomplete)? == 0))
}

fn time(value: &Value) -> Result<i64, ApiError> {
    chrono::DateTime::parse_from_rfc3339(value.as_str().ok_or_else(incomplete)?)
        .map(|time| time.timestamp_millis())
        .map_err(|_| incomplete())
}
