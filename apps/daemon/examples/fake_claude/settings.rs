//! What the fake CLI runs with: its approval mode, model and effort, as the flags start them and control requests
//! change them, the way the real CLI does.

use parking_lot::Mutex;
use serde_json::{Value, json};

use crate::{Out, emit};

/// What a control request changed. Starts as what the flags said.
pub(crate) static LIVE_MODE: Mutex<Option<String>> = Mutex::new(None);
pub(crate) static LIVE_MODEL: Mutex<Option<String>> = Mutex::new(None);
/// The mode approving a plan goes back to, while in plan mode.
pub(crate) static BEFORE_PLAN: Mutex<Option<String>> = Mutex::new(None);

pub(crate) fn current_mode() -> String {
    LIVE_MODE.lock().clone().unwrap_or_else(|| "default".to_string())
}

pub(crate) fn current_model() -> String {
    LIVE_MODEL.lock().clone().unwrap_or_else(|| "fake-model".to_string())
}

/// The real CLI's levels. It ignores any other, with a warning at launch and silently when it's changed.
pub(crate) const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];
pub(crate) static LIVE_EFFORT: Mutex<Option<String>> = Mutex::new(None);
/// After `keep-effort`, a change of effort is answered as if it took, and changes nothing.
pub(crate) static KEEPS_EFFORT: Mutex<bool> = Mutex::new(false);

/// With a level it doesn't know, the real CLI takes the user's own settings. This stands in for them.
pub(crate) fn current_effort() -> String {
    LIVE_EFFORT.lock().clone().unwrap_or_else(|| "high".to_string())
}

/// `apply_flag_settings` answers success whatever it's given, as the real CLI does, so only `get_settings` says
/// whether an effort took.
pub(crate) fn apply_flag_settings(settings: &Value) {
    let level = settings["effortLevel"].as_str().filter(|level| EFFORTS.contains(level));
    if let Some(level) = level.filter(|_| !*KEEPS_EFFORT.lock()) {
        *LIVE_EFFORT.lock() = Some(level.to_string());
    }
}

/// Reported the way the real CLI does: in order, and for changes it makes itself.
pub(crate) fn set_mode(out: &Out, mode: &str) {
    let before = LIVE_MODE.lock().replace(mode.to_string());
    if mode != "plan" {
        *BEFORE_PLAN.lock() = None;
    } else if before.as_deref() != Some("plan") {
        *BEFORE_PLAN.lock() = Some(before.unwrap_or_else(|| "default".to_string()));
    }
    emit(out, json!({"type":"system","subtype":"status","status":null,"permissionMode":mode}));
}

/// Changes the model, refusing what the real CLI would, and `default`, which it takes as the account's own model, and
/// which the hub never leaves it to choose.
pub(crate) fn set_model(model: &str) -> Result<Value, String> {
    match model {
        "not-a-model" => Err(format!("Model '{model}' not found")),
        "default" => Err("fake claude: asked for `default`".to_string()),
        _ => {
            *LIVE_MODEL.lock() = Some(model.to_string());
            Ok(json!({"model": model}))
        }
    }
}

pub(crate) fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).map(|i| args[i + 1].clone())
}

/// Takes the mode, model and effort the flags give, refusing or ignoring what the real CLI would. A launch with no model
/// or effort, which the real CLI would take, is refused, so every test that starts an agent shows the hub always names
/// both.
pub(crate) fn start_as_launched(args: &[String]) {
    for chosen in ["--model", "--effort"] {
        if flag(args, chosen).is_none() {
            eprintln!("fake claude: launched without {chosen}, which leaves it to the CLI");
            std::process::exit(2);
        }
    }
    // The real CLI's flag doesn't list `default`, although that's what it calls the mode once running.
    let launch_mode = flag(args, "--permission-mode");
    if launch_mode.as_deref() == Some("default") {
        eprintln!("error: option '--permission-mode <mode>' argument 'default' is invalid.");
        std::process::exit(1);
    }
    if launch_mode.as_deref() == Some("plan") {
        *BEFORE_PLAN.lock() = Some("default".to_string());
    }
    *LIVE_MODE.lock() = launch_mode;
    *LIVE_MODEL.lock() = flag(args, "--model");
    if let Some(level) = flag(args, "--effort") {
        if EFFORTS.contains(&level.as_str()) {
            *LIVE_EFFORT.lock() = Some(level);
        } else {
            eprintln!("Warning: Unknown --effort value '{level}' — ignoring it and using the default effort.");
        }
    }
}
