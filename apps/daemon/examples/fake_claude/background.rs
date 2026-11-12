//! Work left running in the background, listed whole each time it changes, as the real CLI does. It outlives the
//! turn that started it. When it finishes, the agent picks up again by itself, and on SIGTERM it stops and the list
//! empties before the process exits, as with the real CLI. A monitor is the same, started by a `Monitor` call, which
//! only the `task_started` after the list says. A subagent in the background is listed here too, while it works.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::json;

use crate::turn::{Turn, message_id};
use crate::{Config, Input, Out, Prompt, emit};

/// How often a task looks for its file, and for SIGTERM.
const POLL: Duration = Duration::from_millis(20);

/// What's running, in the order it started.
static TASKS: Mutex<Vec<Task>> = Mutex::new(Vec::new());
static TERMINATED: AtomicBool = AtomicBool::new(false);

/// A task in the list, as the CLI lists it.
struct Task {
    id: String,
    /// Its `task_type`: `local_bash` for a command or a monitor, and `local_agent` for a subagent.
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
pub(crate) fn start(turn: &mut Turn, call: Call, file: &str) -> bool {
    let command = format!("until [ -e {file} ]; do sleep 1; done");
    let tool_use_id = match call {
        Call::Command => turn.tool_use(&command),
        Call::Monitor => {
            let message = message_id();
            let tool_use_id = format!("toolu_{message}");
            let input = json!({"command":command,"description":format!("Wait for {file}")});
            turn.finish_message(&message, json!({"type":"tool_use","id":tool_use_id,"name":"Monitor","input":input}));
            tool_use_id
        }
    };
    let id = format!("task_{tool_use_id}");
    listed(turn.config, turn.out, &id, "local_bash", &format!("Wait for {file}"));
    emit(
        turn.out,
        json!({"type":"system","subtype":"task_started","task_id":id,"tool_use_id":tool_use_id,
        "task_type":"local_bash","session_id":turn.config.session_id}),
    );
    turn.tool_result(&tool_use_id, &format!("Command running in background with ID: {id}"), false);
    watch_for_sigterm(turn.config, turn.out);
    let (config, out, file) = (turn.config.clone(), turn.out.clone(), PathBuf::from(file));
    std::thread::spawn(move || {
        while !file.exists() {
            std::thread::sleep(POLL);
        }
        notify(&config, &out, &id, "completed");
        unlisted(&config, &out, &id);
        // As long as the file says, if it says, which leaves a test time to see that nothing is at work in between.
        if let Some(ms) = std::fs::read_to_string(&file).ok().and_then(|said| said.trim().parse().ok()) {
            std::thread::sleep(Duration::from_millis(ms));
        }
        // The real CLI tells the agent, which starts a turn of its own, or joins the one that's running.
        let _ =
            config.input.send(Input::Resume(Prompt { text: format!("say:finished {}", file.display()), uuid: None }));
    });
    false
}

/// Adds a task to the list, and lists them all.
pub(crate) fn listed(config: &Config, out: &Out, id: &str, kind: &'static str, description: &str) {
    TASKS.lock().push(Task { id: id.to_string(), kind, description: description.to_string() });
    list(config, out);
}

/// Takes a task out of the list, and lists the rest.
pub(crate) fn unlisted(config: &Config, out: &Out, id: &str) {
    TASKS.lock().retain(|task| task.id != id);
    list(config, out);
}

fn list(config: &Config, out: &Out) {
    let tasks: Vec<_> = TASKS
        .lock()
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
fn watch_for_sigterm(config: &Config, out: &Out) {
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
        let stopped: Vec<_> = TASKS.lock().drain(..).collect();
        for task in &stopped {
            notify(&config, &out, &task.id, "stopped");
        }
        list(&config, &out);
        std::process::exit(143);
    });
}
