//! What a worker asks its hub, for a tool its agent called, and the hub's answers. The hub knows which workspace a
//! worker is for, so an ask reaches that workspace and nothing else.

use serde::de::IgnoredAny;
use serde::{Deserialize, Serialize};

use super::WorkerRequest;
use crate::client::Outcome;
use crate::frame::{self, Call};
use crate::model::{PullRequest, ToolUseId, Wait, WaitId, WaitUntil};

/// Which of a worker's asks an answer is for. They're numbered by the worker, apart from the hub's calls.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct AskId(u64);

impl AskId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// The ask a worker's frame is, when the frame as a whole didn't decode, as one from a newer worker after a
    /// rollback, so the hub can still refuse it rather than leave the agent's call waiting until the worker gives up.
    /// Of the frames that name an `id`, only an ask says `"frame":"ask"`.
    pub fn of_unreadable(line: &[u8]) -> Option<Self> {
        #[derive(Deserialize)]
        struct Ask {
            frame: String,
            id: Option<AskId>,
        }
        frame::decode::<Ask>(line).ok().filter(|ask| ask.frame == "ask").and_then(|ask| ask.id)
    }
}

/// A line from a hub to its worker: a call for the worker to answer, or the answer to something it asked. A call's
/// request is unread when this build can't read it, for the worker to refuse, as [`frame::read_call`] says.
#[derive(Debug)]
pub enum ToWorker {
    Call(Call<WorkerRequest>),
    Answer(AskAnswer),
}

/// Only for telling an answer from a call that didn't read, by the field only an answer has.
#[derive(Deserialize)]
struct Answering {
    answer: Option<IgnoredAny>,
}

impl ToWorker {
    /// Reads a call as a call, once, and only a line that names no call as an answer. An untagged enum would read
    /// every line twice, and the second time from a copy of it, which for a call carrying a file costs the file again.
    /// A line that's neither fails with what's wrong with it as an answer if it names one, and as a call if not.
    pub fn decode(line: &[u8]) -> serde_json::Result<Self> {
        let not_a_call = match frame::read_call(line) {
            Ok(call) => return Ok(Self::Call(call)),
            Err(e) => e,
        };
        match frame::decode::<Answering>(line) {
            Ok(Answering { answer: Some(_) }) => frame::decode::<AskAnswer>(line).map(Self::Answer),
            Ok(Answering { answer: None }) | Err(_) => Err(not_a_call),
        }
    }
}

/// The hub's answer to the worker's [`super::WorkerFrame::Ask`] that `answer` names.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AskAnswer {
    pub answer: AskId,
    #[serde(flatten)]
    pub outcome: AskOutcome,
}

pub type AskOutcome = Outcome<AskReply>;

/// What a worker asks its hub. A hub answers one it doesn't know with `Unsupported`, as it does a client's, and a hub
/// from before workers could ask doesn't answer at all, so a worker gives up after a while.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum WorkerAsk {
    /// The workspace's pull request, as the hub reads it for an agent.
    PullRequest,
    /// Starts a wait on the workspace's pull request, for the agent's call `call`, from where it stands now. A branch
    /// without one is refused.
    StartWait {
        #[serde(default)]
        until: WaitUntil,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        call: Option<ToolUseId>,
    },
    /// How one of the session's waits stands.
    Wait { wait: WaitId },
    /// Ends one of the session's waits, as cancelled, unless it has ended already.
    CancelWait { wait: WaitId },
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum AskReply {
    /// What the hub knows of it. `None` when the branch has none.
    PullRequest { pull_request: Option<PullRequest> },
    /// A wait as it stands, and the pull request as the hub last read it.
    Wait { wait: Wait, pull_request: Option<PullRequest> },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::ErrorCode;

    #[test]
    fn a_call_reads_as_a_call_and_an_answer_as_an_answer() {
        let call = br#"{"id":4,"request":{"method":"git_status","path":"/w"}}"#;
        let answer = br#"{"answer":4,"response":{"result":"pull_request","pull_request":null}}"#;
        let refused = br#"{"answer":5,"error":{"code":"unsupported","message":"no"}}"#;

        let ToWorker::Call(Call { id: 4, request: Ok(WorkerRequest::GitStatus(status)) }) =
            ToWorker::decode(call).unwrap()
        else {
            panic!("a call reads as a call");
        };
        assert_eq!(status.path, "/w");
        let ToWorker::Answer(answered) = ToWorker::decode(answer).unwrap() else {
            panic!("an answer reads as an answer");
        };
        assert_eq!(
            answered,
            AskAnswer {
                answer: AskId::new(4),
                outcome: Outcome::Response(AskReply::PullRequest { pull_request: None })
            }
        );
        let ToWorker::Answer(AskAnswer { outcome: Outcome::Error(error), .. }) = ToWorker::decode(refused).unwrap()
        else {
            panic!("a refusal is an answer");
        };
        assert_eq!(error.code, ErrorCode::Unsupported);
    }

    #[test]
    fn a_call_this_build_cant_read_is_still_a_call_and_an_answer_it_cant_read_is_neither() {
        let ToWorker::Call(Call { id: 4, request: Err(call) }) =
            ToWorker::decode(br#"{"id":4,"request":{"method":"git_status"}}"#).unwrap()
        else {
            panic!("a call this build can't read is still a call, for the worker to refuse");
        };
        let answer = ToWorker::decode(br#"{"answer":4,"response":{"result":"nonsense"}}"#).unwrap_err();
        let neither = ToWorker::decode(br#"{"something":4}"#).unwrap_err();

        assert!(call.to_string().contains("path"), "says what the call lacked: {call}");
        assert!(answer.to_string().contains("nonsense"), "says what was wrong with the answer: {answer}");
        assert!(neither.to_string().contains("id"), "says what a call would have had: {neither}");
    }

    #[test]
    fn an_ask_that_doesnt_read_still_names_itself_and_nothing_else_does() {
        let ask = br#"{"frame":"ask","id":9,"session_id":{"newer":true},"ask":{"method":"pull_request"}}"#;
        let reply = br#"{"frame":"reply","id":9,"response":{"result":"newer"}}"#;

        assert_eq!(AskId::of_unreadable(ask), Some(AskId::new(9)));
        assert_eq!(AskId::of_unreadable(reply), None);
        assert_eq!(AskId::of_unreadable(b"not json"), None);
    }

    #[test]
    fn an_ask_from_a_newer_worker_reads_as_unknown() {
        let ask: WorkerAsk = serde_json::from_str(r#"{"method":"something_new","with":1}"#).unwrap();

        assert_eq!(ask, WorkerAsk::Unknown);
    }
}
