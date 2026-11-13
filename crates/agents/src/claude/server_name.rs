/// The name the adapter gives the server of the worker's tools in the CLI's config, which the CLI puts in the name of
/// each of them, such as
/// `mcp__roundhouse__get_pull_request`, and in the description of a task one of them answers with, such as
/// `roundhouse/wait_for_pull_request`.
pub(super) const TOOLS_SERVER: &str = "roundhouse";
