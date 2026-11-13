//! What a prompt can carry and do, beyond its text, and when the agent takes it.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Claimed;
use super::area::area;
use super::settings::PersonaName;
use crate::model::ids;

area! { "a prompt";
    /// A prompt's images reach the model as images, in the order the prompt carries them, rather than as paths it has
    /// to read. Which models take them is each model's [`Input`](super::settings::Input)s. Every agent takes a
    /// prompt's files as paths in the worktree, so one without this still sees them.
    Images => images: Claimed,
    /// A prompt's audio reaches the model as audio.
    Audio => audio: Claimed,
    /// A prompt's documents, such as a PDF, reach the model whole, as documents.
    Documents => documents: Claimed,
    /// The hub sends a prompt while a turn runs, and the agent takes it into that turn at its next chance, such as once
    /// a running tool finishes, rather than waiting for the turn to end. Its `PromptTaken` comes as it enters the
    /// conversation, which can be as the next turn's when it comes as the turn ends, and a cancel before then drops
    /// it. When the agent won't take one at all, as Codex won't during a review, the adapter refuses it rather than
    /// keeping it, and the hub keeps it untaken and sends it again, under the same `seq`, once the turn ends, so only
    /// the hub holds a prompt nothing has taken.
    /// Without this, the hub holds every prompt until the turn ends.
    Steer => steer: Claimed,
    /// A prompt can be added to the conversation without starting a turn, for the agent to read with the next prompt
    /// that does, as Claude's `shouldQuery: false` and opencode's `noReply` do. It's taken at once.
    Notes => notes: Claimed,
    /// A question can be answered from the conversation as it stands, without adding to it, and while a turn runs, as
    /// Claude's side questions and Codex's samples are. The hub records the question before it's sent, and the answer
    /// as a [`Report`](super::Report) that names it, so every client shows both, and the agent's next turn knows
    /// nothing of either.
    Asides => asides: Claimed,
    /// The agent's own commands, such as `/review` or one a project defines, are listed whole with what each does and
    /// the arguments it takes, again whenever the list changes, and the hub keeps the newest on the session's record.
    /// A prompt that starts with one runs it.
    Commands => commands: Claimed,
    /// The agent's skills are listed whole with what each is for, again whenever the list changes, and the hub keeps
    /// the newest on the session's record. A prompt can name one for the agent to use, as [`Extras::skill`].
    Skills => skills: Claimed,
    /// A prompt can ask for its turn's last reply to be JSON that matches a schema it gives, as
    /// [`Extras::reply_schema`]. The reply is then reported as JSON before the turn ends, or the turn ends saying the
    /// reply didn't match.
    StructuredReply => structured_reply: Claimed,
    /// A prompt can carry context for the model that the person didn't write, such as what Roundhouse knows of the
    /// workspace, as [`Extras::context`]. It reaches the model with the prompt, and never shows as the person's words,
    /// as Codex's `additionalContext` and opencode's `system` do.
    HiddenContext => hidden_context: Claimed,
}

ids!(
    /// What a prompt starts with to run one of the agent's commands, without its `/`.
    CommandName,
    /// One of the agent's skills, by the name it lists it under.
    SkillName,
);

/// What a prompt asks of the agent beyond its words and its files, which the recorded prompt carries.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct Extras {
    /// The skill it asks the agent to use, as [`Capability::Skills`] says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill: Option<SkillName>,
    /// The persona of the subagent it goes to, rather than the agent, as `background`'s `DirectSubagents` says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subagent: Option<PersonaName>,
    /// The JSON Schema its turn's last reply has to match, as [`Capability::StructuredReply`] says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_schema: Option<Value>,
    /// What the model reads with it that the person didn't write, as [`Capability::HiddenContext`] says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

impl Extras {
    /// The capabilities it needs, which are none for a prompt that asks for nothing more.
    pub fn needs(&self) -> Vec<super::Capability> {
        let Extras { skill, subagent, reply_schema, context } = self;
        [
            skill.as_ref().map(|_| Capability::Skills.into()),
            subagent.as_ref().map(|_| super::background::Capability::DirectSubagents.into()),
            reply_schema.as_ref().map(|_| Capability::StructuredReply.into()),
            context.as_ref().map(|_| Capability::HiddenContext.into()),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

/// One of the agent's own commands, as [`Capability::Commands`] lists them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentCommand {
    pub name: CommandName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// What it takes after its name, as the agent describes it, such as `<file> [focus]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

/// One of the agent's skills, as [`Capability::Skills`] lists them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ListedSkill {
    pub name: SkillName,
    pub description: String,
}
