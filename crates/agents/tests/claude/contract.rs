//! Checks the adapter against real `claude` output. `../fixtures/claude.jsonl` was captured from the
//! CLI with the adapter's own flags, by `tools/smoke/capture_claude_fixture.py`. Rerun that after a
//! CLI upgrade: a failure here means the CLI changed in a way the adapter relies on.

#![cfg(test)]

use agents::claude::{Normalizer, Parsed};
use protocol::event::{AgentRunState, ApprovalOutcome, ContentBlock, EventBody, FileChange, TurnOutcome};
use protocol::model::{ApprovalId, PermissionMode};
use protocol::tail::{BlockKind, Tail};
use serde_json::Value;

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
            EventBody::ApprovalRequested { approval_id, tool_name, tool_use_id, input, description, suggestions } => {
                Some((approval_id, tool_name, tool_use_id, input, description, suggestions))
            }
            _ => None,
        })
        .collect();
    let bash: Vec<_> = approvals.iter().filter(|(_, tool, ..)| tool == "Bash").collect();
    assert_eq!(bash.len(), 3);
    for (id, _, tool_use_id, input, description, suggestions) in bash {
        assert!(!id.as_str().is_empty());
        assert!(tool_use_id.as_ref().is_some_and(|t| t.as_str().starts_with("toolu_")));
        assert!(input["command"].as_str().is_some_and(|c| c.contains("echo")));
        assert!(description.is_some());
        assert!(suggestions.is_array());
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
        EventBody::ApprovalRequested { tool_name, input, .. } if tool_name == "ExitPlanMode" => Some(input),
        _ => None,
    });
    assert!(plan.unwrap()["plan"].as_str().is_some_and(|text| !text.is_empty()));
}

#[test]
fn a_question_comes_for_approval_with_its_options_and_takes_answers_back() {
    let events = events();
    let asked = events.iter().find_map(|e| match e {
        EventBody::ApprovalRequested { tool_name, input, .. } if tool_name == "AskUserQuestion" => Some(input),
        _ => None,
    });
    let question = &asked.unwrap()["questions"][0];
    assert!(question["question"].as_str().is_some());
    assert!(question["options"][1]["label"].as_str().is_some());
    assert!(question["multiSelect"].is_boolean());
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
    let events = events();
    let blocks: Vec<&ContentBlock> = events
        .iter()
        .flat_map(|e| match e {
            EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. } => {
                blocks.iter().collect::<Vec<_>>()
            }
            _ => vec![],
        })
        .collect();
    // Thinking is only readable because of --thinking-display. Without it the text is empty.
    assert!(blocks.iter().any(|b| matches!(b, ContentBlock::Thinking { text } if !text.is_empty())));
    assert!(blocks.iter().any(|b| matches!(b, ContentBlock::Text { text, .. } if text.contains("DONE"))));
    assert!(blocks.iter().any(|b| matches!(
        b,
        ContentBlock::ToolUse { name, input, id } if name == "Bash" && input["command"].is_string() && id.starts_with("toolu_")
    )));
    assert!(blocks.iter().any(|b| matches!(b, ContentBlock::ToolResult { is_error: false, .. })));
    // A denial reaches the agent as a tool error carrying our message.
    assert!(blocks.iter().any(|b| matches!(
        b,
        ContentBlock::ToolResult { is_error: true, text, .. } if text.contains("Not allowed")
    )));
    assert!(!blocks.iter().any(|b| matches!(b, ContentBlock::Unknown)));
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
