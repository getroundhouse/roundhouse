//! Rules from `AGENTS.md`, `docs/architecture.md`, `docs/rust.md` and `docs/component-library.md`, checked against the real
//! dependency graph and source tree, so breaking one fails the build rather than waiting to be noticed
//! in review.

#![cfg(test)]

mod dependencies;
mod design_system;
mod environments;
mod licenses;
mod rust_standards;
mod waits;
mod workspace;
