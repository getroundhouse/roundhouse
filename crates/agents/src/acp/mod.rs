//! Agent Client Protocol (ACP 1.0) wire types and translators for an adapter that speaks it, which tolerate what a
//! message leaves out. Some read what `agy` sends beyond ACP, and say so, such as a tool call's `name` and `args`, and
//! a question's `<key>_other` field and options numbered from 1, which another agent's ACP server may not send.

mod elicitation;
mod session;
#[cfg(test)]
mod tests;
mod updates;

pub(crate) use elicitation::{
    FieldSpec, PermissionRequest, elicitation_accept, elicitation_cancel, parse_elicitation, parse_permission_request,
    permission_cancelled, permission_selected,
};
pub(crate) use session::{
    ConfigOption, config_offers_value, initialize_params, mcp_http_server, parse_agent_version, parse_session_response,
    prompt_blocks,
};
pub(crate) use updates::{
    SessionUpdate, StopReason, ToolCallWire, ToolKindWire, ToolStatusWire, parse_prompt_response, parse_session_update,
};
