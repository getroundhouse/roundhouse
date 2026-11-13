//! Checks the adapter against real `claude` output. `../fixtures/claude.jsonl` was captured from the
//! CLI with the adapter's own flags, by `tools/smoke/capture_claude_fixture.py`. Rerun that after a
//! CLI upgrade: a failure here means the CLI changed in a way the adapter relies on.

#![cfg(test)]

mod denials;
mod full_access;
mod mcp_task;
mod modes;

mod normalization;

pub(super) use normalization::{blocks, events};
