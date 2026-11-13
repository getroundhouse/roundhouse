//! Checks the adapter against real `claude` output. `../fixtures/claude.jsonl` was captured from the
//! CLI with the adapter's own flags, by `tools/smoke/capture_claude_fixture.py`. Rerun that after a
//! CLI upgrade: a failure here means the CLI changed in a way the adapter relies on.

#![cfg(test)]

use std::collections::HashMap;

use agents::claude::{Normalizer, Parsed};
use protocol::event::{
    AgentRunState, ApprovalOutcome, Asking, CallKind, ContentBlock, EventBody, FileChange, TurnOutcome,
};
use protocol::model::{ApprovalId, CacheRequest, CacheTtl, PermissionMode, ToolUseId};
use protocol::tail::{BlockKind, Tail};
use serde_json::Value;

mod mcp_task;
mod modes;

const FIXTURE: &str = include_str!("../fixtures/claude.jsonl");

fn parsed() -> Vec<Parsed> {
    let mut normalizer = Normalizer::default();
    FIXTURE.lines().map(|line| normalizer.line(line).parsed).collect()
}

pub(super) fn events() -> Vec<EventBody> {
    parsed()
        .into_iter()
        .filter_map(|p| match p {
            Parsed::Event(body) | Parsed::EventWithImages { body, .. } => Some(body),
            _ => None,
        })
        .collect()
}

/// Every block of what the agent wrote and what its tools sent back, in order.
pub(super) fn blocks() -> Vec<ContentBlock> {
    events()
        .into_iter()
        .filter_map(|e| match e {
            EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. } => Some(blocks),
            _ => None,
        })
        .flatten()
        .collect()
}

#[test]
fn every_line_is_understood_or_deliberately_kept() {
    for (i, item) in parsed().into_iter().enumerate() {
        match item {
            Parsed::Event(EventBody::AgentOther { kind, .. }) => {
                assert_ne!(kind, "unparsed", "line {i} isn't JSON");
                // New kinds are fine as long as they're recorded, but these need real handling.
                assert!(!["assistant", "user", "result", "stream_event"].contains(&kind.as_str()), "line {i}: {kind}");
            }
            Parsed::UnsupportedControl { subtype, .. } => panic!("line {i}: the CLI asked for {subtype}"),
            _ => {}
        }
    }
}

#[test]
fn init_reports_what_the_hub_shows_and_relies_on() {
    let init = events().into_iter().find(|e| matches!(e, EventBody::AgentInit { .. })).unwrap();
    let EventBody::AgentInit { agent_session_id, model, cwd, permission_mode, agent_version } = init else {
        unreachable!()
    };
    assert!(agent_session_id.is_some_and(|id| !id.as_str().is_empty()));
    assert!(model.is_some_and(|m| m.contains("haiku")));
    assert_eq!(cwd.as_deref(), Some("/tmp/fixture"));
    // Captured launched with `--permission-mode manual`, which the CLI reports as `default`, a mode a chat can't have.
    assert_eq!(permission_mode, Some(PermissionMode::Unknown));
    assert!(agent_version.is_some_and(|v| !v.is_empty()));
}

#[test]
fn approvals_carry_what_a_client_must_show_and_the_cli_must_get_back() {
    let approvals: Vec<_> = events()
        .into_iter()
        .filter_map(|e| match e {
            EventBody::ApprovalRequested {
                approval_id,
                tool_name,
                tool_use_id,
                input,
                description,
                suggestions,
                asking,
                ..
            } => Some((approval_id, tool_name, tool_use_id, input, description, suggestions, asking)),
            _ => None,
        })
        .collect();
    let bash: Vec<_> = approvals.iter().filter(|(_, tool, ..)| tool == "Bash").collect();
    assert_eq!(bash.len(), 3);
    for (id, _, tool_use_id, input, description, suggestions, asking) in bash {
        assert!(!id.as_str().is_empty());
        assert!(tool_use_id.as_ref().is_some_and(|t| t.as_str().starts_with("toolu_")));
        assert!(input["command"].as_str().is_some_and(|c| c.contains("echo")));
        assert!(description.is_some());
        assert!(suggestions.is_array());
        assert_eq!(asking.as_deref(), Some(&Asking::ToolUse { call: Some(CallKind::Command) }));
    }
}

