use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

use super::{AgentTask, EndedTask, EventBody, TaskKind, TaskOutcome};
use crate::capability::turns::OwnTurn;
use crate::model::{EndedWait, WaitEnd, WaitEnded, WaitUntil, Watching};

fn wait() -> impl Strategy<Value = EndedWait> {
    let how = prop::sample::select(vec![WaitEnd::ChecksPassed, WaitEnd::ChecksFailed, WaitEnd::CalledOffByPerson]);
    let until = prop::sample::select(vec![WaitUntil::AllChecks, WaitUntil::RequiredChecks, WaitUntil::Merged]);
    let numbers = prop::collection::vec(1..2000_u64, 1..4);
    (numbers, until, any::<bool>(), prop::option::of((0..10_i64, how))).prop_map(
        |(numbers, until, on_the_branch, ended)| EndedWait {
            id: "w".into(),
            watching: if on_the_branch {
                Watching::Branch { number: numbers[0], until }
            } else {
                Watching::PullRequests { numbers, until }
            },
            ended: ended.map(|(at_ms, how)| WaitEnded { at_ms, how }),
        },
    )
}

fn ended_task() -> impl Strategy<Value = EndedTask> {
    let kind = prop::sample::select(vec![TaskKind::Subagent, TaskKind::Monitor, TaskKind::Wait, TaskKind::Command]);
    let outcome = prop::sample::select(vec![TaskOutcome::Finished, TaskOutcome::Failed, TaskOutcome::Stopped]);
    let words = prop::option::of("[a-z ]{0,12}");
    (kind, words.clone(), prop::option::of("toolu_[0-9]{1,3}"), outcome, prop::option::of(wait())).prop_map(
        |(kind, description, call, outcome, wait)| EndedTask {
            task: AgentTask { id: "t".into(), description, kind, call: call.map(Into::into) },
            outcome,
            wait,
        },
    )
}

proptest! {
    #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

    /// A turn the agent started by itself reads back as itself, whatever it heard had ended.
    #[test]
    fn a_turn_the_agent_started_by_itself_reads_back_as_itself(ended in prop::collection::vec(ended_task(), 0..4)) {
        let own = EventBody::OwnTurn(OwnTurn::TaskEnded { ended });

        let json = serde_json::to_string(&own).unwrap();

        prop_assert_eq!(serde_json::from_str::<EventBody>(&json).unwrap(), own);
    }
}
