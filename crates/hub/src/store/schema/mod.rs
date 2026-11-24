//! The database's schema, and opening it.

#[cfg(test)]
use super::StoreError;
#[cfg(test)]
use rusqlite::Connection;

#[cfg(test)]
mod before;
mod migrations;

#[cfg(test)]
pub(crate) use before::{
    AT_THE_FLOOR, BEFORE_ACCOUNT_IDENTITY_OBSERVED, BEFORE_ACCOUNT_LIMITS_OBSERVED, BEFORE_APPS_OBSERVED,
    BEFORE_BRANCHES_OBSERVED, BEFORE_GITHUB_ACCOUNT_REMOVAL, BEFORE_GITHUB_LEFT_TO_MACHINE, BEFORE_GITHUB_SET_UP,
    BEFORE_GITHUB_USERS_OBSERVED, BEFORE_PULL_REQUEST_LINKS, BEFORE_REACH_OBSERVED, BEFORE_SESSION_CACHE_OBSERVED,
    BEFORE_SESSION_CONTEXT_OBSERVED, BEFORE_TASK_KINDS_GIVEN, BEFORE_TITLES_TIDIED, BEFORE_WAIT_READ_OBSERVED,
    BEFORE_WAITS_AS_FACETS, BEFORE_WORKER_BUILD_OBSERVED, BEFORE_WORKSPACE_PLACEMENT,
};
#[cfg(test)]
use migration::Migration;
#[cfg(test)]
use migrations::MIGRATIONS;

#[cfg(test)]
mod tests;

mod migration;
#[cfg(test)]
pub(crate) use migration::migrate_to;
#[cfg(test)]
use migration::{KNOWN, STARTS_AT, migrate_through, schema_version};
pub(crate) use migration::{migrate, open_connection};