#[test]
fn interrupting_during_an_approval_cancels_it_and_ends_the_turn_as_cancelled() {
    let events = events();
    let asked: Vec<&ApprovalId> = events
        .iter()
        .filter_map(|e| match e {
            EventBody::ApprovalRequested { approval_id, tool_name, .. } if tool_name != "AskUserQuestion" => {
                Some(approval_id)
            }
            _ => None,
        })
        .collect();
    let cancelled: Vec<&ApprovalId> = events
        .iter()
        .filter_map(|e| match e {
            EventBody::ApprovalResolved { approval_id, outcome: ApprovalOutcome::Cancelled, .. } => Some(approval_id),
            _ => None,
        })
        .collect();
    assert!(cancelled.contains(&asked[2]), "the third Bash approval, which the capture interrupts");
    let outcomes: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            EventBody::TurnEnded { outcome, .. } => Some(*outcome),
            _ => None,
        })
        .collect();
    assert_eq!(outcomes[..3], [TurnOutcome::Success, TurnOutcome::Success, TurnOutcome::Cancelled]);
}

#[test]
fn the_cli_answers_changes_and_says_why_it_refuses() {
    let replies: Vec<(String, Option<String>)> = parsed()
        .into_iter()
        .filter_map(|p| match p {
            Parsed::ControlReply { request_id, answer } => Some((request_id, answer.err())),
            _ => None,
        })
        .collect();
    let reply = |id: &str| replies.iter().find(|(request_id, _)| request_id == id).map(|(_, error)| error.clone());
    assert_eq!(reply("mode-1"), Some(None));
    assert!(reply("model-1").flatten().is_some_and(|error| error.contains("not found")));
}

#[test]
fn a_plan_comes_for_approval_with_its_text() {
    let plan = events().into_iter().find_map(|e| match e {
        EventBody::ApprovalRequested { tool_name, input, asking, .. } if tool_name == "ExitPlanMode" => {
            Some((input, asking))
        }
        _ => None,
    });
    let (input, asking) = plan.unwrap();
    let text = input["plan"].as_str().unwrap();
    assert!(!text.is_empty());
    assert_eq!(asking.as_deref(), Some(&Asking::Plan { plan: text.to_string() }), "what it asks says the plan");
}

#[test]
fn a_question_comes_for_approval_with_its_options_and_takes_answers_back() {
    let events = events();
    let asked = events.iter().find_map(|e| match e {
        EventBody::ApprovalRequested { tool_name, input, asking, .. } if tool_name == "AskUserQuestion" => {
            Some((input, asking))
        }
        _ => None,
    });
    let (input, asking) = asked.unwrap();
    let question = &input["questions"][0];
    assert!(question["question"].as_str().is_some());
    assert!(question["options"][1]["label"].as_str().is_some());
    assert!(question["multiSelect"].is_boolean());
    let Some(Asking::Questions { questions }) = asking.as_deref() else { panic!("{asking:?}") };
    let read = &questions[0];
    assert_eq!(Some(read.text.as_str()), question["question"].as_str(), "what it asks says the question");
    assert_eq!(Some(read.options[1].label.as_str()), question["options"][1]["label"].as_str());
    assert_eq!(Some(read.multiple), question["multiSelect"].as_bool());
    let answered = events.iter().any(|e| match e {
        EventBody::AgentUser { blocks, .. } => blocks
            .iter()
            .any(|b| matches!(b, ContentBlock::ToolResult { text, .. } if text.contains("have been answered"))),
        _ => false,
    });
    assert!(answered, "the capture answers with `updatedInput.answers`, and the CLI should accept that");
}

/// Every turn says what its own requests took and which models its replies came from, each turn only its own. In plan
/// mode, the fourth turn, with Haiku chosen, they came from Sonnet first. Models go by family, so a capture with newer
/// ones still reads.
#[test]
fn turn_results_report_cost_duration_and_what_the_agent_asked_of_which_models() {
    let mut families: Vec<Vec<String>> = Vec::new();
    for end in events() {
        let EventBody::TurnEnded { duration_ms, cost_usd, num_turns, models, tokens, .. } = end else { continue };
        assert!(duration_ms.is_some() && cost_usd.is_some_and(|c| c >= 0.0) && num_turns.is_some_and(|t| t >= 1));
        assert!(tokens.is_some_and(|used| used.output_tokens > 0 && used.cache_read_tokens > 0));
        families.push(models.iter().map(|model| model.as_str().split('-').nth(1).unwrap().to_string()).collect());
    }
    let one = |family: &str| vec![family.to_string()];
    let planned = vec!["sonnet".to_string(), "haiku".to_string()];
    assert_eq!(families, [one("haiku"), one("haiku"), one("haiku"), planned, one("haiku")]);
}

