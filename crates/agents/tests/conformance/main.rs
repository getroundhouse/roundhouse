//! Captured JSON-RPC conformance fixtures replayed through the supervised Codex and Antigravity adapters.

#![cfg(test)]

#[cfg(unix)]
mod antigravity;
#[cfg(unix)]
mod codex;
