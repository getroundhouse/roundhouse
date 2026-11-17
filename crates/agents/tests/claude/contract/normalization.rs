#![cfg(test)]

use agents::claude::{Normalizer, Parsed};
use protocol::capability::approvals::Asks;
use protocol::capability::calls::{Call, CallKind, CallOutput};
use protocol::event::{AgentRunState, ContentBlock, EventBody, FileChange, TurnOutcome};
use protocol::model::{ApprovalId, CacheRequest, CacheTtl, PermissionMode, Seq, ToolUseId};
use protocol::tail::{BlockKind, Tail};
use serde_json::Value;
use std::collections::HashMap;

const FIXTURE: &str = include_str!("../../fixtures/claude.jsonl");

fn parsed() -> Vec<Parsed> {
    let mut normalizer = Normalizer::default();
    FIXTURE.lines().map(|line| normalizer.line(line).parsed).collect()
}

pub(in super::super) fn events() -> Vec<EventBody> {
    parsed()
        .into_iter()
        .filter_map(|p| match p {
            Parsed::Event(body) | Parsed::EventWithImages { body, .. } => Some(body),
            _ => None,
        })
        .collect()
}

/// Every block of what the agent wrote and what its tools sent back, in order.
pub(in super::super) fn blocks() -> Vec<ContentBlock> {
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
    let EventBody::AgentInit { agent_session_id, model, cwd, permission_mode, agent_version, .. } = init else {
        unreachable!()
    };
    assert!(agent_session_id.is_some_and(|id| !id.as_str().is_empty()));
    assert!(model.is_some_and(|m| m.contains("haiku")));
    assert_eq!(cwd.as_deref(), Some("/tmp/fixture"));
    // Captured launched with `--permission-mode manual`, which the CLI reports as `default`, a mode a chat can't have.
    assert_eq!(permission_mode, Some(PermissionMode::Unknown));
    assert!(agent_version.is_some_and(|v| !v.is_empty()));
}

/// What the CLI asked, as the capture has it, by each request's ID.
fn asked() -> HashMap<String, Value> {
    let lines = FIXTURE.lines().filter_map(|line| serde_json::from_str::<Value>(line).ok());
    let requests =
        lines.filter(|line| line["type"] == "control_request" && line["request"]["subtype"] == "can_use_tool");
    requests.map(|line| (line["request_id"].as_str().unwrap().to_string(), line["request"].clone())).collect()
}

#[test]
fn approvals_carry_what_a_client_must_show_and_the_rules_the_cli_suggests() {
    let asked = asked();
    let approvals: Vec<_> = events()
        .into_iter()
        .filter_map(|e| match e {
            EventBody::ApprovalRequested { approval_id, tool_use_id, request, .. } => {
                Some((asked[approval_id.as_str()].clone(), tool_use_id, request))
            }
            _ => None,
        })
        .collect();
    let bash: Vec<_> = approvals.iter().filter(|(asked, ..)| asked["tool_name"] == "Bash").collect();
    assert_eq!(bash.len(), 3);
    for (asked, tool_use_id, request) in bash {
        assert!(tool_use_id.as_ref().is_some_and(|t| t.as_str().starts_with("toolu_")));
        let Asks::Call { call: Call::Command(command) } = &request.asks else {
            panic!("{request:?} isn't the command it makes");
        };
        assert_eq!(command.command, asked["input"]["command"].as_str().unwrap());
        assert_eq!(command.description.as_deref(), asked["description"].as_str(), "what it's for");
        let [rule] = &request.suggested[..] else { panic!("{:?}", request.suggested) };
        let suggested = &asked["permission_suggestions"][0]["rules"][0];
        let content = suggested["ruleContent"].as_str().unwrap();
        assert_eq!(rule.matching, format!("Bash({content})"), "the rule it suggests, but not the directory");
        assert!(!request.always);
    }
}