#[test]
fn state_events_cover_running_waiting_and_idle() {
    let states: Vec<_> = events()
        .into_iter()
        .filter_map(|e| match e {
            EventBody::AgentState { state } => Some(state),
            _ => None,
        })
        .collect();
    for wanted in [AgentRunState::Running, AgentRunState::RequiresAction, AgentRunState::Idle] {
        assert!(states.contains(&wanted), "{wanted:?} in {states:?}");
    }
    assert_eq!(states.last(), Some(&AgentRunState::Idle));
    assert!(!states.contains(&AgentRunState::Unknown));
}

#[test]
fn thinking_tools_and_results_all_normalize() {
    let blocks = blocks();
    // Thinking is only readable because of --thinking-display. Without it the text is empty.
    assert!(blocks.iter().any(|b| matches!(b, ContentBlock::Thinking { text } if !text.is_empty())));
    assert!(blocks.iter().any(|b| matches!(b, ContentBlock::Text { text, .. } if text.contains("DONE"))));
    assert!(blocks.iter().any(|b| matches!(
        b,
        ContentBlock::ToolUse { name, input, id, .. } if name == "Bash" && input["command"].is_string() && id.as_str().starts_with("toolu_")
    )));
    assert!(blocks.iter().any(|b| matches!(b, ContentBlock::ToolResult { is_error: false, .. })));
    // A denial reaches the agent as a tool error carrying our message.
    assert!(blocks.iter().any(|b| matches!(
        b,
        ContentBlock::ToolResult { is_error: true, text, .. } if text.contains("Not allowed")
    )));
    assert!(!blocks.iter().any(|b| matches!(b, ContentBlock::Unknown)));
}

/// So a client reads each call by its kind, and needs none of the CLI's names for its tools.
#[test]
fn every_call_says_what_kind_it_is_and_every_result_the_kind_of_the_call_it_answers() {
    let mut kinds: HashMap<ToolUseId, CallKind> = HashMap::new();
    let mut answered = 0;
    for block in blocks() {
        match block {
            ContentBlock::ToolUse { id, name, call, .. } => {
                let kind = call.unwrap_or_else(|| panic!("{name} says no kind"));
                assert_ne!(kind, CallKind::Unknown, "{name}");
                assert_eq!(kind == CallKind::Command, name == "Bash", "{name}");
                kinds.insert(id, kind);
            }
            ContentBlock::ToolResult { tool_use_id, call, .. } => {
                let made = kinds.get(&tool_use_id).unwrap_or_else(|| panic!("{tool_use_id} answers no call"));
                assert_eq!(call.as_ref(), Some(made), "the result of {tool_use_id}");
                answered += 1;
            }
            ContentBlock::Text { .. }
            | ContentBlock::Thinking { .. }
            | ContentBlock::Image { .. }
            | ContentBlock::Unknown => {}
        }
    }
    assert!(kinds.values().any(|kind| *kind == CallKind::Command), "the fixture runs a command");
    assert!(kinds.values().any(|kind| *kind == CallKind::Plan), "and asks to leave plan mode");
    assert!(answered > 0, "and its calls are answered");
}

/// The fixture's plan is written to a new file, and the CLI says so beside the write's result.
#[test]
fn a_write_s_result_says_it_made_the_file() {
    let changes: Vec<FileChange> = events()
        .into_iter()
        .filter_map(|e| match e {
            EventBody::AgentUser { blocks, .. } => Some(blocks),
            _ => None,
        })
        .flatten()
        .filter_map(|b| match b {
            ContentBlock::ToolResult { change, .. } => change,
            _ => None,
        })
        .collect();
    assert_eq!(changes, [FileChange::Created]);
}

