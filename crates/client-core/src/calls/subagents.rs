//! Calls to subagents already at work: telling one something, waiting for them, and stopping one. Each names its
//! subagents by the IDs their agent gave them, which their records keep, so a client that has the records can say what
//! each is for.

use protocol::capability::calls::call::{MessageSubagent, StopSubagent, WaitForSubagents};
use protocol::model::SubagentId;

use super::first_line;

/// A call to subagents the agent started, by the IDs it gave them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToSubagents<'a> {
    /// Telling `subagent` `message`.
    Message {
        subagent: &'a SubagentId,
        message: &'a str,
    },
    /// Waiting for `subagents`, or for any of the agent's, where it names none.
    Wait {
        subagents: &'a [SubagentId],
    },
    Stop {
        subagent: &'a SubagentId,
    },
}

impl<'a> From<&'a MessageSubagent> for ToSubagents<'a> {
    fn from(to: &'a MessageSubagent) -> Self {
        Self::Message { subagent: &to.subagent, message: &to.message }
    }
}

impl<'a> From<&'a WaitForSubagents> for ToSubagents<'a> {
    fn from(wait: &'a WaitForSubagents) -> Self {
        Self::Wait { subagents: &wait.subagents }
    }
}

impl<'a> From<&'a StopSubagent> for ToSubagents<'a> {
    fn from(stop: &'a StopSubagent) -> Self {
        Self::Stop { subagent: &stop.subagent }
    }
}

impl<'a> ToSubagents<'a> {
    /// What it does, in words, without saying which subagents, as a client says it where it can't, such as
    /// `Wait for subagents`.
    pub fn title(&self) -> &'static str {
        match self {
            Self::Message { .. } => "Message a subagent",
            Self::Wait { subagents: [] } => "Wait for any subagent",
            Self::Wait { subagents: [_] } => "Wait for a subagent",
            Self::Wait { .. } => "Wait for subagents",
            Self::Stop { .. } => "Stop a subagent",
        }
    }

    /// What it does, in a word or two, for a line that goes on to say which subagents, such as `Wait for`.
    pub fn verb(&self) -> &'static str {
        match self {
            Self::Message { .. } => "Message",
            Self::Wait { .. } => "Wait for",
            Self::Stop { .. } => "Stop",
        }
    }

    /// The subagents it names, in its order.
    pub fn subagents(&self) -> &'a [SubagentId] {
        match self {
            Self::Message { subagent, .. } | Self::Stop { subagent } => std::slice::from_ref(subagent),
            Self::Wait { subagents } => subagents,
        }
    }

    /// One line saying what it's about, as [`super::ToolCall::summary`] says: the start of the message, or the IDs of
    /// the subagents it names, which are all a client without their records has to say which they are. `None` for a
    /// wait for any of them.
    pub fn summary(&self) -> Option<String> {
        if let Self::Message { message, .. } = self {
            return Some(first_line(message).to_string());
        }
        let named: Vec<&str> = self.subagents().iter().map(SubagentId::as_str).collect();
        (!named.is_empty()).then(|| named.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::ToSubagents;
    use crate::calls::{Recorded, ToolCall, typed};
    use protocol::capability::calls::Call;
    use protocol::capability::calls::call::{MessageSubagent, StopSubagent, WaitForSubagents};

    /// Each call to subagents at work is read as one, and said in words, as the CLI prints it: what it does, then the
    /// start of a message, or the subagents it names.
    #[test]
    fn a_call_to_subagents_at_work_says_what_it_does_and_to_which() {
        let message = Call::MessageSubagent(MessageSubagent {
            subagent: "thread-1".into(),
            message: "\nLook at depot.rs too.\nIt has a check of its own.".into(),
        });
        let wait = |subagents: &[&str]| {
            Call::WaitForSubagents(WaitForSubagents {
                subagents: subagents.iter().map(|&id| id.into()).collect(),
                timeout_ms: None,
            })
        };
        let stop = Call::StopSubagent(StopSubagent { subagent: "thread-1".into() });
        let said = |call: &Call| -> (&str, Option<String>) {
            let ToolCall::ToSubagents(to) = ToolCall::read(Recorded::Typed(call)) else {
                panic!("read as another kind")
            };
            assert_eq!(typed::title(call), to.title(), "named the same as a typed call");
            (to.title(), Recorded::Typed(call).summary())
        };

        assert_eq!(said(&message), ("Message a subagent", Some("Look at depot.rs too.".into())));
        assert_eq!(said(&wait(&["thread-1"])), ("Wait for a subagent", Some("thread-1".into())));
        assert_eq!(said(&wait(&["thread-1", "thread-2"])), ("Wait for subagents", Some("thread-1, thread-2".into())));
        assert_eq!(said(&wait(&[])), ("Wait for any subagent", None));
        assert_eq!(said(&stop), ("Stop a subagent", Some("thread-1".into())));
        let Call::MessageSubagent(to) = &message else { unreachable!() };
        let to = ToSubagents::from(to);
        assert_eq!((to.verb(), to.subagents()), ("Message", &["thread-1".into()][..]));
    }
}
