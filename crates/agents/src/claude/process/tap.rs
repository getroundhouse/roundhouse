//! What a conformance trial captures the CLI's launches, exits and stdio with, which only a `testing` build can make.

#[cfg(feature = "testing")]
pub(super) use crate::conformance::capture::Tap;
#[cfg(not(feature = "testing"))]
use protocol::model::AgentSessionId;
#[cfg(not(feature = "testing"))]
use serde_json::Value;

/// Without `testing` there's no way to make one, so a process's `Option<Tap>` is always `None`, and the calls that would
/// tell it something compile to nothing.
#[cfg(not(feature = "testing"))]
pub(super) enum Tap {}

#[cfg(not(feature = "testing"))]
impl Tap {
    pub(super) fn launched(&self, _resume: bool, _session: &AgentSessionId, _args: &[String]) {
        match *self {}
    }

    pub(super) fn wrote(&self, _line: &Value) {
        match *self {}
    }

    pub(super) fn read(&self, _line: &str) {
        match *self {}
    }

    pub(super) fn exited(&self, _code: Option<i32>, _signal: Option<i32>, _requested: bool) {
        match *self {}
    }
}
