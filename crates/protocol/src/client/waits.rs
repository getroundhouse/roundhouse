//! The requests about agents' waits on their pull request, which the hub keeps. Each is the body of the
//! `ClientRequest` of its name, which has the same form on the wire as if its fields were the request's own. A hub
//! older than one refuses it, as `is_unsupported` says.

use serde::{Deserialize, Serialize};

use crate::model::{SessionId, WaitId};

/// Calls off the session's wait `wait`, for the person, as the agent can itself, so it's over as
/// [`WaitEnd::CalledOffByPerson`](crate::model::WaitEnd::CalledOffByPerson) says. It names the wait, as the session's
/// record shows it, so one that has ended calls nothing off, and sent again after the agent has started another, it
/// leaves that one going. Answers with the session.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionCallOffWait {
    pub id: SessionId,
    pub wait: WaitId,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::client::ClientRequest;

    /// The same as if its fields were the request's own.
    #[test]
    fn calling_off_a_wait_names_the_session_and_the_wait_beside_its_method() {
        let request = ClientRequest::SessionCallOffWait(SessionCallOffWait { id: "s1".into(), wait: "w1".into() });
        let wire = json!({"method": "session_call_off_wait", "id": "s1", "wait": "w1"});

        assert_eq!(serde_json::to_value(&request).unwrap(), wire);
        assert_eq!(serde_json::from_value::<ClientRequest>(wire).unwrap(), request);
    }
}
