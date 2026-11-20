//! Rules from `docs/standards/dependencies.md`, `docs/principles/layers.md`, `docs/standards/rust.md`,
//! `docs/standards/components.md` and `docs/standards/documentation.md`, checked against the real dependency graph,
//! source tree and docs, so breaking one fails the build rather than waiting to be noticed in review.

#![cfg(test)]

mod dependencies;
mod design_system;
mod docs;
mod environments;
mod icons;
mod licenses;
mod rust_standards;
mod waits;
mod workspace;
