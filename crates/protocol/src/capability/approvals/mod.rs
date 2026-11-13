//! How much an agent does without asking, what it asks, and what the person can decide.
//!
//! A request for approval is a [`Request`], which says what it asks as an [`Asks`], and the person answers with a
//! [`Decision`]. Asking the person questions, or to approve a plan, goes through the same request, and
//! [`super::asking`] has what those carry.

mod claims;
mod decision;
mod modes;
mod request;
mod rules;
mod verdicts;

pub use claims::{Capability, Claims, Requests};
pub use decision::{AfterDenial, Decision, DecisionPart, Lasts, Remember};
pub use modes::{ApprovalMode, Beyond, Modes, Sandbox, Unasked};
pub use request::{Access, Asks, NetworkProtocol, Request};
pub use rules::{Covers, Kept, Rule, RuleEffect};
pub use verdicts::{Allowed, Denied, DeniedBy, Risk, Verdict};
