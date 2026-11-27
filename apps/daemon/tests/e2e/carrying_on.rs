//! A hub that goes and leaves its workers running, as one restarting for an update does and one that crashes can, and
//! the next hub carrying on with their agents.

#![cfg(test)]

use std::path::{Path, PathBuf};

use client_core::Restart;
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{BranchName, Branches, Head, Seq, SetupStatus};
use protocol::worker::{WorkerFrame, WorkerOutcome, WorkerRequest, WorkerResponse};

use crate::dialing::NextHub;

use crate::support::{
    self, Follower, Gate, Stack, assert_gapless, assistant_text, commit_settings, is_alive, pending_approval, read_all,
    started_pid, wait_for_approval, wait_for_setup, wait_for_turn_end, wait_until,
};

/// As an update has the hub do: asks it to restart, and waits for it to go. A client starts the next one.
pub(crate) fn restart(stack: &mut Stack) {
    restart_then(stack, Restart::Exited, || {});
}

/// As an update has it: the hub restarts, and the next one starts. The hub goes at once, or once a setup under way has
/// finished, whichever it says, where [`restart`] is for a test that says which. A hub that goes by itself first writes
/// to its cache file what it kept of everything it observed, so the next knows all it knew, such as where a
/// workspace's branch is pushed. One that's killed, as [`Stack::restart_hub`] has it, can go with the last of that
/// still to write, and the next then knows less than the first told its clients.
pub(crate) fn update(stack: &mut Stack) {
    client_core::restart_hub(&stack.client()).unwrap();
    wait_for_the_hub_to_go(stack);
    stack.start_hub();
}

/// Asks the hub to restart, as `restart` does, and once it has said how that went, does `meanwhile`, such as letting
/// what it waits for finish, and waits for it to go.
fn restart_then(stack: &mut Stack, said: Restart, meanwhile: impl FnOnce()) {
    assert_eq!(client_core::restart_hub(&stack.client()).unwrap(), said);
    meanwhile();
    wait_for_the_hub_to_go(stack);
}

/// Waits for the stack's hub to exit by itself, as one asked to restart does.
fn wait_for_the_hub_to_go(stack: &mut Stack) {
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
    let (workspace, session) = stack.workspace(&client);
    let (go, finished) = (stack.path("go"), stack.path("finished"));
    let script = format!("say:before; hang-until:{}; say:after; tool:touch {}", go.display(), finished.display());
    client.session_prompt(&session.id, &script, None).unwrap();
    wait_until("the agent to start its tool", || assistant_text(&read_all(&client, &session.id)) == "before");
    let agent = started_pid(&read_all(&client, &session.id)).unwrap();
    assert!(client.hub_status().unwrap().keeps_workers, "so the app knows it can restart this hub at once");
    drop(client);

    restart(&mut stack);
    assert!(is_alive(agent), "the restart left the agent running");
    // So the agent finishes its turn with no hub there. All that's left once its last call has run is that call's
    // result and the turn's end, which are written straight after.
    std::fs::write(&go, "").unwrap();
    wait_until("the agent to run its last call", || finished.exists());
    stack.start_hub();

    let events = wait_for_turn_end(&stack.client(), &session.id, 1);
    assert_one_agent_said_it_all_once(&events, "beforeafter");
    // Its worker dialed the next hub, at the address its ticket names, as it dialed the first.
    let log =
        std::fs::read_to_string(protocol::paths::worker_log(&stack.paths().worker_logs_dir, &workspace.instance_id));
    assert_eq!(log.unwrap().matches("\"event\":\"hub.admitted\"").count(), 2);
}

