//! Converts complete GitHub responses into revision-bound observations and effective check requirements.

use std::collections::{BTreeMap, BTreeSet};

use protocol::client::ApiError;
use protocol::model::{CheckState, CommitId, ObservedCheck, PrState, PullRequestNumber, Review};
use serde_json::Value;

use super::said::{array, changed, incomplete, string};

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

/// A branch's rules for the checks a pull request into it needs: those its protection and its rulesets require, by
/// name and the app that has to report them, if one does, and those GitHub shows of a protection it hides the rest of.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Policy {
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

/// A check GitHub reported, as read, before a branch's rules say whether it's required.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReportedCheck {
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

/// Where `pr`'s review stands, by its `reviewDecision`, which GitHub leaves empty where the repository asks for none.
pub(crate) fn review(pr: &Value) -> Result<Review, ApiError> {
    serde_json::from_value(pr["reviewDecision"].as_str().unwrap_or("").into()).map_err(|_| incomplete())
}

/// When `value`, a time as GitHub writes one, such as `2026-01-01T00:00:00Z`, was, in milliseconds since the epoch.
pub(crate) fn time(value: &Value) -> Result<i64, ApiError> {
    value.as_str().ok_or_else(incomplete)?.parse().map(jiff::Timestamp::as_millisecond).map_err(|_| incomplete())
}
