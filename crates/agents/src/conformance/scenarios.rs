//! The scenarios, in Roundhouse's terms. Each agent's binding gives each a script for its scripted model, from the
//! words here that the script answers to.

use std::time::Duration;

use protocol::event::{Asking, CallKind, ContentBlock, EventBody, TurnOutcome};
use protocol::model::PermissionMode;

use super::driver::INTERRUPTED;
use super::scenario::{BOTH, Check, SCRIPTED_ONLY, Scenario};
use super::trial::Trial;

/// Every scenario, in the order they're played.
pub static ALL: &[&Scenario] = &[&PLAIN_TURN, &CANCEL_COMMAND, &RESUME, &THINKING, &PLAN_APPROVED];

/// A prompt, one command, what it printed, and a reply.
pub static PLAIN_TURN: Scenario = Scenario {
    name: "plain_turn",
    against: BOTH,
    mode: PermissionMode::FullAccess,
    steps: |driver| {
        Box::pin(async move {
            driver.start().await?;
            driver
                .prompt(&format!("Run exactly `{ECHO}` in a shell, then say what it printed. Do nothing else."))
                .await?;
            driver.turn_ended().await
        })
    },
    checks: &[
        A_COMMAND_RAN,
        Check {
            name: "what it printed came back",
            check: |trial| {
                let printed = trial.results().iter().any(|text| text.contains(PRINTED));
                holds(printed, || format!("no tool result says {PRINTED}, of {:?}", trial.results()))
            },
        },
        THE_TURN_SUCCEEDED,
    ],
};

/// What [`PLAIN_TURN`]'s command is, whose output isn't in its text, so a result that only quotes the command doesn't
/// pass for what it printed.
pub const ECHO: &str = "echo hello-from-$((6*7))";
pub const PRINTED: &str = "hello-from-42";

/// An interrupt while a command runs, then the next prompt, to the same process.
pub static CANCEL_COMMAND: Scenario = Scenario {
    name: "cancel_command",
    against: BOTH,
    mode: PermissionMode::FullAccess,
    steps: |driver| {
        Box::pin(async move {
            driver.start().await?;
            driver.prompt(&format!("Run exactly `{SLOW}` in a shell. Do nothing else.")).await?;
            driver.made(STARTED).await?;
            driver.interrupt().await?;
            driver.turn_ended().await?;
            driver.mark(CANCELLED);
            driver.prompt(READY).await?;
            driver.turn_ended().await
        })
    },
    checks: &[
        A_COMMAND_RAN,
        Check {
            name: "the turn it ran in was cancelled, and the next succeeded",
            check: |trial| turns(trial, &[TurnOutcome::Cancelled, TurnOutcome::Success]),
        },
        Check {
            name: "the turn ended within 10 s of the interrupt",
            check: |trial| {
                let took = trial.between(INTERRUPTED, CANCELLED);
                holds(took.is_some_and(|took| took < Duration::from_secs(10)), || format!("it took {took:?}"))
            },
        },
    ],
};

/// What [`CANCEL_COMMAND`]'s command makes as it starts, so the driver knows it's running.
pub const STARTED: &str = "started";
/// A command that runs for longer than the scenario waits. It isn't a plain `sleep`, which a CLI can refuse, as
/// Claude's does, telling the model to wait with a tool of its own instead.
pub const SLOW: &str = "touch started && perl -e 'sleep 30'";
const CANCELLED: &str = "cancelled";
/// A prompt the scripted model answers with `ready`, and the real one is asked to.
pub const READY: &str = "Reply with the single word: ready";

