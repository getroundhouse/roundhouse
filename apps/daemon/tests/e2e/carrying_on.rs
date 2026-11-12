//! A hub that goes and leaves its workers running, as one restarting for an update does and one that crashes can, and
//! the next hub carrying on with their agents.

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use client_core::{Client, Restart};
use protocol::event::{ApprovalDecision, EventBody, SessionEvent};
use protocol::model::{BranchName, Head, SetupStatus};
use protocol::worker::{WorkerOutcome, WorkerRequest, WorkerResponse};

use crate::attaching::Attached;

use crate::support::{
    self, Stack, assert_gapless, assistant_text, commit_settings, is_alive, pending_approval, read_all, started_pid,
    wait_for_approval, wait_for_setup, wait_for_turn_end, wait_until,
};

/// As an update has the hub do: asks it to restart, and waits for it to go. A client starts the next one.
pub(crate) fn restart(stack: &mut Stack) {
    restart_then(stack, Restart::Exited);
}

/// Asks the hub to restart, as `restart` does, and waits for it to go, once it has said how that went.
fn restart_then(stack: &mut Stack, said: Restart) {
    assert_eq!(client_core::restart_hub(&stack.client()).unwrap(), said);
    let mut hub = stack.hub.take().unwrap();
    wait_until("the hub to exit", || !matches!(hub.try_wait(), Ok(None)));
}

fn count(events: &[SessionEvent], wanted: impl Fn(&EventBody) -> bool) -> usize {
    events.iter().filter(|e| wanted(&e.body)).count()
}

/// What an agent says with no hub there is kept by its worker, and recorded once by the next hub, along with what it
/// said before, which the worker may have kept too, if the hub went before saying it had recorded it.
fn assert_one_agent_said_it_all_once(events: &[SessionEvent], said: &str) {
    assert_gapless(events);
    assert_eq!(assistant_text(events), said, "{events:#?}");
    assert_eq!(count(events, |b| matches!(b, EventBody::AgentStarted { .. })), 1, "{events:#?}");
    assert_eq!(count(events, |b| matches!(b, EventBody::AgentExited { .. })), 0, "{events:#?}");
    assert_eq!(count(events, |b| matches!(b, EventBody::TurnEnded { .. })), 1, "{events:#?}");
}

#[test]
fn a_restart_leaves_the_agent_at_work_and_the_next_hub_carries_on_with_it() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:before; hang:1500; say:after", None).unwrap();
    wait_until("the agent to start its tool", || assistant_text(&read_all(&client, &session.id)) == "before");
    let agent = started_pid(&read_all(&client, &session.id)).unwrap();
    assert!(client.hub_status().unwrap().keeps_workers, "so the app knows it can restart this hub at once");
    drop(client);

    restart(&mut stack);
    assert!(is_alive(agent), "the restart left the agent running");
    // So the agent finishes its turn with no hub there.
    std::thread::sleep(Duration::from_secs(2));
    stack.start_hub();

    let events = wait_for_turn_end(&stack.client(), &session.id, 1);
    assert_one_agent_said_it_all_once(&events, "beforeafter");
}

#[test]
fn a_hub_crash_leaves_the_agent_running_and_the_next_hub_carries_on_with_it() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:before; hang:1500; say:after", None).unwrap();
    // The hub may go before saying it has recorded this, so the next hub may hear it again.
    wait_until("the agent to start its tool", || assistant_text(&read_all(&client, &session.id)) == "before");
    let agent = started_pid(&read_all(&client, &session.id)).unwrap();
    drop(client);

    stack.kill_hub();
    assert!(is_alive(agent), "the crash left the agent running");
    stack.start_hub();

    let client = stack.client();
    let events = wait_for_turn_end(&client, &session.id, 1);
    assert_one_agent_said_it_all_once(&events, "beforeafter");
    // The same agent takes the next prompt.
    client.session_prompt(&session.id, "say:next", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);
    assert_eq!(count(&events, |b| matches!(b, EventBody::AgentStarted { .. })), 1, "{events:#?}");
    assert!(assistant_text(&events).ends_with("next"));
    assert_eq!(client.hub_status().unwrap().workers, 1, "the worker the last hub left");
}

/// The approval an agent asked for is still open, and answering it reaches the agent that asked.
#[test]
fn an_approval_open_as_the_hub_went_can_be_answered_by_the_next_hub() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace_planning(&client);
    client.session_prompt(&session.id, "tool:echo x > x.txt", None).unwrap();
    let approval = wait_for_approval(&client, &session.id);
    drop(client);

    stack.kill_hub();
    stack.start_hub();

    let client = stack.client();
    assert_eq!(pending_approval(&read_all(&client, &session.id)), Some(approval.clone()));
    let allow = ApprovalDecision::Allow { updated_input: None, updated_permissions: None };
    client.session_approve(&session.id, &approval, allow).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 1);
    assert_gapless(&events);
    assert_eq!(count(&events, |b| matches!(b, EventBody::AgentStarted { .. })), 1, "{events:#?}");
    assert!(PathBuf::from(workspace.path.unwrap()).join("x.txt").exists(), "the agent ran what was approved");
}

