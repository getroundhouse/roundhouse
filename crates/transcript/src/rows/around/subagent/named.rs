//! The subagents a call to subagents at work names: the one it tells something or stops, or those it waits for. Its
//! line says what each is for, as the subagent's record says.

use crate::transcript::Model;
use client_core::calls::{Recorded, ToolCall};
use protocol::model::SubagentId;

/// The subagents a call names, in its order.
pub(in crate::rows) struct Names {
    pub(in crate::rows) subagents: Vec<Named>,
    /// Their work is kept apart from the chat's, in the subagent tab, as [`super::Subagent`]'s `apart` says, so the tab
    /// can be opened on one.
    pub(in crate::rows) apart: bool,
}

pub(in crate::rows) struct Named {
    pub(in crate::rows) id: SubagentId,
    /// What it's for, or else its kind, as its record says, once the hub has sent it. An agent that doesn't record its
    /// subagents has none.
    pub(in crate::rows) called: Option<String>,
}

impl Names {
    /// The subagents `call` names, if it's a call to subagents at work.
    pub(in crate::rows) fn read(m: &Model, call: Recorded<'_>) -> Option<Self> {
        let ToolCall::ToSubagents(to) = ToolCall::read(call) else { return None };
        let subagents =
            to.subagents().iter().map(|id| Named { id: id.clone(), called: m.subagents.called(id) }).collect();
        Some(Self { subagents, apart: m.records_subagents() })
    }

    /// The subagent the tab can be opened on: the one they are, where its work is kept apart for the tab to show. A
    /// call that names several has none, since no one of them is the way in.
    pub(in crate::rows) fn opened(&self) -> Option<&Named> {
        let [named] = self.subagents.as_slice() else { return None };
        self.apart.then_some(named)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use client_core::calls::Recorded;
    use protocol::capability::Capabilities;
    use protocol::capability::calls::Call;
    use protocol::capability::calls::call::{MessageSubagent, WaitForSubagents};
    use protocol::fixtures::typed_command;
    use protocol::model::{RunCapabilities, RunId, Subagent as Record, SubagentStatus};

    use super::Names;
    use crate::fixtures::record;
    use crate::transcript::Model;

    /// A call to subagents at work names each as its record does, by what it's for or else its kind, and one with no
    /// record, as an agent that keeps none has, by nothing. Its tab opens on them only where their work is kept apart.
    #[test]
    fn a_call_to_subagents_names_each_as_its_record_does() {
        let mut m = Model::default();
        let recorded = |id: &str, call, description: &str| Record {
            id: id.into(),
            kind: Some("explorer".into()),
            description: Some(description.into()),
            ..record(call, SubagentStatus::Working)
        };
        m.subagents.insert(Arc::new(recorded("thread-1", "spawn-1", "/root/reviewer")));
        m.subagents.insert(Arc::new(recorded("thread-2", "spawn-2", " ")));
        let subagents = ["thread-1", "thread-2", "conv-7"].map(Into::into).to_vec();
        let wait = Call::WaitForSubagents(WaitForSubagents { subagents, timeout_ms: None });
        let read = |m: &Model| Names::read(m, Recorded::Typed(&wait)).unwrap();

        let names = read(&m);

        let called: Vec<_> = names.subagents.iter().map(|named| (named.id.as_str(), named.called.as_deref())).collect();
        assert_eq!(called, [("thread-1", Some("/root/reviewer")), ("thread-2", Some("explorer")), ("conv-7", None)]);
        assert!(names.apart);
        assert!(names.opened().is_none(), "the tab opens on no one of several");
        let message =
            Call::MessageSubagent(MessageSubagent { subagent: "thread-1".into(), message: "And this".into() });
        let to_one = |m: &Model| {
            let names = Names::read(m, Recorded::Typed(&message)).unwrap();
            names.opened().map(|named| named.id.clone())
        };
        assert_eq!(to_one(&m), Some("thread-1".into()));
        m.claims = Some(RunCapabilities::new(RunId::new(1), Capabilities::default()));
        assert!(!read(&m).apart, "for an agent that doesn't record its subagents");
        assert_eq!(to_one(&m), None);
        let command = Call::Command(typed_command("cargo test"));
        assert!(Names::read(&m, Recorded::Typed(&command)).is_none(), "a call that names none");
    }
}
