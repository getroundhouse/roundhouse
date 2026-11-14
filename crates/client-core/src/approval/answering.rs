use super::older;
use protocol::event::{ApprovalDecision, Asking, EventBody, Question, QuestionAnswer};
use std::borrow::Cow;
use std::collections::BTreeMap;

/// What an approval asks, and how to answer it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Approval<'a> {
    pub(super) asking: Cow<'a, Asking>,
    /// It's from before adapters said what each asks, so its adapter may take answers only by text, and a refusal only
    /// in a client's words.
    pub(super) from_before: bool,
    typed: Vec<protocol::capability::asking::TypedAnswer>,
}

impl<'a> Approval<'a> {
    /// What the request `body` asks, or `None` for any other event.
    pub fn of(body: &'a EventBody) -> Option<Self> {
        let EventBody::ApprovalRequested { tool_name, input, asking, request, .. } = body else { return None };
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
                Asks::Call { .. } => (Asking::ToolUse { call: None }, Vec::new()),
                Asks::Network { .. } | Asks::Access { .. } | Asks::Server(_) | Asks::Unknown => {
                    (Asking::Unknown, Vec::new())
                }
            };
            return Some(Approval { asking: Cow::Owned(asking), from_before: false, typed });
        }
        Some(match asking.as_deref() {
            Some(asking) => Approval { asking: Cow::Borrowed(asking), from_before: false, typed: Vec::new() },
            None => {
                Approval { asking: Cow::Owned(older::asking(tool_name, input)), from_before: true, typed: Vec::new() }
            }
        })
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
    /// left unanswered. The adapter tells the agent so, or for a request from before, the words clients used then do.
    pub fn refused(&self) -> ApprovalDecision {
        let message = self.from_before.then(|| older::refusal(&self.asking).to_string());
        ApprovalDecision::Deny { message, interrupt: false }
    }

    /// The person's answers to its questions, one for each, in the order they were asked, with one that has nothing in
    /// it for a question they left. A request from before has them by each question's text.
    pub fn answered(&self, answers: Vec<QuestionAnswer>) -> ApprovalDecision {
        if !self.from_before {
            return ApprovalDecision::AnswerByPosition { answers };
        }
        let questions = self.questions().unwrap_or_default();
        ApprovalDecision::AnswerByText { answers: older::by_text(questions, &answers) }
    }

    /// The same, holding what it asks, for keeping past the event it was read from.
    pub fn into_owned(self) -> Approval<'static> {
        Approval { asking: Cow::Owned(self.asking.into_owned()), from_before: self.from_before, typed: self.typed }
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
