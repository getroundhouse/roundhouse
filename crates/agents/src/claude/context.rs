//! How full the CLI's context is, from what it answers `get_context_usage`. `docs/claude-cli.md` has how it behaves.

use protocol::model::{ContextPart, ContextPartKind, ContextUsage};
use serde_json::Value;

/// More parts than the CLI names, and longer than any name it gives one, so a record can't grow without limit on a
/// reply that lists far more or names them at length. It goes out with every notice of its session.
const MAX_PARTS: usize = 32;
const MAX_NAME_CHARS: usize = 64;

/// What the reply says the context holds, or `None` for one that doesn't say how much is used and of how much. Only
/// this is kept of the reply, which also lists the user's skills, memory files and MCP tools by name and path.
pub fn context_usage(reply: &Value) -> Option<ContextUsage> {
    let used_tokens = reply.get("totalTokens")?.as_u64()?;
    let max_tokens = reply.get("maxTokens")?.as_u64()?;
    let categories = reply.get("categories").and_then(Value::as_array);
    let parts = categories.map(|all| all.iter().filter_map(part).take(MAX_PARTS).collect()).unwrap_or_default();
    Some(ContextUsage { used_tokens, max_tokens, parts })
}

/// A category that has a name and holds something. The CLI lists some that are empty, which say nothing.
fn part(category: &Value) -> Option<ContextPart> {
    let name = category.get("name")?.as_str()?.trim();
    let tokens = category.get("tokens")?.as_u64()?;
    let kind = match category.get("kind").and_then(Value::as_str) {
        Some("used") => ContextPartKind::Used,
        Some("deferred") => ContextPartKind::Deferred,
        Some("buffer") => ContextPartKind::Buffer,
        Some("free") => ContextPartKind::Free,
        _ => ContextPartKind::Unknown,
    };
    (!name.is_empty() && tokens > 0).then(|| ContextPart {
        name: name.chars().take(MAX_NAME_CHARS).collect(),
        tokens,
        kind,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn part(name: &str, tokens: u64, kind: ContextPartKind) -> ContextPart {
        ContextPart { name: name.to_string(), tokens, kind }
    }

    #[test]
    fn a_reply_gives_what_is_used_of_how_much_and_each_part_in_the_clis_order() {
        let reply = json!({
            "categories": [
                {"id": "system_prompt", "name": "System prompt", "tokens": 3364, "color": "promptBorder", "kind": "used"},
                {"id": "system_tools_deferred", "name": "System tools (deferred)", "tokens": 19358, "isDeferred": true,
                 "kind": "deferred"},
                {"id": "messages", "name": "Messages", "tokens": 9138, "kind": "used"},
                {"id": "free_space", "name": "Free space", "tokens": 942_228, "kind": "free"},
            ],
            "totalTokens": 24772,
            "maxTokens": 1_000_000,
            "percentage": 2,
            "gridRows": [[{"categoryName": "Free space"}]],
            "skills": {"skillFrontmatter": [{"name": "deploy", "path": "/home/user/.claude/skills/deploy/SKILL.md"}]},
        });

        let context = context_usage(&reply).unwrap();

        assert_eq!((context.used_tokens, context.max_tokens), (24772, 1_000_000));
        assert_eq!(
            context.parts,
            [
                part("System prompt", 3364, ContextPartKind::Used),
                part("System tools (deferred)", 19358, ContextPartKind::Deferred),
                part("Messages", 9138, ContextPartKind::Used),
                part("Free space", 942_228, ContextPartKind::Free)
            ]
        );
    }

    #[test]
    fn a_reply_that_doesnt_say_how_much_is_used_or_of_how_much_gives_nothing() {
        assert_eq!(context_usage(&Value::Null), None);
        assert_eq!(context_usage(&json!({"totalTokens": 5})), None);
        assert_eq!(context_usage(&json!({"maxTokens": 5, "totalTokens": "many"})), None);
    }

    #[test]
    fn an_empty_or_unnamed_part_is_left_out_and_a_long_list_is_cut_off() {
        let mut categories = vec![json!({"name": "MCP tools", "tokens": 0}), json!({"name": " ", "tokens": 4})];
        categories.extend((0..40).map(|n| json!({"name": format!("Part {n}"), "tokens": 1})));
        let reply = json!({"totalTokens": 40, "maxTokens": 100, "categories": categories});

        let context = context_usage(&reply).unwrap();

        assert_eq!(context.parts.len(), MAX_PARTS);
        assert_eq!(context.parts[0], part("Part 0", 1, ContextPartKind::Unknown));
    }

    #[test]
    fn a_long_name_is_cut_off() {
        let reply = json!({"totalTokens": 1, "maxTokens": 2, "categories": [{"name": "é".repeat(500), "tokens": 1}]});

        assert_eq!(context_usage(&reply).unwrap().parts[0].name, "é".repeat(MAX_NAME_CHARS));
    }

    #[test]
    fn a_reply_with_no_categories_still_says_how_full_the_context_is() {
        let context = context_usage(&json!({"totalTokens": 40, "maxTokens": 100})).unwrap();

        assert_eq!(context, ContextUsage { used_tokens: 40, max_tokens: 100, parts: Vec::new() });
    }
}
