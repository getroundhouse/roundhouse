//! Work left running in the background, listed whole each time it changes, as the real CLI does. It outlives the
//! turn that started it. When it finishes, the agent picks up again by itself, and on SIGTERM it stops and the list
//! empties before the process exits, as with the real CLI. A monitor is the same, started by a `Monitor` call, which
//! only the `task_started` after the list says. A subagent in the background is listed here too, while it works.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::json;

use crate::turn::Turn;
use crate::{Config, Input, Out, Prompt, emit};

/// How often a task looks for its file, and for SIGTERM.
const POLL: Duration = Duration::from_millis(20);

/// What's running, in the order it started.
static TASKS: Mutex<Vec<Task>> = Mutex::new(Vec::new());
static TERMINATED: AtomicBool = AtomicBool::new(false);

/// A task in the list, as the CLI lists it.
struct Task {
    id: String,
    /// Its `task_type`: `local_bash` for a command or a monitor, `local_agent` for a subagent, and `mcp_task` for a
    /// tool's task.
    kind: &'static str,
    description: String,
}

/// The call that starts a task.
#[derive(Clone, Copy)]
pub(crate) enum Call {
    Command,
    Monitor,
}

impl Call {
    /// The call and the file its task waits for, if `command` starts one.
    pub(crate) fn read(command: &str) -> Option<(Call, &str)> {
        let command_for = command.strip_prefix("background:").map(|file| (Call::Command, file));
        command_for.or_else(|| command.strip_prefix("monitor:").map(|file| (Call::Monitor, file)))
    }
}

/// Starts a task that finishes once `file` exists. The turn goes on without waiting for it, so it's never
/// interrupted here. The lines come in the real CLI's order: the call, the list with its task, the `task_started`
/// that names the call, then the call's result.
#[expect(
    clippy::disallowed_methods,
    reason = "the pace a background task looks again for the file it waits for, as the real CLI's command does"
)]
pub(crate) fn start(turn: &mut Turn, call: Call, file: &str) -> bool {
    let command = format!("until [ -e {file} ]; do sleep 0.05; done");
    let description = format!("Wait for {file}");
    let tool_use_id = match call {
        Call::Command => {
            let input = json!({"command":command,"description":description,"run_in_background":true});
            turn.named_tool_use("Bash", input)
        }
        Call::Monitor => turn.named_tool_use("Monitor", json!({"command":command,"description":description})),
    };
    let id = format!("task_{tool_use_id}");
    started(turn.config, turn.out, &id, "local_bash", &tool_use_id, &description);
    turn.tool_result(&tool_use_id, &format!("Command running in background with ID: {id}"), false);
    watch_for_sigterm(turn.config, turn.out);
    let (config, out, file) = (turn.config.clone(), turn.out.clone(), PathBuf::from(file));
    std::thread::spawn(move || {
        while !file.exists() {
            std::thread::sleep(POLL);
        }
        // Once the file it names exists, if it names one, so a test can see what the moment before the agent picks up
        // again is taken for for as long as it needs. Anything else it says is the script the agent picks up with,
        // such as a failure.
        let said = std::fs::read_to_string(&file).unwrap_or_default();
        let said = said.trim();
        let (after, told) = match said.strip_prefix("after:") {
            Some(after) => (Some(PathBuf::from(after)), None),
            None => (None, Some(said).filter(|said| !said.is_empty())),
        };
        let told = told.map_or_else(|| format!("say:finished {}", file.display()), str::to_string);
        finished(&config, &out, &id, "completed");
        while after.as_ref().is_some_and(|after| !after.exists()) {
            std::thread::sleep(POLL);
        }
        pick_up_again(&config, told);
    });
    false
}

