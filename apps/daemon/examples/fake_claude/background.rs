//! Work left running in the background, listed whole each time it changes, as the real CLI does. It outlives the
//! turn that started it. When it finishes, the agent picks up again by itself, and on SIGTERM it stops and the list
//! empties before the process exits, as with the real CLI.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::json;

use crate::turn::Turn;
use crate::{Config, Input, Out, emit};

/// How often a task looks for its file, and for SIGTERM.
const POLL: Duration = Duration::from_millis(20);

/// What's running, by ID and file.
static TASKS: Mutex<Vec<(String, PathBuf)>> = Mutex::new(Vec::new());
static TERMINATED: AtomicBool = AtomicBool::new(false);

/// Starts a task that finishes once `file` exists. The turn goes on without waiting for it, so it's never
/// interrupted here.
pub(crate) fn start(turn: &mut Turn, file: &str) -> bool {
    let tool_use_id = turn.tool_use(&format!("until [ -e {file} ]; do sleep 1; done"));
    let id = format!("task_{tool_use_id}");
    TASKS.lock().push((id.clone(), PathBuf::from(file)));
    list(turn.config, turn.out);
    turn.tool_result(&tool_use_id, &format!("Command running in background with ID: {id}"), false);
    watch_for_sigterm(turn.config, turn.out);
    let (config, out, file) = (turn.config.clone(), turn.out.clone(), PathBuf::from(file));
    std::thread::spawn(move || {
        while !file.exists() {
            std::thread::sleep(POLL);
        }
        TASKS.lock().retain(|(task, _)| *task != id);
        notify(&config, &out, &id, "completed");
        list(&config, &out);
        // The real CLI tells the agent, which starts a turn of its own, or joins the one that's running.
        let _ = config.input.send(Input::Resume(format!("say:finished {}", file.display())));
    });
    false
}

fn list(config: &Config, out: &Out) {
    let tasks: Vec<_> = TASKS
        .lock()
        .iter()
        .map(|(id, file)| {
            let description = format!("Wait for {}", file.display());
            json!({"task_id":id,"task_type":"local_bash","description":description})
        })
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
        for (id, _) in &stopped {
            notify(&config, &out, id, "stopped");
        }
        list(&config, &out);
        std::process::exit(143);
    });
}
