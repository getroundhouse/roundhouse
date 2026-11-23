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
    BEFORE_ACCOUNT_IDENTITY_OBSERVED, BEFORE_ACCOUNT_LIMITS_OBSERVED, BEFORE_GITHUB_ACCOUNT_REMOVAL,
    BEFORE_GITHUB_LEFT_TO_MACHINE, BEFORE_GITHUB_SET_UP, BEFORE_PULL_REQUEST_LINKS, BEFORE_WAITS_AS_FACETS,
    BEFORE_WORKSPACE_PLACEMENT,
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
