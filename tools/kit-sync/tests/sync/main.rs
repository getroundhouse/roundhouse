//! `kit-sync`, against a repository standing in for upstream's, with releases tagged in it, and one standing in for
//! this one.

#![cfg(test)]

mod bringing;
mod conflicts;
mod fixture;
mod manifest;
mod patches;
mod refusals;
mod settings;
