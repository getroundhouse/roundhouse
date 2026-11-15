//! Rules from `AGENTS.md`, `docs/architecture.md` and `docs/rust.md`, checked against the real
//! dependency graph and source tree, so breaking one fails the build rather than waiting to be noticed
//! in review.

#![cfg(test)]

mod dependencies;
mod licenses;
mod rust_standards;
mod workspace;
