//! How full the CLI's context is, from what it answers `get_context_usage`, and how much each model holds, from its
//! `result` lines. `docs/claude-cli.md` has how it behaves.

use protocol::model::{ContextPart, ContextPartKind, ContextUsage, MaxSetBy, ModelName};
use protocol::short_text::start_of;
use serde_json::Value;

/// More parts than the CLI names, and longer than any name it gives one, so a record can't grow without limit on a
/// reply that lists far more or names them at length. It goes out with every notice of its session.
const MAX_PARTS: usize = 32;
const MAX_NAME_CHARS: usize = 64;
/// More models than a CLI uses, so what's kept of them can't grow without limit on lines that name far more.
const MAX_MODELS: usize = 16;

/// What the reply says the context holds, or `None` for one that doesn't say how much is used and of how much. Only
/// this is kept of the reply, which also lists the user's skills, memory files and MCP tools by name and path. The
/// reply gives a context that a setting made smaller as if that were all there is, so how much its model holds comes
/// from `windows`.
pub fn context_usage(reply: &Value, windows: &ModelWindows) -> Option<ContextUsage> {
    let used_tokens = reply.get("totalTokens")?.as_u64()?;
    let max_tokens = reply.get("maxTokens")?.as_u64()?;
    let model_tokens = reply.get("model").and_then(Value::as_str).and_then(|model| windows.of(model));
    let max_set_by = reply.get("autocompactSource").and_then(Value::as_str).map(|source| match source {
        "model-default" => MaxSetBy::Model,
        "settings" => MaxSetBy::Settings,
        "env" => MaxSetBy::Environment,
        _ => MaxSetBy::Unknown,
    });
    let categories = reply.get("categories").and_then(Value::as_array);
    let parts = categories.map_or_default(|all| all.iter().filter_map(part).take(MAX_PARTS).collect());
    Some(ContextUsage { used_tokens, max_tokens, model_tokens, max_set_by, parts })
}

/// How much each model holds, as the CLI's newest `result` line said, which is the only kind of line that says.
#[derive(Debug, Default)]
pub struct ModelWindows(Vec<(ModelName, u64)>);

impl ModelWindows {
    /// Takes what a `result` line says each model holds. Its `modelUsage` counts every model the CLI has used since
    /// it started, so it replaces what earlier lines said, unless it has none.
    pub fn heard(&mut self, result: &Value) {
        let Some(models) = result.get("modelUsage").and_then(Value::as_object) else { return };
        let window = |(model, usage): (&String, &Value)| {
            Some((ModelName::try_from(model.clone()).ok()?, usage.get("contextWindow")?.as_u64()?))
        };
        self.0 = models.iter().filter_map(window).take(MAX_MODELS).collect();
    }

    fn of(&self, model: &str) -> Option<u64> {
        self.0.iter().find(|(name, _)| name.as_str() == model).map(|&(_, window)| window)
    }
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
        name: start_of(name, MAX_NAME_CHARS).0.to_owned(),
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

        let context = context_usage(&reply, &ModelWindows::default()).unwrap();

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

    /// What the CLI says of a context that a setting made half its model's, once a turn with that model has ended.
    #[test]
    fn a_context_made_smaller_than_its_model_says_how_much_the_model_holds_and_what_made_it_smaller() {
        let mut windows = ModelWindows::default();
        windows.heard(&json!({"type": "result", "modelUsage": {
            "claude-opus-5-5": {"inputTokens": 4, "contextWindow": 1_000_000},
            "claude-haiku-5-5": {"contextWindow": 200_000},
        }}));
        let reply = |model: &str, source: &str| json!({"totalTokens": 40, "maxTokens": 500_000, "model": model, "autocompactSource": source});
        let said = |reply: &Value| {
            let context = context_usage(reply, &windows).unwrap();
            (context.model_tokens, context.max_set_by)
        };

        assert_eq!(said(&reply("claude-opus-5-5", "settings")), (Some(1_000_000), Some(MaxSetBy::Settings)));
        assert_eq!(said(&reply("claude-opus-5-5", "env")).1, Some(MaxSetBy::Environment));
        assert_eq!(said(&reply("claude-opus-5-5", "model-default")).1, Some(MaxSetBy::Model));
        assert_eq!(said(&reply("claude-opus-5-5", "somewhere-newer")).1, Some(MaxSetBy::Unknown));
        assert_eq!(said(&reply("claude-sonnet-5-5", "settings")).0, None, "no turn with it has ended");
        assert_eq!(said(&json!({"totalTokens": 40, "maxTokens": 500_000})), (None, None), "a CLI that says neither");
    }

    #[test]
    fn what_models_hold_is_what_the_newest_result_said_and_only_so_many_are_kept() {
        let mut windows = ModelWindows::default();
        let opus = |window: Value| json!({"modelUsage": {"claude-opus-5-5": {"contextWindow": window}}});
        windows.heard(&opus(json!(200_000)));
        windows.heard(&opus(json!(1_000_000)));
        assert_eq!(windows.of("claude-opus-5-5"), Some(1_000_000));

        windows.heard(&json!({"type": "result"}));
        assert_eq!(
            windows.of("claude-opus-5-5"),
            Some(1_000_000),
            "a line that says nothing of models changes nothing"
        );
        windows.heard(&opus(json!("lots")));
        assert_eq!(windows.of("claude-opus-5-5"), None, "it's what the newest said, which was no number");

        let many: serde_json::Map<String, Value> =
            (0..40).map(|n| (format!("model-{n}"), json!({"contextWindow": 1}))).collect();
        windows.heard(&json!({"modelUsage": many}));
        assert_eq!(windows.0.len(), MAX_MODELS);
    }

    #[test]
    fn a_reply_that_doesnt_say_how_much_is_used_or_of_how_much_gives_nothing() {
        assert_eq!(context_usage(&Value::Null, &ModelWindows::default()), None);
        assert_eq!(context_usage(&json!({"totalTokens": 5}), &ModelWindows::default()), None);
        assert_eq!(context_usage(&json!({"maxTokens": 5, "totalTokens": "many"}), &ModelWindows::default()), None);
    }

    #[test]
    fn an_empty_or_unnamed_part_is_left_out_and_a_long_list_is_cut_off() {
        let mut categories = vec![json!({"name": "MCP tools", "tokens": 0}), json!({"name": " ", "tokens": 4})];
        categories.extend((0..40).map(|n| json!({"name": format!("Part {n}"), "tokens": 1})));
        let reply = json!({"totalTokens": 40, "maxTokens": 100, "categories": categories});

        let context = context_usage(&reply, &ModelWindows::default()).unwrap();

        assert_eq!(context.parts.len(), MAX_PARTS);
        assert_eq!(context.parts[0], part("Part 0", 1, ContextPartKind::Unknown));
    }

    #[test]
    fn a_long_name_is_cut_off() {
        let reply = json!({"totalTokens": 1, "maxTokens": 2, "categories": [{"name": "é".repeat(500), "tokens": 1}]});

        assert_eq!(context_usage(&reply, &ModelWindows::default()).unwrap().parts[0].name, "é".repeat(MAX_NAME_CHARS));
    }

    #[test]
    fn a_reply_with_no_categories_still_says_how_full_the_context_is() {
        let context = context_usage(&json!({"totalTokens": 40, "maxTokens": 100}), &ModelWindows::default()).unwrap();

        assert_eq!(
            context,
            ContextUsage { used_tokens: 40, max_tokens: 100, model_tokens: None, max_set_by: None, parts: Vec::new() }
        );
    }
}
