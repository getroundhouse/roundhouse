//! Who wrote a prompt.

use serde::{Deserialize, Serialize};

use crate::model::{ProposalId, RevisionNumber};

/// Who wrote a prompt: the person, or the hub on their behalf, saying why. A queued prompt keeps it to the prompt it's
/// recorded as, so a client draws one the hub wrote for what it is, rather than as something the person said, and the
/// hub doesn't count it as the person's words, such as when it names a chat. It's left out of what's sent for the
/// person's, so a prompt from before it was said is theirs.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(tag = "from", rename_all = "snake_case")]
pub enum PromptFrom {
    #[default]
    Person,
    /// The hub, telling the agent the person's response to the revision `revision` of `proposal`, whose kind, such as
    /// a choice, the revision says, so a prompt for any kind is written alike.
    ProposalResponse { proposal: ProposalId, revision: RevisionNumber },
    /// One this build doesn't know, from a newer hub, which isn't the person's.
    #[serde(other)]
    Unknown,
}

impl PromptFrom {
    /// Whether the person wrote it.
    pub fn is_person(&self) -> bool {
        matches!(self, PromptFrom::Person)
    }

    /// The proposal it tells the agent the person's response to, if it's one the hub wrote for that.
    pub fn responds_to(&self) -> Option<&ProposalId> {
        if let PromptFrom::ProposalResponse { proposal, .. } = self { Some(proposal) } else { None }
    }
}

#[cfg(test)]
mod tests {
    use super::PromptFrom;
    use crate::model::RevisionNumber;

    #[test]
    fn the_hubs_answer_says_its_proposal_and_one_from_a_newer_hub_isnt_the_persons() {
        let answer = PromptFrom::ProposalResponse { proposal: "p".into(), revision: RevisionNumber(2) };
        let sent = serde_json::to_value(&answer).unwrap();
        let newer: PromptFrom = serde_json::from_value(serde_json::json!({ "from": "a_schedule", "at": 9 })).unwrap();

        assert_eq!(sent, serde_json::json!({ "from": "proposal_response", "proposal": "p", "revision": 2 }));
        assert_eq!(serde_json::from_value::<PromptFrom>(sent).unwrap().responds_to(), Some(&"p".into()));
        assert_eq!((newer.is_person(), PromptFrom::default().is_person()), (false, true));
    }
}