/// An idle agent is still running too, and the next hub stops it as it stops, as it would one it started.
#[test]
fn an_idle_agent_is_carried_on_with_and_stops_with_the_next_hub() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:first", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let agent = started_pid(&read_all(&client, &session.id)).unwrap();
    drop(client);

    restart(&mut stack);
    stack.start_hub();
    let client = stack.client();
    client.session_prompt(&session.id, "say:second", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);
    assert_eq!(count(&events, |b| matches!(b, EventBody::AgentStarted { .. })), 1, "{events:#?}");
    assert_eq!(assistant_text(&events), "firstsecond");

    drop(client);
    stack.stop_hub();
    wait_until("the agent to stop with the hub", || !is_alive(agent));
}

/// The worker the next hub carries on with watches the worktree's branches for it, so the next hub hears of a branch
/// checked out while there was no hub, which the worker couldn't send anyone.
#[test]
fn the_next_hub_hears_of_a_branch_checked_out_while_there_was_none() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:first", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let head = |client: &Client| support::workspace(client, &workspace.id).branches.map(|branches| branches.head);
    let started_on = Head::Branch { name: workspace.branch.clone() };
    wait_until("the branches to be known", || head(&client) == Some(started_on.clone()));
    drop(client);

    stack.kill_hub();
    support::git(Path::new(&workspace.path.unwrap()), &["switch", "-q", "-c", "next-thing"]);
    // Longer than the watch waits for git to stop writing, so it has read the branch before the next hub connects.
    std::thread::sleep(Duration::from_secs(1));
    stack.start_hub();

    let client = stack.client();
    let next_thing = Some(Head::Branch { name: BranchName::from("next-thing") });
    wait_until("the next hub to hear of the branch", || head(&client) == next_thing);
}

/// So a restart can't interrupt a setup that the next hub would only find failed.
#[test]
fn a_restart_waits_for_a_setup_script_to_finish() {
    let mut stack = Stack::start();
    commit_settings(&stack, "[scripts]\nsetup = \"sleep 1; touch ready.txt\"\n");
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    wait_until("the setup to start", || client.hub_status().unwrap().running_setups == 1);
    drop(client);

    // Asking says it's still finishing, rather than waiting for the setup.
    restart_then(&mut stack, Restart::Finishing);
    assert!(PathBuf::from(workspace.path.clone().unwrap()).join("ready.txt").exists(), "it went once setup was done");
    stack.start_hub();
    assert_eq!(wait_for_setup(&stack.client(), &workspace.id), SetupStatus::Succeeded);
}

/// `roundhouse hub stop` says it stops the agents, so it does, rather than leave them running with no hub.
#[test]
fn a_stop_while_a_restart_waits_for_a_setup_script_stops_the_agents() {
    let mut stack = Stack::start();
    let agent = an_agent_at_work_while_a_restart_waits(&stack);

    client_core::stop_hub(&stack.socket()).unwrap();

    stack.hub.take().unwrap().wait().unwrap();
    wait_until("the agent to stop with the hub", || !is_alive(agent));
}

/// Whoever sends a signal, such as systemd, can't say whether another hub is coming.
#[test]
fn a_signal_while_a_restart_waits_for_a_setup_script_stops_the_agents() {
    let mut stack = Stack::start();
    let agent = an_agent_at_work_while_a_restart_waits(&stack);

    stack.stop_hub();

    wait_until("the agent to stop with the hub", || !is_alive(agent));
}

/// Asks the hub to restart with an agent at work in one workspace and a setup script running in another, so it
/// waits, and returns the agent's PID.
fn an_agent_at_work_while_a_restart_waits(stack: &Stack) -> u32 {
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:before; hang:60000", None).unwrap();
    wait_until("the agent to start its tool", || assistant_text(&read_all(&client, &session.id)) == "before");
    let agent = started_pid(&read_all(&client, &session.id)).unwrap();
    commit_settings(stack, "[scripts]\nsetup = \"sleep 30\"\n");
    client.workspace_create(&workspace.repo_id, None, None, None).unwrap();
    wait_until("the setup to start", || client.hub_status().unwrap().running_setups == 1);
    assert_eq!(client_core::restart_hub(&client).unwrap(), Restart::Finishing);
    std::thread::sleep(Duration::from_millis(200));
    assert!(client.hub_status().is_ok(), "the restart waits for the setup script");
    agent
}

/// So a worker doesn't keep for good what its hub has recorded, and send it all again to the next hub.
#[test]
fn a_worker_lets_go_of_what_its_hub_said_it_recorded() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:first", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    // Longer than the hub gathers what it records before saying so.
    std::thread::sleep(Duration::from_millis(500));
    drop(client);
    stack.kill_hub();

    let instance = stack.paths().instances_dir.join(workspace.instance_id.to_string());
    let mut next = Attached::start_in(&stack.path("workspaces"), &instance);
    next.hello(true);
    // What a worker kept comes straight after the hello's reply, so it would come before this one's.
    let running = next.call(2, WorkerRequest::RunningAgents);
    assert!(
        matches!(&running, WorkerOutcome::Response(WorkerResponse::RunningAgents { agents }) if agents.len() == 1),
        "{running:?}"
    );
    assert_eq!(next.call(3, WorkerRequest::Stop), WorkerOutcome::Response(WorkerResponse::Ok));
    next.close();
}