/// The streaming tail depends on a sequence the CLI could change: each block's deltas, then its
/// finished `assistant` event, then the next block. This replays the capture through the same
/// rules the hub and clients use.
#[test]
fn deltas_rebuild_exactly_what_each_finished_block_says() {
    let mut tail = Tail::default();
    let mut compared = (0, 0, 0);
    // Numbered as the hub would record them.
    for (seq, item) in (1..).zip(parsed()) {
        match item {
            Parsed::Delta(delta) => tail.apply(&delta),
            Parsed::Event(body) => {
                if let EventBody::Assistant { message_id: Some(id), blocks, first_index, .. } = &body {
                    let open = tail.messages.iter().find(|m| m.message_id == *id);
                    let open = open.unwrap_or_else(|| panic!("no streaming tail for {id}"));
                    assert_eq!(open.blocks.len(), 1, "one block at a time is open");
                    let streamed = &open.blocks[0];
                    // So the finished block keeps the identity it had while it streamed.
                    assert_eq!(*first_index, Some(streamed.index), "the block's place in its message");
                    match &blocks[0] {
                        ContentBlock::Text { text, .. } => {
                            assert_eq!(streamed.kind, BlockKind::Text);
                            assert_eq!(streamed.text, *text);
                            compared.0 += 1;
                        }
                        ContentBlock::Thinking { text } => {
                            assert_eq!(streamed.kind, BlockKind::Thinking);
                            assert_eq!(streamed.text, *text);
                            compared.1 += 1;
                        }
                        ContentBlock::ToolUse { name, input, .. } => {
                            assert_eq!(streamed.kind, BlockKind::ToolUse);
                            assert_eq!(streamed.tool_name.as_deref(), Some(name.as_str()));
                            // The CLI fills this one in itself from the plan file, so nothing streams.
                            if name == "ExitPlanMode" {
                                assert_eq!(streamed.text, "");
                            } else {
                                let streamed_input: Value = serde_json::from_str(&streamed.text).unwrap();
                                assert_eq!(&streamed_input, input);
                                compared.2 += 1;
                            }
                        }
                        other => panic!("unexpected block {other:?}"),
                    }
                }
                tail.on_event(seq, None, &body);
            }
            _ => {}
        }
    }
    assert!(compared.0 >= 2 && compared.1 >= 2 && compared.2 >= 3, "text, thinking and tool blocks: {compared:?}");
    assert!(tail.is_empty(), "nothing should stay open at the end: {tail:?}");
}

/// What each of the agent's own requests said of the cache, in order.
fn requests() -> Vec<CacheRequest> {
    parsed()
        .into_iter()
        .filter_map(|p| match p {
            Parsed::Request(request) => Some(request),
            _ => None,
        })
        .collect()
}

#[test]
fn each_of_the_agents_requests_says_how_it_met_the_cache_and_how_long_before_the_line_it_was_sent() {
    let requests = requests();

    assert!(requests.len() > 2, "{requests:?}");
    for request in &requests {
        assert_eq!(request.ttl, Some(CacheTtl::OneHour), "the agent's own write at 1h: {request:?}");
        assert!(request.sent_ms_ago > 0 && request.read_tokens > 0, "{request:?}");
        assert!(request.model.as_ref().is_some_and(|model| model.as_str().starts_with("claude-")), "{request:?}");
        assert!(!request.first_for_prompt, "which only the process knows: {request:?}");
    }
}

/// What the hub tells a cache that had expired by, in `crates/hub/src/session/cache.rs`: a warm one is read as far as
/// the request before to the same model left it, however much was written since, by requests to another model too.
#[test]
fn a_warm_cache_is_read_as_far_as_the_request_before_to_the_same_model_left_it() {
    let requests = requests();
    let mut followed = 0;
    for (i, request) in requests.iter().enumerate() {
        if let Some(before) = requests[..i].iter().rfind(|before| before.model == request.model) {
            assert!(request.read_tokens >= before.read_tokens + before.written_tokens, "{before:?} then {request:?}");
            followed += 1;
        }
    }
    let models: std::collections::HashSet<_> = requests.iter().map(|request| &request.model).collect();
    assert!(models.len() > 1 && followed > models.len(), "two models, each asked more than once: {requests:?}");
}