/// A conversation carried from one process to the next.
pub static RESUME: Scenario = Scenario {
    name: "resume",
    against: BOTH,
    mode: PermissionMode::FullAccess,
    steps: |driver| {
        Box::pin(async move {
            driver.start().await?;
            driver.prompt(&format!("Remember this word for later: {WORD}. Reply only with: noted")).await?;
            driver.turn_ended().await?;
            driver.stop().await?;
            driver.resume().await?;
            driver.prompt(ASKED).await?;
            driver.turn_ended().await
        })
    },
    checks: &[
        Check {
            name: "both processes had the same conversation",
            check: |trial| {
                let conversations = trial.conversations();
                let same = conversations.len() >= 2
                    && conversations[0].is_some()
                    && conversations.windows(2).all(|pair| pair[0] == pair[1]);
                holds(same, || format!("their conversations were {conversations:?}"))
            },
        },
        Check {
            name: "the second remembered what the first was told",
            check: |trial| {
                let said = trial.results_of_turns().last().copied().flatten().unwrap_or_default();
                holds(said.to_lowercase().contains(WORD), || format!("it said {said:?}"))
            },
        },
        Check {
            name: "both turns succeeded",
            check: |trial| turns(trial, &[TurnOutcome::Success, TurnOutcome::Success]),
        },
    ],
};

pub const WORD: &str = "pineapple";
pub const ASKED: &str = "What word did I ask you to remember? Reply with just the word.";

/// What the model thought, recorded with its reply. Only the scripted model can be told to think.
pub static THINKING: Scenario = Scenario {
    name: "thinking",
    against: SCRIPTED_ONLY,
    mode: PermissionMode::FullAccess,
    steps: |driver| {
        Box::pin(async move {
            driver.start().await?;
            driver.prompt("Think, then answer.").await?;
            driver.turn_ended().await
        })
    },
    checks: &[
        Check {
            name: "the thinking was recorded",
            check: |trial| {
                let thought =
                    trial.blocks().any(|block| matches!(block, ContentBlock::Thinking { text } if text == THOUGHT));
                holds(thought, || "no thinking block says what the model thought".into())
            },
        },
        THE_TURN_SUCCEEDED,
    ],
};

pub const THOUGHT: &str = "Weighing it up.";

/// A chat in plan mode, whose agent presents a plan, which the driver approves, so the agent leaves plan mode.
pub static PLAN_APPROVED: Scenario = Scenario {
    name: "plan_approved",
    against: BOTH,
    mode: PermissionMode::Plan,
    steps: |driver| {
        Box::pin(async move {
            driver.start().await?;
            driver
                .prompt(&format!(
                    "Write exactly this plan to your plan file, then present it for approval, without reading or \
                     running anything else: {PLAN} Once it's approved, reply with the single word: approved"
                ))
                .await?;
            driver.turn_ended().await
        })
    },
    checks: &[
        Check {
            name: "the agent asked for the plan to be approved",
            check: |trial| {
                let asked = trial.events().any(|event| {
                    matches!(event, EventBody::ApprovalRequested { asking: Some(asking), .. }
                        if matches!(&**asking, Asking::Plan { plan } if plan.contains(PLAN)))
                });
                holds(asked, || "no approval asked of the plan".into())
            },
        },
        Check {
            name: "it left plan mode once approved",
            check: |trial| {
                let modes: Vec<_> = trial
                    .events()
                    .filter_map(|event| {
                        if let EventBody::AgentMode { permission_mode } = event { Some(*permission_mode) } else { None }
                    })
                    .collect();
                holds(modes.last() == Some(&PermissionMode::FullAccess), || format!("its modes were {modes:?}"))
            },
        },
        THE_TURN_SUCCEEDED,
    ],
};

pub const PLAN: &str = "Add the line `Planned.` to README.md.";

const THE_TURN_SUCCEEDED: Check =
    Check { name: "the turn succeeded", check: |trial| turns(trial, &[TurnOutcome::Success]) };

const A_COMMAND_RAN: Check = Check {
    name: "a command ran",
    check: |trial| holds(!trial.calls(&CallKind::Command).is_empty(), || "no command call".into()),
};

fn holds(condition: bool, otherwise: impl FnOnce() -> String) -> Result<(), String> {
    if condition { Ok(()) } else { Err(otherwise()) }
}

/// Whether the trial's turns ended with `outcomes`, in order.
fn turns(trial: &Trial, outcomes: &[TurnOutcome]) -> Result<(), String> {
    let ended = trial.outcomes();
    holds(ended == outcomes, || format!("the turns ended {ended:?}"))
}
