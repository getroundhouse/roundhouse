//! How full Codex's context is, and its account's limits, as it says while it works.

use protocol::event::EventBody;
use protocol::model::{ContextUsage, LimitWindow, UsageLimit};
use serde_json::json;

use super::script::{bodies, play, turn};
use crate::AgentOutput;
use crate::codex::usage::limits_of;

#[tokio::test]
async fn the_newest_request_says_how_full_the_context_is_and_the_limits_are_an_event() {
    let tokens = |total: u64| {
        json!({"method":"thread/tokenUsage/updated","params":{"threadId":"thread","turnId":"root-turn",
            "tokenUsage":{"last":{"totalTokens":total,"inputTokens":total - 10,"cachedInputTokens":0,
                "outputTokens":10,"reasoningOutputTokens":0},
            "total":{"totalTokens":total * 3,"inputTokens":0,"cachedInputTokens":0,"outputTokens":0,
                "reasoningOutputTokens":0},"modelContextWindow":258_400}}})
    };
    let limits = json!({"method":"account/rateLimits/updated","params":{"rateLimits":{"limitId":"codex",
        "primary":{"usedPercent":67,"windowDurationMins":10_080,"resetsAt":1_795_406_516},"secondary":null}}});
    let actions = vec![turn("thread", "root-turn", "inProgress"), tokens(1000), tokens(2000), limits, {
        turn("thread", "root-turn", "completed")
    }];

    let output = play(actions).await;
    let contexts: Vec<_> = output
        .iter()
        .filter_map(|out| if let AgentOutput::Context(context) = out { Some(context) } else { None })
        .collect();
    let context = |used_tokens| ContextUsage {
        used_tokens,
        max_tokens: 258_400,
        model_tokens: None,
        max_set_by: None,
        parts: Vec::new(),
    };
    assert_eq!(contexts, [&context(1000), &context(2000)], "what the newest request held, not the thread's total");
    let week = UsageLimit {
        window: LimitWindow::SevenDays,
        model: None,
        used_percent: 67,
        resets_at_ms: Some(1_795_406_516_000),
        heard_at_ms: None,
    };
    assert!(bodies(&output).contains(&&EventBody::AgentLimits { limits: vec![week] }));
    assert!(
        !bodies(&output)
            .iter()
            .any(|body| matches!(body, EventBody::AgentOther { kind, .. } if kind != "test/request"))
    );
}

#[test]
fn only_the_windows_of_the_limit_on_every_model_are_named() {
    let window = |minutes: u64, used: u64| json!({"usedPercent":used,"windowDurationMins":minutes,"resetsAt":10});
    let both = json!({"limitId":"codex","primary":window(10_080, 40),"secondary":window(300, 150)});
    let named: Vec<_> = limits_of(&both).iter().map(|limit| (limit.window, limit.used_percent)).collect();
    assert_eq!(named, [(LimitWindow::FiveHours, 100), (LimitWindow::SevenDays, 40)], "shortest first, at most all");

    assert!(limits_of(&json!({"limitId":"codex","primary":window(60, 5)})).is_empty(), "a window with no name");
    assert!(limits_of(&json!({"limitId":"gpt-6","primary":window(300, 5)})).is_empty(), "a model's own limit");
    assert_eq!(limits_of(&json!({"primary":window(300, 5)})).len(), 1, "an update that leaves out its ID");
}
