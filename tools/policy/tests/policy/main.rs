//! Rules from `docs/standards/dependencies.md`, `docs/architecture.md`, `docs/standards/rust.md` and
//! `docs/standards/components.md`, checked against the real dependency graph and source tree, so breaking one fails
//! the build rather than waiting to be noticed in review.

#![cfg(test)]

mod dependencies;
mod design_system;
mod environments;
mod icons;
mod licenses;
mod rust_standards;
mod waits;
mod workspace;