#[test]
fn interrupting_during_an_approval_cancels_it_and_ends_the_turn_as_cancelled() {
    let events = events();
    let asked: Vec<&ApprovalId> = events
        .iter()
        .filter_map(|e| match e {
            EventBody::ApprovalRequested { approval_id, request, .. } => {
                (!matches!(request.asks, Asks::Questions { .. })).then_some(approval_id)
            }
            _ => None,
        })
        .collect();
    let cancelled: Vec<&ApprovalId> = events
        .iter()
        .filter_map(|e| match e {
            EventBody::ApprovalResolved { approval_id, decision: None, .. } => Some(approval_id),
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
    let asked = asked();
    let plan = events().into_iter().find_map(|e| match e {
        EventBody::ApprovalRequested { approval_id, request, .. } if matches!(request.asks, Asks::Plan { .. }) => {
            Some((asked[approval_id.as_str()]["input"].clone(), request.asks))
        }
        _ => None,
    });
    let (input, asks) = plan.unwrap();
    let text = input["plan"].as_str().unwrap();
    assert!(!text.is_empty());
    let file = input["planFilePath"].as_str().map(str::to_string);
    assert_eq!(asks, Asks::Plan { plan: text.to_string(), file }, "what it asks says the plan, and its file");
}

#[test]
fn a_question_comes_for_approval_with_its_options_and_takes_answers_back() {
    let events = events();
    let requests = asked();
    let asked = events.iter().find_map(|e| match e {
        EventBody::ApprovalRequested { approval_id, request, .. } if matches!(request.asks, Asks::Questions { .. }) => {
            Some((requests[approval_id.as_str()]["input"].clone(), &request.asks))
        }
        _ => None,
    });
    let (input, asks) = asked.unwrap();
    let question = &input["questions"][0];
    assert!(question["question"].as_str().is_some());
    assert!(question["options"][1]["label"].as_str().is_some());
    assert!(question["multiSelect"].is_boolean());
    let Asks::Questions { questions } = asks else { panic!("{asks:?}") };
    let read = &questions[0];
    assert_eq!(Some(read.text.as_str()), question["question"].as_str(), "what it asks says the question");
    assert_eq!(Some(read.options[1].label.as_str()), question["options"][1]["label"].as_str());
    assert_eq!(Some(read.several()), question["multiSelect"].as_bool());
    let answered = events.iter().any(|e| match e {
        EventBody::AgentUser { blocks, .. } => blocks
            .iter()
            .any(|b| matches!(b, ContentBlock::CallResult { output: CallOutput::Words { text }, .. } if text.contains("have been answered"))),
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
        ContentBlock::Call { id, call: Call::Command(command) } if !command.command.is_empty() && id.as_str().starts_with("toolu_")
    )));
    assert!(blocks.iter().any(|b| matches!(b, ContentBlock::CallResult { is_error: false, .. })));
    // A denial reaches the agent as a tool error carrying our message.
    assert!(blocks.iter().any(|b| matches!(
        b,
        ContentBlock::CallResult { is_error: true, output: CallOutput::Command(printed), .. } if printed.output.contains("Not allowed")
    )));
    assert!(!blocks.iter().any(|b| matches!(b, ContentBlock::Unknown)));
}

/// So a client reads each call by what it is, and needs none of the CLI's names for its tools or the shape of their
/// inputs, and reads each result against the call it answers.
#[test]
fn every_call_is_typed_and_every_result_answers_one() {
    let mut kinds: HashMap<ToolUseId, CallKind> = HashMap::new();
    let mut answered = 0;
    for block in blocks() {
        match block {
            ContentBlock::Call { id, call } => {
                assert_ne!(call.kind(), CallKind::Unknown, "{call:?}");
                kinds.insert(id, call.kind());
            }
            ContentBlock::CallResult { tool_use_id, .. } => {
                assert!(kinds.contains_key(&tool_use_id), "{tool_use_id} answers no call");
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
            ContentBlock::CallResult { output: CallOutput::Changed { files }, .. } => Some(files),
            _ => None,
        })
        .flatten()
        .filter_map(|file| file.change)
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
                        ContentBlock::Call { call, .. } => {
                            assert_eq!(streamed.kind, BlockKind::ToolUse);
                            // The CLI fills this one in itself from the plan file, so nothing streams.
                            if *call == Call::Plan {
                                assert_eq!(streamed.text, "");
                            } else {
                                let streamed_input: Value = serde_json::from_str(&streamed.text).unwrap();
                                assert!(made_from(call, &streamed_input), "{call:?} from {streamed_input}");
                                compared.2 += 1;
                            }
                        }
                        other => panic!("unexpected block {other:?}"),
                    }
                }
                tail.on_event(Seq::new(seq), None, &body);
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

/// What the hub tells a cache that had expired by, in `crates/hub/src/session/cache/mod.rs`: a warm one is read as far as
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

/// Whether `call` is the call `input` makes, as far as its kind says what it was given.
fn made_from(call: &Call, input: &Value) -> bool {
    match call {
        Call::Command(command) => input["command"] == command.command.as_str(),
        Call::Write(write) => input["file_path"] == write.path.as_str() && input["content"] == write.content.as_str(),
        Call::Read(read) => input["file_path"] == read.path.as_str(),
        Call::Subagent(start) => input["prompt"] == start.prompt.as_str(),
        Call::Other { input: given, .. } | Call::Mcp { input: given, .. } => given == input,
        Call::Questions => input["questions"].is_array(),
        other => panic!("the capture makes no {other:?}"),
    }
}
