//! What a conformance trial captures the CLI's launches, exits and stdio with, which only a `testing` build can make. The
//! adapter is what knows which lines belong to which launch, and a program between it and the CLI would change the
//! process the trial is about. The product keeps no such record, since what people write is in it.

#[cfg(any(test, feature = "testing"))]
pub(crate) use crate::conformance::capture::Tap;
#[cfg(not(any(test, feature = "testing")))]
use protocol::model::AgentSessionId;
#[cfg(not(any(test, feature = "testing")))]
use serde_json::Value;

/// Without `testing` there's no way to make one, so a process's `Option<Tap>` is always `None`, and the calls that would
/// tell it something compile to nothing.
#[cfg(not(any(test, feature = "testing")))]
#[derive(Clone)]
pub(crate) enum Tap {}

#[cfg(not(any(test, feature = "testing")))]
impl Tap {
    pub(crate) fn launched(&self, _resume: bool, _session: Option<&AgentSessionId>, _args: &[String]) {
        match *self {}
    }

    pub(crate) fn wrote(&self, _line: &Value) {
        match *self {}
    }

    pub(crate) fn read(&self, _line: &str) {
        match *self {}
    }

    pub(crate) fn exited(&self, _code: Option<i32>, _signal: Option<i32>, _requested: bool) {
        match *self {}
    }
}