/// A message the agent was writing as its hub went is whole in the next hub's tail, with what it wrote while there was
/// no hub, rather than starting from the next delta, so a chat shows it as it was until it's finished.
#[test]
fn a_message_the_agent_is_writing_as_the_hub_restarts_is_whole_in_the_next_hub_s_tail() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    // Thinking until the test ends, so it's still writing once the next hub has started.
    let never = stack.path("never");
    client.session_prompt(&session.id, &format!("think-until:{}:t", never.display()), None).unwrap();
    let subscription = client.follow_session(&session.id, Seq::new(1)).unwrap();
    let mut before = Follower::default();
    before.drain_until(&subscription, "the agent to start thinking", |f| f.tail_text().contains("t2 "));
    drop((subscription, client));

    restart(&mut stack);
    // So the agent writes with no hub there: two more than it had said as the hub went, since the first of them may
    // have been on its way already.
    let said = || std::fs::read_to_string(format!("{}.said", never.display())).map_or(0, |said| said.lines().count());
    let as_the_hub_went = said();
    wait_until("the agent to write with no hub there", || said() >= as_the_hub_went + 2);
    stack.start_hub();

    let client = stack.client();
    let subscription = client.follow_session(&session.id, Seq::new(1)).unwrap();
    let mut after = Follower::default();
    let heard = before.tail_text().len();
    after.drain_until(&subscription, "the thinking to go on", |f| f.tail_text().len() > heard + 40);
    let tail = after.tail_text();
    let every_chunk = tail.split_whitespace().enumerate().all(|(i, chunk)| chunk == format!("t{i}"));
    assert!(every_chunk, "the next hub's tail of the message is missing what came before: {tail:?}");
}

#[test]
fn a_hub_crash_leaves_the_agent_running_and_the_next_hub_carries_on_with_it() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let tool = Gate::new(&stack, "tool");
    client.session_prompt(&session.id, &format!("say:before; hang-until:{tool}; say:after"), None).unwrap();
    // The hub may go before saying it has recorded this, so the next hub may hear it again.
    wait_until("the agent to start its tool", || assistant_text(&read_all(&client, &session.id)) == "before");
    let agent = started_pid(&read_all(&client, &session.id)).unwrap();
    drop(client);

    stack.kill_hub();
    assert!(is_alive(agent), "the crash left the agent running");
    stack.start_hub();

    let client = stack.client();
    // The rest of the turn comes once the next hub has the worker back, so it's that hub that carries on with it.
    wait_until("the next hub to have the worker", || client.hub_status().unwrap().workers == 1);
    tool.open();
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
    let allow = protocol::fixtures::allow_once();
    client.session_decide(&session.id, &approval, allow).unwrap();
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
/// checked out after the last hub went, which the worker's watch has read already and sent where nothing recorded it.
/// Here that's a stand-in for a hub rather than none, so the test knows the watch read it before the next hub came.
#[test]
fn the_next_hub_hears_of_a_branch_checked_out_after_the_last_one_went() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:first", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let on = |head: Head| move |branches: Option<&Branches>| branches.is_some_and(|branches| branches.head == head);
    let started_on = Head::Branch { name: workspace.branch.clone() };
    support::branches_once(&client, &workspace.id, "the branches to be known", on(started_on));
    drop(client);

    stack.kill_hub();
    let mut stand_in = NextHub::admitting(&stack.paths().data_dir);
    support::git(Path::new(&workspace.path.unwrap()), &["switch", "-q", "-c", "next-thing"]);
    let next_thing = Head::Branch { name: BranchName::from("next-thing") };
    stand_in.frame_until("the watch the last hub asked for to read the branch", |frame| match frame {
        WorkerFrame::Branches { branches } => (branches.head == next_thing).then_some(()),
        _ => None,
    });
    stand_in.close();
    stack.start_hub();

    let client = stack.client();
    support::branches_once(&client, &workspace.id, "the next hub to hear of the branch", on(next_thing));
}

