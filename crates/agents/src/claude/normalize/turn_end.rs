//! The end of a turn, which the CLI says in a `result` line: how it ended, how long it took, and how many tokens the
//! agent's own requests took. Which models the agent's own replies came from the `result` doesn't say, so the replies
//! do, as they pass.

use protocol::event::{EventBody, TokenUsage, TurnOutcome};
use protocol::model::ModelName;
use serde_json::Value;

use super::{str_of, string_of};

/// What the `result` line says. Its models are the [`TurnModels`]' to add.
pub(super) fn turn_ended(result: &Value) -> EventBody {
    let is_error = result.get("is_error").and_then(Value::as_bool).unwrap_or(false);
    let terminal_reason = str_of(result, "terminal_reason").unwrap_or_default();
    let outcome = if !is_error {
        TurnOutcome::Success
    } else if terminal_reason.starts_with("aborted") {
        TurnOutcome::Cancelled
    } else {
        TurnOutcome::Error
    };
    let errors = result
        .get("errors")
        .and_then(Value::as_array)
        .map_or_default(|items| items.iter().filter_map(|e| e.as_str().map(str::to_string)).collect());
    EventBody::TurnEnded {
        outcome,
        result: string_of(result, "result"),
        errors,
        duration_ms: result.get("duration_ms").and_then(Value::as_u64),
        cost_usd: result.get("total_cost_usd").and_then(Value::as_f64),
        num_turns: result.get("num_turns").and_then(Value::as_u64),
        started_at_ms: None,
        models: Vec::new(),
        tokens: tokens(result),
        only_compacted: false,
        request_failed: false,
    }
}

/// The `result` line's `usage`, which counts the agent's own requests in the turn. It leaves out what subagents
/// asked, which `model_usage_since_last_result` counts with it, as it does some requests the CLI made besides, and
/// `modelUsage` counts every request since the CLI started. `None` without it.
///
/// A count that isn't there, or isn't one, reads as 0 rather than leaving its row out of the turn's card, so a CLI
/// that renames a field shows as a 0 someone notices, rather than a row that quietly goes.
fn tokens(result: &Value) -> Option<TokenUsage> {
    let usage = result.get("usage").filter(|usage| usage.is_object())?;
    let tokens = |key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0);
    Some(TokenUsage {
        input_tokens: tokens("input_tokens"),
        output_tokens: tokens("output_tokens"),
        cache_read_tokens: tokens("cache_read_input_tokens"),
        cache_write_tokens: tokens("cache_creation_input_tokens"),
    })
}

/// The models the agent's own replies came from in the turn going, in the order they first did. The model the turn's
/// `init` names isn't always the one that answers, as in plan mode.
#[derive(Debug, Default)]
pub(super) struct TurnModels(Vec<ModelName>);

impl TurnModels {
    /// Notes the model of one of the agent's own replies. A subagent's is left out, and so is a name that names no
    /// model. One the CLI wrote itself names none by now.
    pub(super) fn note(&mut self, body: &EventBody) {
        let EventBody::Assistant { parent_tool_use_id: None, model: Some(model), .. } = body else { return };
        let Ok(model) = ModelName::try_from(model.clone()) else { return };
        if !self.0.contains(&model) {
            self.0.push(model);
        }
    }

    /// The turn's models, for its end, so the next turn starts with none.
    pub(super) fn take(&mut self) -> Vec<ModelName> {
        std::mem::take(&mut self.0)
    }
}

#[cfg(test)]
mod tests {
    use protocol::model::ToolUseId;
    use serde_json::json;

    use super::*;

    fn reply(model: &str, parent: Option<&str>) -> EventBody {
        EventBody::Assistant {
            message_id: None,
            parent_tool_use_id: parent.map(ToolUseId::from),
            model: Some(model.to_string()),
            blocks: Vec::new(),
            first_index: None,
        }
    }

    #[test]
    fn the_agent_s_own_requests_are_counted_from_the_result_s_usage() {
        let result = json!({"type":"result","usage":{"input_tokens":6,"output_tokens":616,
            "cache_read_input_tokens":70_433,"cache_creation_input_tokens":21_784,"output_tokens_details":{}},
            "model_usage_since_last_result":{"claude-sonnet-5-5":{"inputTokens":40,"outputTokens":9_000}}});

        let counted = tokens(&result);

        let said =
            TokenUsage { input_tokens: 6, output_tokens: 616, cache_read_tokens: 70_433, cache_write_tokens: 21_784 };
        assert_eq!(counted, Some(said));
    }

    #[test]
    fn a_result_without_usage_says_none_and_a_count_that_isnt_there_or_isnt_one_is_0() {
        assert_eq!(tokens(&json!({"type":"result"})), None);
        assert_eq!(tokens(&json!({"type":"result","usage":null})), None);
        let odd = tokens(&json!({"usage":{"input_tokens":"lots","output_tokens":-1,"cache_read_input_tokens":7}}));
        assert_eq!(odd, Some(TokenUsage { cache_read_tokens: 7, ..TokenUsage::default() }));
    }

    #[test]
    fn a_turn_s_models_are_its_own_replies_once_each_in_order_and_the_next_turn_starts_with_none() {
        let mut models = TurnModels::default();

        for body in [
            reply("claude-sonnet-5-5", None),
            reply("claude-fable-5-2", Some("toolu_1")),
            reply("claude-haiku-5-5", None),
            reply("claude-sonnet-5-5", None),
            reply("", None),
        ] {
            models.note(&body);
        }

        let said: Vec<&str> = models.0.iter().map(ModelName::as_str).collect();
        assert_eq!(said, ["claude-sonnet-5-5", "claude-haiku-5-5"], "not a subagent's, or no name");
        assert_eq!(models.take().len(), 2);
        assert!(models.take().is_empty());
    }
}
