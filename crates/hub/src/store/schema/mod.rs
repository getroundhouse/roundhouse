//! The database's schema, and opening it.

#[cfg(test)]
use super::StoreError;
#[cfg(test)]
use rusqlite::Connection;
#[cfg(test)]
use std::path::Path;

#[cfg(test)]
mod before;
mod migrations;

#[cfg(test)]
pub(crate) use before::{
    BEFORE_ACCOUNT_DEFAULTS, BEFORE_ACCOUNTS, BEFORE_ACCOUNTS_USED, BEFORE_ALLOWING_ASKED, BEFORE_CHOSEN_MODELS,
    BEFORE_CONTEXT, BEFORE_EFFORT, BEFORE_FULL_ACCESS, BEFORE_HUB_SETTINGS, BEFORE_INSTANCES, BEFORE_LIMIT_MODELS,
    BEFORE_LOGINS, BEFORE_NEW_WORKSPACE_DRAFT_NAMES, BEFORE_PULL_REQUESTS, BEFORE_RECENT_WORK, BEFORE_REPLY_OUTCOME,
    BEFORE_RUNS, BEFORE_SUBAGENT_CONTEXT_TOKENS, BEFORE_SUBAGENT_MODELS, BEFORE_SUBAGENT_RECORDS, BEFORE_SUBAGENTS,
    BEFORE_TITLES, BEFORE_UNUSED_DELETED, BEFORE_USER_ACTIVITY, BEFORE_WAITING_ON, BEFORE_WORKSPACE_ACTIVITY,
};
#[cfg(test)]
use migrations::MIGRATIONS;

#[cfg(test)]
mod tests;

mod migration;
#[cfg(test)]
pub(crate) use migration::migrate_to;
pub(crate) use migration::{migrate, open_connection};
#[cfg(test)]
use migration::{migrate_through, schema_version};
