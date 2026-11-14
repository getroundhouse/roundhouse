//! What a run claims of approvals.

use serde::{Deserialize, Serialize};

use super::{DecisionPart, Kept, Modes};
use crate::capability::Claimed;
use crate::capability::area::area;

area! { "approvals";
    /// The agent runs in any approval mode [`Modes`] offers, chosen at launch and changed as the
    /// [`settings`](crate::capability::settings) area says. It reports every change of mode, its own included, such as
    /// leaving plan mode once a plan is approved, so what a chat shows is the mode the agent is in.
    Modes => modes: Modes,
    /// The agent asks before a use its mode doesn't allow, as a [`Request`](super::Request), and waits for a
    /// [`Decision`](super::Decision) made of the parts [`Requests::parts`] lists. No turn ends with a request open: the
    /// turn waits for it, and one open when the turn is cancelled is resolved as cancelled, by the agent or else by the
    /// adapter, so none is left with nobody able to answer. Where a decision settles other open requests too, as an
    /// opencode refusal refuses every request open in the session, and an approval that keeps a rule allows the open
    /// ones it covers, the adapter reports each of those resolved, as the agent decided it.
    Requests => requests: Requests,
    /// A request comes with rules the agent suggests keeping, as [`Request::suggested`](super::Request::suggested), so
    /// the same use isn't asked about again, such as allowing `npm test` from now on, and a decision can keep one,
    /// where [`Requests::kept`] says.
    SuggestedRules => suggested_rules: Claimed,
    /// A request can ask to reach a host on the network, naming it and how, and a decision can keep a rule for it.
    NetworkRequests => network_requests: Claimed,
    /// A request can ask for more access than the mode gives, such as writing to a folder outside the worktree, and
    /// the person grants it, or part of it, for the turn or the conversation.
    AccessRequests => access_requests: Claimed,
    /// In a mode whose uses beyond it go to [`Beyond::Reviewer`](super::Beyond::Reviewer), a reviewer the agent runs
    /// decides each in the person's place. Each is recorded as a request, which the reviewer's verdict resolves, with
    /// how risky it judged the use and why, whether the agent asks its reviewer as it would the person or only says
    /// what the reviewer decided, as Codex does, so the person sees every use that was reviewed.
    ReviewVerdicts => review_verdicts: Claimed,
    /// The person can overrule a reviewer's refusal, and the use goes ahead.
    Overrule => overrule: Claimed,
    /// Some requests come whatever the mode, as Claude's checks of a removal do. Such a request says so, as
    /// [`Request::always`](super::Request::always), and no rule can be kept from it. The hub can answer it for the
    /// person, as its settings say.
    AlwaysAsks => always_asks: Claimed,
    /// A use refused without asking, by the mode, a rule, a reviewer or a hook, is reported with what refused it and
    /// why, which the hub records, so the person sees why a call failed, as Claude's `permission_denied` says.
    Denials => denials: Claimed,
    /// The rules the agent goes by can be read, whole, each with whether it allows, asks or refuses, what it matches,
    /// and where it's kept, and the hub keeps the newest on the session's record.
    Rules => rules: Claimed,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Requests {
    /// What a decision can do besides allowing a use once or refusing it, which every request takes.
    #[serde(default)]
    pub parts: Vec<DecisionPart>,
    /// Where a rule a decision keeps can be kept. Empty where none can.
    #[serde(default)]
    pub kept: Vec<Kept>,
}