/// Lists a task the call `tool_use_id` started, then says which call started it, in the real CLI's order, which the
/// call's result comes after.
pub(crate) fn started(config: &Config, out: &Out, id: &str, kind: &'static str, tool_use_id: &str, description: &str) {
    listed(config, out, id, kind, description);
    emit(
        out,
        json!({"type":"system","subtype":"task_started","task_id":id,"tool_use_id":tool_use_id,"task_type":kind,
        "description":description,"session_id":config.session_id}),
    );
}

/// Ends a task that `completed` or `failed` by itself, taking it out of the list. The agent then picks up again, by
/// [`pick_up_again`].
pub(crate) fn finished(config: &Config, out: &Out, id: &str, status: &str) {
    notify(config, out, id, status);
    unlisted(config, out, id);
}

/// Has the agent pick up again with `told`, the prompt it gives, as the real CLI tells the agent a task has ended
/// either way, which starts a turn of its own, or joins the one that's running.
pub(crate) fn pick_up_again(config: &Config, told: String) {
    let _ = config.input.send(Input::Resume(Prompt { text: told, uuid: None }));
}

/// Adds a task to the list, and lists them all.
pub(crate) fn listed(config: &Config, out: &Out, id: &str, kind: &'static str, description: &str) {
    let mut tasks = TASKS.lock();
    tasks.push(Task { id: id.to_string(), kind, description: description.to_string() });
    list(config, out, &tasks);
}

/// Takes a task out of the list, and lists the rest.
pub(crate) fn unlisted(config: &Config, out: &Out, id: &str) {
    let mut tasks = TASKS.lock();
    tasks.retain(|task| task.id != id);
    list(config, out, &tasks);
}

/// Writes the list as it stands. Whoever changed it still holds it, so the lists are written in the order the list
/// changed, and the last one written is the list as it is. A command, a monitor and a tool's task each end on a thread
/// of their own here, and several can end at once. The real CLI's lists are taken not to cross, since it's one thread
/// that changes its list, though no capture shows it ending several at once.
fn list(config: &Config, out: &Out, tasks: &[Task]) {
    let tasks: Vec<_> = tasks
        .iter()
        .map(|task| json!({"task_id":task.id,"task_type":task.kind,"description":task.description}))
        .collect();
    emit(
        out,
        json!({"type":"system","subtype":"background_tasks_changed","tasks":tasks,"session_id":config.session_id}),
    );
}

fn notify(config: &Config, out: &Out, id: &str, status: &str) {
    emit(
        out,
        json!({"type":"system","subtype":"task_notification","task_id":id,"status":status,
        "session_id":config.session_id}),
    );
}

extern "C" fn on_sigterm(_: libc::c_int) {
    TERMINATED.store(true, Ordering::SeqCst);
}

/// From the first task on, SIGTERM stops every task and reports it, then exits as the real CLI does.
#[expect(
    clippy::disallowed_methods,
    reason = "the pace the agent looks again whether SIGTERM has come, since a signal's handler can only set a flag"
)]
pub(crate) fn watch_for_sigterm(config: &Config, out: &Out) {
    static WATCHING: AtomicBool = AtomicBool::new(false);
    if WATCHING.swap(true, Ordering::SeqCst) {
        return;
    }
    // SAFETY: the handler only stores to an atomic, which is async-signal-safe.
    unsafe { libc::signal(libc::SIGTERM, on_sigterm as *const () as libc::sighandler_t) };
    let (config, out) = (config.clone(), out.clone());
    std::thread::spawn(move || {
        while !TERMINATED.load(Ordering::SeqCst) {
            std::thread::sleep(POLL);
        }
        let mut tasks = TASKS.lock();
        let stopped: Vec<_> = tasks.drain(..).collect();
        // A tool's task goes with no word of it, and with only those listed, the real CLI writes nothing at all.
        let others: Vec<_> = stopped.iter().filter(|task| task.kind != "mcp_task").collect();
        for task in &others {
            notify(&config, &out, &task.id, "stopped");
        }
        if !others.is_empty() {
            list(&config, &out, &tasks);
        }
        std::process::exit(143);
    });
}
