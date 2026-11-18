//! The approval modes a chat of Claude's can be given, as its adapter claims them, for tests whose agent is Claude, and
//! what else of Claude's claims a client acts on, with the models it offers.

use crate::capability::approvals::{ApprovalMode, Beyond, DecisionPart, Kept, Modes, Requests, Unasked};
use crate::capability::settings::{Effort, EffortOffer, Input, ModelCatalog, ModelOffer, Models, Stage};
use crate::capability::{Capabilities, Claimed, context};
use crate::model::{AgentDescription, AgentKind, ModelName};

/// Full access, which asks about nothing but what Claude's CLI asks about in any mode.
pub fn full_access() -> ApprovalMode {
    ApprovalMode { unasked: Unasked::Everything, beyond: Beyond::Asks, sandbox: None, plan: false }
}

/// Plan mode, which reads unasked and asks about the rest until its plan is approved.
pub fn plan_mode() -> ApprovalMode {
    ApprovalMode { unasked: Unasked::Reads, plan: true, ..full_access() }
}

/// Both, as a kind's claims offer them: full access first, as a chat starts.
pub fn modes() -> Modes {
    Modes { default: full_access(), others: vec![plan_mode()] }
}

/// A model offered at `efforts`, least first, which thinks at `default` when no effort is chosen.
pub fn offer(model: &'static str, label: &str, efforts: &[Effort], default: Effort) -> ModelOffer {
    ModelOffer {
        model: ModelName::named(model),
        label: label.to_string(),
        description: None,
        model_provider: None,
        efforts: efforts.iter().cloned().map(EffortOffer::Id).collect(),
        default_effort: Some(default),
        speeds: Vec::new(),
        inputs: vec![Input::Images],
        context_tokens: None,
        price: None,
        stage: Stage::Current,
    }
}

/// Claude's four aliases at every level up to Max, starting a chat on Opus at Extra high, as its adapter offers them.
pub fn claude_models() -> Models {
    let efforts = [Effort::Low, Effort::Medium, Effort::High, Effort::Xhigh, Effort::Max];
    let offered = [("fable", "Fable"), ("opus", "Opus"), ("sonnet", "Sonnet"), ("haiku", "Haiku")]
        .map(|(model, label)| offer(model, label, &efforts, Effort::Xhigh));
    Models::Static(ModelCatalog { offered: offered.to_vec(), default_model: Some(ModelName::named("opus")) })
}

/// Claude as a hub describes it, with [`claude_claims`], and no login a hub can hold.
pub fn claude() -> AgentDescription {
    AgentDescription {
        kind: AgentKind::Claude,
        capabilities: claude_claims(),
        name: "Claude Code".to_string(),
        usage_heading: "Claude usage".to_string(),
        login: None,
    }
}

/// What Claude's kind claims, as far as a client acts on a kind's claims: its models and their efforts, its modes, a
/// refusal in words, and compacting when asked.
pub fn claude_claims() -> Capabilities {
    let mut claims = Capabilities::default();
    claims.settings.models = Some(claude_models());
    claims.settings.effort = Some(Claimed {});
    claims.approvals.modes = Some(modes());
    let parts = vec![DecisionPart::KeepAllow, DecisionPart::Words, DecisionPart::Stop];
    claims.approvals.requests =
        Some(Requests { parts, kept: vec![Kept::Conversation, Kept::Worktree, Kept::Project, Kept::User] });
    claims.context.compaction = Some(context::Compaction { keeping: context::Keeping::Read });
    claims
}
