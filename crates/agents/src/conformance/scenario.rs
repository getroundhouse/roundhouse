//! A scenario: what the driver does to an agent through its adapter, and what's checked of what the adapter made of
//! it, in Roundhouse's terms. Each agent's binding gives it a script for the scripted model, and it's the same
//! whichever model it runs against.

use std::fmt;
use std::future::Future;
use std::pin::Pin;

use protocol::model::PermissionMode;

use super::driver::Driver;
use super::trial::Trial;

/// What a scenario's CLI talks to in place of a model's API.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Against {
    /// A stand-in for the API whose every reply the scenario's script makes, with no network but to it. Free, and the
    /// same every time, so its captures are fixtures.
    ScriptedModel,
    /// The real API, on the cheapest model, which costs a little. Its capture says what the account is and has used,
    /// so it's never kept.
    RealModel,
}

impl fmt::Display for Against {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Against::ScriptedModel => "scripted",
            Against::RealModel => "real",
        })
    }
}

/// Both models, for a scenario a real model can be asked to play.
pub const BOTH: &[Against] = &[Against::ScriptedModel, Against::RealModel];
/// Only the scripted model, for a scenario only a script can make happen, such as a model that thinks on cue.
pub const SCRIPTED_ONLY: &[Against] = &[Against::ScriptedModel];

/// Why a step couldn't go on, such as what it waited for never coming.
#[derive(Debug)]
pub struct Failed(pub String);

impl fmt::Display for Failed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

pub type StepsFuture<'a> = Pin<Box<dyn Future<Output = Result<(), Failed>> + Send + 'a>>;

/// What the driver does, from the first launch. Whatever still runs once they're done is stopped.
pub type Steps = for<'a> fn(&'a mut Driver) -> StepsFuture<'a>;

pub struct Check {
    /// What holds when it passes, such as "a command ran".
    pub name: &'static str,
    /// Why it doesn't hold, if it doesn't.
    pub check: fn(&Trial) -> Result<(), String>,
}

pub struct Scenario {
    /// Names its fixtures, so it's a file name.
    pub name: &'static str,
    pub against: &'static [Against],
    /// The chat's approval mode, which each process is launched in.
    pub mode: PermissionMode,
    pub steps: Steps,
    pub checks: &'static [Check],
}
