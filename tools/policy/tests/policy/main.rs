//! Rules from `docs/standards/dependencies.md`, `docs/principles/layers.md`, `docs/standards/rust.md`,
//! `docs/standards/components.md`, `docs/standards/documentation.md` and `docs/principles/observability.md`, checked against the real dependency graph,
//! source tree and docs, so breaking one fails the build rather than waiting to be noticed in review.

#![cfg(test)]

mod dependencies;
mod design;
mod design_paths;
mod design_system;
mod docs;
mod environments;
mod icons;
mod licenses;
mod logs;
mod rust_standards;
mod waits;
mod workspace;
