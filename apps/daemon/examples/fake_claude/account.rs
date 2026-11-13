//! What the real CLI says of its context, its account's limits and its login, with made-up numbers.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};

/// Where the real CLI reads a token `claude setup-token` made.
const TOKEN_ENV: &str = "CLAUDE_CODE_OAUTH_TOKEN";
/// What the system prompt and tools take before anything is said, and what each line a conversation has seen adds.
const BASE_TOKENS: u64 = 15_000;
const TOKENS_A_LINE: u64 = 1_000;
/// What the context holds, which its settings make a fifth of what its model holds, as a real CLI's can.
const MAX_TOKENS: u64 = 200_000;
pub(crate) const MODEL_TOKENS: u64 = 1_000_000;
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
        "autocompactSource": "settings",
        "percentage": used * 100 / MAX_TOKENS,
        "gridRows": [[{"color": "promptBorder", "isFilled": true, "categoryName": "Free space", "tokens": 1}]],
        "model": super::current_model(),
        "skills": {"totalSkills": 1, "tokens": 90, "skillFrontmatter": [
            {"name": "deploy", "source": "userSettings", "path": "/home/user/.claude/skills/deploy/SKILL.md", "tokens": 90},
        ]},
    })
}

/// The line the real CLI writes once its first turn has asked the model for something, saying how much of the
/// account's week is used. It writes it once a process, so a later turn has to ask with `get_usage`.
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

/// Whether this turn says what's left of the account's limits by itself, as only a process's first does. None does
/// once `$FAKE_CLAUDE_STATE/no-limits` exists, as for a login that has none, so a test can tell what the hub learns
/// when an agent launches from what it learns of the limits. None does either once `limits-unsaid` exists, though
/// `get_usage` still answers, so a test can tell what's asked from what's said. `conversation` is in the state
/// directory.
pub(crate) fn says_limits(conversation: &Path) -> bool {
    static SAID: AtomicBool = AtomicBool::new(false);
    let quiet = ["no-limits", "limits-unsaid"].iter().any(|file| in_state(conversation, file));
    !quiet && !SAID.swap(true, Ordering::SeqCst)
}

/// What `get_usage` answers: the week's limit, as `rate_limit_event` says it but as a whole percent and an ISO time, and
/// more that the worker doesn't keep, such as what the account was used for. A login with no limits says so.
pub(crate) fn usage(conversation: &Path) -> Value {
    if in_state(conversation, "no-limits") {
        return json!({"subscription_type": null, "rate_limits_available": false, "rate_limits": null});
    }
    json!({
        "session": {"total_cost_usd": 0.0, "model_usage": {}},
        "subscription_type": "max",
        "rate_limits_available": true,
        "rate_limits": {
            "five_hour": null,
            "seven_day": {"utilization": 4, "resets_at": "2100-01-01T00:00:00.000000+00:00", "limit_dollars": null},
            "model_scoped": [],
        },
        "behaviors": {"day": {"request_count": 1, "session_count": 1, "skills": [{"name": "deploy"}]}},
    })
}

/// Whether `$FAKE_CLAUDE_STATE` has `file`, which `conversation` is beside.
fn in_state(conversation: &Path, file: &str) -> bool {
    conversation.parent().is_some_and(|state| state.join(file).exists())
}

/// What `claude auth status --json` prints: whatever `$FAKE_CLAUDE_STATE/auth-status` holds, so a test can log in as
/// someone else, log out or make the answer unreadable, or else someone logged in with a subscription. Given a token
/// in `CLAUDE_CODE_OAUTH_TOKEN`, it says only that it's logged in with one, as the real CLI does, since a token doesn't
/// say whose it is. Each call adds a line to `$FAKE_CLAUDE_STATE/auth-status-calls` once it has read what to say, so a
/// test knows what it was told, and the line ends with the token it was given, if any.
pub(crate) fn auth_status(state: Option<&Path>) -> String {
    let set = state.and_then(|state| std::fs::read_to_string(state.join("auth-status")).ok());
    let token = std::env::var(TOKEN_ENV).ok();
    if let Some(state) = state {
        std::fs::create_dir_all(state).unwrap();
        let calls = std::fs::OpenOptions::new().create(true).append(true).open(state.join("auth-status-calls"));
        let asked = token.as_ref().map_or_else(|| "asked\n".to_string(), |token| format!("asked under {token}\n"));
        calls.and_then(|mut calls| std::io::Write::write_all(&mut calls, asked.as_bytes())).unwrap();
    }
    if set.is_none() && token.is_some() {
        return json!({"loggedIn": true, "authMethod": "oauth_token", "apiProvider": "firstParty"}).to_string();
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
