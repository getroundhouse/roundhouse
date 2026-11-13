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
    started(turn.config, turn.out, &id, "local_bash", &tool_use_id, &format!("Wait for {file}"));
    turn.tool_result(&tool_use_id, &format!("Command running in background with ID: {id}"), false);
    watch_for_sigterm(turn.config, turn.out);
    let (config, out, file) = (turn.config.clone(), turn.out.clone(), PathBuf::from(file));
    std::thread::spawn(move || {
        while !file.exists() {
            std::thread::sleep(POLL);
        }
        // As long as the file says, if it says, so a test can see what the moment before the agent picks up again is
        // taken for.
        let ms = std::fs::read_to_string(&file).ok().and_then(|said| said.trim().parse().ok());
        let pause = Duration::from_millis(ms.unwrap_or(0));
        finished(&config, &out, &id, "completed", format!("say:finished {}", file.display()), pause);
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

/// Ends a task that `completed` or `failed` by itself, taking it out of the list, and has the agent pick up again
/// `pause` later with `told`, the prompt it gives, as the real CLI tells the agent either way, which starts a turn of its
/// own, or joins the one that's running.
pub(crate) fn finished(config: &Config, out: &Out, id: &str, status: &str, told: String, pause: Duration) {
    notify(config, out, id, status);
    unlisted(config, out, id);
    std::thread::sleep(pause);
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
