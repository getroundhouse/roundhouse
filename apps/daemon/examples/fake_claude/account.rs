//! What the real CLI says of its context, its account's limits and its login, with made-up numbers.

use std::path::Path;

use serde_json::{Value, json};

/// What the system prompt and tools take before anything is said, and what each line a conversation has seen adds.
const BASE_TOKENS: u64 = 15_000;
const TOKENS_A_LINE: u64 = 1_000;
const MAX_TOKENS: u64 = 200_000;
/// The start of 2100, in seconds, so the limit hasn't reset whenever a test runs.
const RESETS_AT_S: u64 = 4_102_444_800;

/// What `get_context_usage` answers: each part of the context, what's used of how much, and more that the worker
/// doesn't keep, such as the grid the real CLI draws and the user's skills by path. The conversation's file says how
/// much has been said, so a resumed conversation is as full as it was. A long one fills the context and goes on,
/// since nothing here compacts it.
pub(crate) fn context_usage(conversation: &Path) -> Value {
    let said = std::fs::read_to_string(conversation).unwrap_or_default();
    let messages = said.lines().count() as u64 * TOKENS_A_LINE;
    let used = BASE_TOKENS + messages;
    json!({
        "categories": [
            {"id": "system_prompt", "name": "System prompt", "tokens": 3_000, "color": "promptBorder", "kind": "used"},
            {"id": "system_tools", "name": "System tools", "tokens": 12_000, "color": "inactive", "kind": "used"},
            {"id": "system_tools_deferred", "name": "System tools (deferred)", "tokens": 19_000, "color": "inactive",
             "isDeferred": true, "kind": "deferred"},
            {"id": "mcp_tools", "name": "MCP tools", "tokens": 0, "color": "inactive", "kind": "used"},
            {"id": "messages", "name": "Messages", "tokens": messages, "color": "purple", "kind": "used"},
            {"id": "autocompact_buffer", "name": "Autocompact buffer", "tokens": 33_000, "color": "inactive",
             "kind": "buffer"},
            {"id": "free_space", "name": "Free space", "tokens": MAX_TOKENS.saturating_sub(33_000 + used), "color": "promptBorder",
             "kind": "free"},
        ],
        "totalTokens": used,
        "maxTokens": MAX_TOKENS,
        "rawMaxTokens": MAX_TOKENS,
        "percentage": used * 100 / MAX_TOKENS,
        "gridRows": [[{"color": "promptBorder", "isFilled": true, "categoryName": "Free space", "tokens": 1}]],
        "model": super::current_model(),
        "skills": {"totalSkills": 1, "tokens": 90, "skillFrontmatter": [
            {"name": "deploy", "source": "userSettings", "path": "/home/user/.claude/skills/deploy/SKILL.md", "tokens": 90},
        ]},
    })
}

/// The line the real CLI writes once a turn has asked the model for something, saying how much of the account's
/// week is used.
pub(crate) fn rate_limit_event(session_id: &str) -> Value {
    json!({"type": "rate_limit_event", "session_id": session_id, "rate_limit_info": {
        "status": "allowed", "resetsAt": RESETS_AT_S, "rateLimitType": "seven_day", "overageStatus": "allowed",
        "isUsingOverage": false,
        "unifiedWindows": {
            "seven_day": {"utilization": 0.04, "resetsAt": RESETS_AT_S, "status": "allowed"},
            "seven_day_overage_included": {"utilization": 0.04, "resetsAt": RESETS_AT_S, "status": "allowed"},
        },
    }})
}

/// Whether the agent hears of its account's limits as it works. It doesn't once `$FAKE_CLAUDE_STATE/no-limits` exists,
/// as for a login that has none, so a test can tell what the hub learns when an agent launches from what it learns
/// when the agent hears of a limit. `conversation` is in the state directory.
pub(crate) fn hears_limits(conversation: &Path) -> bool {
    conversation.parent().is_none_or(|state| !state.join("no-limits").exists())
}

/// What `claude auth status --json` prints: whatever `$FAKE_CLAUDE_STATE/auth-status` holds, so a test can log in as
/// someone else, log out or make the answer unreadable, or else someone logged in with a subscription. Each call adds
/// a line to `$FAKE_CLAUDE_STATE/auth-status-calls` once it has read what to say, so a test knows what it was told.
pub(crate) fn auth_status(state: Option<&Path>) -> String {
    let set = state.and_then(|state| std::fs::read_to_string(state.join("auth-status")).ok());
    if let Some(state) = state {
        std::fs::create_dir_all(state).unwrap();
        let calls = std::fs::OpenOptions::new().create(true).append(true).open(state.join("auth-status-calls"));
        calls.and_then(|mut calls| std::io::Write::write_all(&mut calls, b"asked\n")).unwrap();
    }
    set.unwrap_or_else(|| {
        json!({
            "loggedIn": true, "authMethod": "claude.ai", "apiProvider": "firstParty", "email": "ada@example.com",
            "orgId": "00000000-0000-0000-0000-000000000000", "orgName": "Ada's Organization",
            "subscriptionType": "max", "planTier": "max_20x",
        })
        .to_string()
    })
}
