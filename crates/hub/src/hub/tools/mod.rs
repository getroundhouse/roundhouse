//! The worker's tools, which its agents call, and what the hub answers for them. A worker asks only for its own
//! workspace, which the hub knows, so a tool reaches that workspace and nothing else.

#[cfg(test)]
mod tests;

mod tool_calls;
