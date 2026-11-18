use protocol::capability::Capabilities;
use protocol::capability::approvals::{AfterDenial, Asks, Decision, Remember, Request};
use protocol::capability::asking::{PlanContext, Question, QuestionAnswer, TypedAnswer};
use protocol::event::EventBody;
use protocol::model::{RunCapabilities, RunId};
use std::borrow::Cow;

/// What an approval asks, and how to answer it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Approval<'a> {
    request: Cow<'a, Request>,
}

impl<'a> Approval<'a> {
    /// What the request `body` asks, or `None` for any other event.
    pub fn of(body: &'a EventBody) -> Option<Self> {
        let EventBody::ApprovalRequested { request, .. } = body else { return None };
        Some(Approval { request: Cow::Borrowed(request) })
    }

    pub fn request(&self) -> &Request {
        &self.request
    }

    pub fn asks(&self) -> &Asks {
        &self.request.asks
    }

    /// The questions it asks, or `None` where it asks something else.
    pub fn questions(&self) -> Option<&[Question]> {
        match self.asks() {
            Asks::Questions { questions } => Some(questions),
            Asks::Call { .. }
            | Asks::Plan { .. }
            | Asks::Network { .. }
            | Asks::Access { .. }
            | Asks::Server(_)
            | Asks::Unknown => None,
        }
    }

    /// Whether what's typed in answer to its question at `question` is secret, which nothing but the agent is told.
    pub fn secret(&self, question: usize) -> bool {
        self.typed(question) == Some(TypedAnswer::Secret)
    }

    /// Whether its question at `question` takes a typed answer.
    pub fn can_type(&self, question: usize) -> bool {
        matches!(self.typed(question), Some(TypedAnswer::Yes | TypedAnswer::Secret))
    }

    fn typed(&self, question: usize) -> Option<TypedAnswer> {
        Some(self.questions()?.get(question)?.typed)
    }

    /// Yes to what it asks, where a yes is all it needs: a call made as the agent asked it, this once, or a plan
    /// approved, to be carried out in the conversation it was planned in. `None` for what a yes doesn't answer, such
    /// as questions, which want answers.
    pub fn approve(&self) -> Option<Decision> {
        match self.asks() {
            Asks::Call { .. } | Asks::Network { .. } => Some(Decision::Allow { remember: Remember::Once }),
            Asks::Plan { .. } => Some(Decision::ApprovePlan { context: PlanContext::Kept }),
            Asks::Questions { .. } | Asks::Access { .. } | Asks::Server(_) | Asks::Unknown => None,
        }
    }

    /// No to what it asks, and the agent carries on: a call isn't made, and a plan is kept to and worked on, the agent
    /// told in the person's `words` where they gave some, and otherwise in its adapter's; or questions are left
    /// unanswered, an empty answer to each, which is how a person chooses not to answer them, and takes no words.
    pub fn refuse(&self, words: Option<String>) -> Decision {
        match self.asks() {
            Asks::Plan { .. } => Decision::KeepPlanning { words },
            Asks::Questions { questions } => {
                Decision::Answer { answers: vec![QuestionAnswer::default(); questions.len()] }
            }
            Asks::Call { .. } | Asks::Network { .. } | Asks::Access { .. } | Asks::Server(_) | Asks::Unknown => {
                Decision::Deny { words, then: AfterDenial::Continue, keep: None }
            }
        }
    }

    /// The person's answers to its questions, one for each, in the order they were asked, with one that has nothing in
    /// it for a question they left.
    pub fn answered(&self, answers: Vec<QuestionAnswer>) -> Decision {
        Decision::Answer { answers }
    }

    /// The answer to its question whose text is `question`, in the words `said`, as a person types one in a terminal,
    /// with any other question left unanswered. Where the words name an option, by its label, that's chosen, as are
    /// the options they name separated by a comma and a space where several can be, and otherwise they're what was
    /// typed. `None` where it asks no such question.
    pub fn answered_in_words(&self, question: &str, said: &str) -> Option<Decision> {
        let questions = self.questions()?;
        let at = questions.iter().position(|asked| asked.text == question)?;
        let mut answers = vec![QuestionAnswer::default(); questions.len()];
        answers[at] = in_words(&questions[at], said);
        Some(self.answered(answers))
    }

    /// The same, holding what it asks, for keeping past the event it was read from.
    pub fn into_owned(self) -> Approval<'static> {
        Approval { request: Cow::Owned(self.request.into_owned()) }
    }
}

/// What the run that asked a request claims, which says what a decision on it can be: `running`, the claims of the run
/// the session's record says goes, where that's `asked`, the run that asked, and otherwise `kind`, what a run of the
/// session's kind claims, as near as anything says of a run that's gone.
pub fn claims_of_the_asker<'c>(
    running: Option<&'c RunCapabilities>,
    asked: Option<RunId>,
    kind: Option<&'c Capabilities>,
) -> Option<&'c Capabilities> {
    running.filter(|running| Some(running.run) == asked).map(|running| &*running.claims).or(kind)
}

/// `said`, an answer to `question` in words, as [`Approval::answered_in_words`] reads it.
fn in_words(question: &Question, said: &str) -> QuestionAnswer {
    let at = |label: &str| {
        let at = question.options.iter().position(|option| option.label == label)?;
        u32::try_from(at).ok()
    };
    let picked = match at(said) {
        Some(one) => Some(vec![one]),
        None if question.several() => said.split(", ").map(at).collect(),
        None => None,
    };
    match picked {
        Some(picked) => QuestionAnswer { picked, typed: None },
        None => QuestionAnswer { picked: Vec::new(), typed: Some(said.to_string()) },
    }
}

/// What the person answered an approval's questions, as the record that closed it keeps it: one for each question, in
/// the order they were asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answered(pub Vec<QuestionAnswer>);

impl Answered {
    /// What the record `body` keeps of the answers, or `None` where it keeps none, as for any decision but an answer,
    /// and any other event.
    pub fn of(body: &EventBody) -> Option<Self> {
        let EventBody::ApprovalResolved { decision: Some(decision), .. } = body else { return None };
        let Decision::Answer { answers } = &**decision else { return None };
        Some(Answered(answers.clone()))
    }

    /// What was answered to `question`, asked at `at` among the questions, in words, or `None` where nothing was.
    pub fn to(&self, at: usize, question: &Question) -> Option<String> {
        let said: Vec<&str> = question.said(self.0.get(at)?).collect();
        (!said.is_empty()).then(|| said.join(", "))
    }
}