/// So a restart can't interrupt a setup that the next hub would only find failed.
#[test]
fn a_restart_waits_for_a_setup_script_to_finish() {
    let mut stack = Stack::start();
    let setting_up = Gate::new(&stack, "setting-up");
    commit_settings(&stack, &format!("[scripts]\nsetup = \"{}; touch ready.txt\"\n", setting_up.shell_wait()));
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    wait_until("the setup to start", || client.hub_status().unwrap().running_setups == 1);
    drop(client);

    // Asking says it's still finishing, rather than waiting for the setup.
    restart_then(&mut stack, Restart::Finishing, || setting_up.open());
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
/// waits, and returns the agent's PID. It says it's finishing, and goes only once the script ends, as
/// `a_restart_waits_for_a_setup_script_to_finish` shows, which is long after the test.
fn an_agent_at_work_while_a_restart_waits(stack: &Stack) -> u32 {
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:before; hang", None).unwrap();
    wait_until("the agent to start its tool", || assistant_text(&read_all(&client, &session.id)) == "before");
    let agent = started_pid(&read_all(&client, &session.id)).unwrap();
    commit_settings(stack, "[scripts]\nsetup = \"sleep 30\"\n");
    client.workspace_create(&workspace.repo_id, None, None, None).unwrap();
    wait_until("the setup to start", || client.hub_status().unwrap().running_setups == 1);
    assert_eq!(client_core::restart_hub(&client).unwrap(), Restart::Finishing);
    agent
}

/// So the next hub isn't sent again what the last one recorded: a hub that leaves its worker for the next, as a
/// restart does, waits for the worker to say it has let go of it. What a running hub says, which is all a crash leaves,
/// is in the hub's own tests, in `worker_link::acks`.
#[test]
fn a_worker_lets_go_of_what_its_hub_said_it_recorded() {
    let mut stack = Stack::start();
    // Its agent would say how full its context is after its turn ends, which could come after the hub last said what
    // it recorded, and be kept for the next hub, rightly.
    std::fs::create_dir_all(stack.agent_state()).unwrap();
    std::fs::write(stack.agent_state().join("no-context"), "").unwrap();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:first", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    drop(client);
    restart(&mut stack);

    let mut next = NextHub::admitting(&stack.paths().data_dir);
    // What a worker kept comes straight after the hello's reply, so it would come before this one's.
    let running = next.call(2, WorkerRequest::RunningAgents);
    assert!(
        matches!(&running, WorkerOutcome::Response(WorkerResponse::RunningAgents(reply)) if reply.agents.len() == 1),
        "{running:?}"
    );
    assert_eq!(next.call(3, WorkerRequest::Stop), WorkerOutcome::Response(WorkerResponse::Ok));
    next.close();
}

/// So an update doesn't end the agents of a worker an earlier build left listening on a socket, which doesn't dial: the
/// first hub whose local workers dial still reaches it through `worker --attach`. This one has no agent, so the hub
/// ends it once it has reached it, as it would a worker of its own build.
#[test]
fn a_local_worker_an_earlier_build_left_listening_is_reached_by_the_next_hub() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:first", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    drop(client);
    // Which stops its worker, and forgets it dialed, as of an instance whose worker an earlier build started.
    stack.stop_hub();
    let instance = stack.paths().instances_dir.join(workspace.instance_id.to_string());
    let record = instance.join(protocol::paths::WORKER_RECORD_FILE);
    wait_until("the hub's worker to stop", || {
        process::ProcessRecord::read(&record).is_none_or(|worker| !worker.is_running())
    });
    let logged_before = std::fs::metadata(stack.hub_log()).unwrap().len();

    crate::attaching::start_listening(&stack.path("workspaces"), &instance);
    let listening = process::ProcessRecord::read(&record).unwrap();
    stack.start_hub();

    let logged = || {
        let logged = std::fs::read(stack.hub_log()).unwrap();
        String::from_utf8_lossy(&logged[usize::try_from(logged_before).unwrap()..]).into_owned()
    };
    wait_until("the hub to reach the worker", || logged().contains("\"event\":\"recovery.worker_connected\""));
    assert!(!logged().contains("recovery.worker_unreached"), "{}", logged());
    wait_until("the hub to end the worker it had no use for", || !is_alive(listening.pid));
}
