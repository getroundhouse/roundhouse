use protocol::capability::asking::QuestionAnswer;
use protocol::event::{ApprovalDecision, Asking, EventBody, Question};
use std::borrow::Cow;
use std::collections::BTreeMap;

/// What an approval asks, and how to answer it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Approval<'a> {
    pub(super) asking: Cow<'a, Asking>,
    typed: Vec<protocol::capability::asking::TypedAnswer>,
}

impl<'a> Approval<'a> {
    /// What the request `body` asks, or `None` for any other event.
    pub fn of(body: &'a EventBody) -> Option<Self> {
        let EventBody::ApprovalRequested { asking, request, .. } = body else { return None };
        if let Some(request) = request {
            use protocol::capability::{
                approvals::Asks,
                asking::{Choose, TypedAnswer},
            };
            let (asking, typed) = match &request.asks {
                Asks::Plan { plan, .. } => (Asking::Plan { plan: plan.clone() }, Vec::new()),
                Asks::Questions { questions } => (
                    Asking::Questions {
                        questions: questions
                            .iter()
                            .map(|q| Question {
                                text: q.text.clone(),
                                header: q.header.clone(),
                                multiple: q.choose == Choose::Several,
                                options: q
                                    .options
                                    .iter()
                                    .map(|o| protocol::event::QuestionOption {
                                        label: o.label.clone(),
                                        description: o.description.clone(),
                                    })
                                    .collect(),
                            })
                            .collect(),
                    },
                    questions.iter().map(|q| q.typed).collect::<Vec<TypedAnswer>>(),
                ),
                Asks::Call { call } => (Asking::ToolUse { typed: Some(call.clone()) }, Vec::new()),
                Asks::Network { .. } | Asks::Access { .. } | Asks::Server(_) | Asks::Unknown => {
                    (Asking::Unknown, Vec::new())
                }
            };
            return Some(Approval { asking: Cow::Owned(asking), typed });
        }
        // One that says nothing of what it asks is from a hub from before it typed what Claude's requests asked, and is
        // asked about as any tool's use this build can't read, by its tool and input.
        let asking = asking.as_deref().map_or(Cow::Owned(Asking::Unknown), Cow::Borrowed);
        Some(Approval { asking, typed: Vec::new() })
    }

    pub fn secret(&self, question: usize) -> bool {
        self.typed.get(question) == Some(&protocol::capability::asking::TypedAnswer::Secret)
    }

    pub fn can_type(&self, question: usize) -> bool {
        !matches!(
            self.typed.get(question),
            Some(protocol::capability::asking::TypedAnswer::No | protocol::capability::asking::TypedAnswer::Unknown)
        )
    }

    pub fn asking(&self) -> &Asking {
        &self.asking
    }

    /// The questions it asks, or `None` where it asks something else.
    pub fn questions(&self) -> Option<&[Question]> {
        match &*self.asking {
            Asking::Questions { questions } => Some(questions),
            Asking::ToolUse { .. } | Asking::Plan { .. } | Asking::Unknown => None,
        }
    }

    /// Yes, as it was asked: the tool is used with the input the agent gave it, or the plan is carried out.
    pub fn allowed(&self) -> ApprovalDecision {
        ApprovalDecision::Allow { updated_input: None, updated_permissions: None }
    }

    /// No, in the person's own `words`, which the agent is told.
    pub fn refused_saying(&self, words: String) -> ApprovalDecision {
        ApprovalDecision::Deny { message: Some(words), interrupt: false }
    }

    /// No, with no words of the person's: the tool isn't to be used, the plan wants more work, or the questions are
    /// left unanswered. The adapter tells the agent so.
    pub fn refused(&self) -> ApprovalDecision {
        ApprovalDecision::Deny { message: None, interrupt: false }
    }

    /// The person's answers to its questions, one for each, in the order they were asked, with one that has nothing in
    /// it for a question they left.
    pub fn answered(&self, answers: Vec<QuestionAnswer>) -> ApprovalDecision {
        ApprovalDecision::AnswerByPosition { answers }
    }

    /// The same, holding what it asks, for keeping past the event it was read from.
    pub fn into_owned(self) -> Approval<'static> {
        Approval { asking: Cow::Owned(self.asking.into_owned()), typed: self.typed }
    }
}

/// What the person answered an approval's questions, as the record that closed it keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answered {
    /// One for each question, in the order they were asked.
    ByPosition(Vec<QuestionAnswer>),
    /// By each question's text, with several choices for one joined with a comma and a space, as a client from before
    /// answered, and the hub recorded what such a client sent.
    ByText(BTreeMap<String, String>),
}

impl Answered {
    /// What the record `body` keeps of the answers, or `None` where it keeps none, as for any decision but an answer,
    /// one recorded before the hub kept answers, and any other event.
    pub fn of(body: &EventBody) -> Option<Self> {
        let EventBody::ApprovalResolved { answers_by_text, answers_by_position, .. } = body else { return None };
        match (answers_by_position, answers_by_text) {
            (Some(answers), _) => Some(Answered::ByPosition(answers.clone())),
            (None, Some(answers)) => Some(Answered::ByText(answers.clone())),
            (None, None) => None,
        }
    }

    /// What was answered to `question`, asked at `at` among the questions, in words, or `None` where nothing was.
    pub fn to(&self, at: usize, question: &Question) -> Option<Cow<'_, str>> {
        match self {
            Answered::ByPosition(answers) => {
                let said: Vec<&str> = question.said(answers.get(at)?).collect();
                (!said.is_empty()).then(|| Cow::Owned(said.join(", ")))
            }
            Answered::ByText(answers) => answers.get(&question.text).map(|answer| Cow::Borrowed(answer.as_str())),
        }
    }
}
