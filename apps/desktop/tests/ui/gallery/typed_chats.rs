//! Chats whose calls are recorded typed, as an adapter that types its calls records them, for the screens of how each
//! kind is drawn from it. Made up, about the same signalling crate as the chats of Claude's calls, so they read alike.

#![cfg(test)]

use protocol::capability::calls::Call;
use protocol::capability::calls::call::{Command, Runs};
use protocol::capability::calls::output::Ended;
use protocol::event::EventBody;
use protocol::fixtures::{command_output, typed_command};
use protocol::model::Seq;

use super::events::{replied, taken, text, typed_call, typed_result, user_prompt};
use super::tool_chats::ASK;

/// What the agent said its command in the background is for.
pub(crate) const BACKGROUND_LINE: &str = "Start the depot simulator";

/// Commands that ended without exiting: one that went on in the background, one that ran past its time, and one that
/// was stopped.
pub(crate) fn commands_that_ended() -> Vec<EventBody> {
    let reply = "The slow tests ran past their time, so I stopped the benchmark too. The simulator is still running.";
    let backgrounded = Ended::Backgrounded { task: "simulator".into() };
    vec![
        user_prompt(ASK),
        taken(Seq::new(1)),
        text("msg-1", "I'll start the depot's simulator, then run the slow signal tests against it."),
        typed_call("msg-2", "call-1", command("cargo run -p depot --bin simulator", BACKGROUND_LINE, Runs::Background)),
        typed_result("call-1", command_output(SIMULATOR, None, backgrounded), false),
        typed_call(
            "msg-3",
            "call-2",
            command("cargo test -p signals --release -- --ignored", "Run the slow signal tests", Runs::Waits),
        ),
        typed_result("call-2", command_output(SLOW_TESTS, None, Ended::TimedOut), true),
        typed_call("msg-4", "call-3", command("cargo bench -p signals may_pass", "Benchmark may_pass", Runs::Waits)),
        typed_result("call-3", command_output(BENCHMARK, None, Ended::Stopped), true),
        text("msg-5", reply),
        replied(reply, 214, 0.38),
    ]
}

/// What the simulator printed before it went on in the background.
const SIMULATOR: &str = "    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.38s
     Running `target/debug/simulator`
depot simulator listening on 127.0.0.1:7341";

/// What the slow tests printed before they ran past their time.
const SLOW_TESTS: &str = "    Finished `release` profile [optimized] target(s) in 41.20s
     Running unittests src/lib.rs (target/release/deps/signals-91c3e0d2)

running 14 tests
test depot::caution_holds_for_the_whole_block ... ok";

/// What the benchmark printed before it was stopped.
const BENCHMARK: &str = "Benchmarking may_pass: Warming up for 3.0000 s";

/// Running `command`, which the agent said is for `description`.
fn command(command: &str, description: &str, runs: Runs) -> Call {
    Call::Command(Command { description: Some(description.into()), runs, ..typed_command(command) })
}
