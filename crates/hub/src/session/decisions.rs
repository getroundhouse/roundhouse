//! Translate clients' existing approval replies into the capability decision the run takes.

use super::LiveSession;
use protocol::capability::{
    Capabilities, Control,
    approvals::{AfterDenial, Asks, Decision, Remember},
    asking::PlanContext,
};
use protocol::client::{ApiError, ErrorCode};
use protocol::event::{ApprovalDecision, EventBody, QuestionAnswer};
use protocol::model::ApprovalId;

impl LiveSession {
    pub(super) async fn approval_request(&self, approval: &ApprovalId) -> Result<EventBody, ApiError> {
        let (id, approval) = (self.id.clone(), approval.clone());
        self.hub()?
            .blocking(move |store| store.approval_request(&id, &approval))
            .await?
            .ok_or_else(|| ApiError::new(ErrorCode::Conflict, "the approval request is missing"))
    }
}

pub(super) fn control(
    body: &EventBody,
    approval: ApprovalId,
    decision: &ApprovalDecision,
    claims: Option<&Capabilities>,
) -> Result<Option<Control>, ApiError> {
    let EventBody::ApprovalRequested { request, .. } = body else { return Err(unsupported()) };
    // Legacy requests keep their native decision envelope: translating it can lose edited input, permission
    // updates or the distinction between skipping questions and answering none. Typed requests use run claims.
    let Some(request) = request else { return Ok(None) };
    let claims = claims.ok_or_else(unsupported)?;
    let plan = matches!(request.asks, Asks::Plan { .. });
    let questions = matches!(request.asks, Asks::Questions { .. });
    let decision = match decision {
        ApprovalDecision::Deny { interrupt: false, .. } if questions => Decision::Answer { answers: Vec::new() },
        ApprovalDecision::Allow { updated_input: None, updated_permissions: None } if plan => {
            let modes = claims.approvals.modes.as_ref().ok_or_else(unsupported)?;
            let then = std::iter::once(modes.default)
                .chain(modes.others.iter().copied())
                .find(|m| !m.plan)
                .ok_or_else(unsupported)?;
            Decision::ApprovePlan { then, context: PlanContext::Kept }
        }
        ApprovalDecision::Allow { updated_input: None, updated_permissions: None } => {
            Decision::Allow { edited: None, remember: Remember::Once }
        }
        ApprovalDecision::Deny { message, interrupt: false } if plan => {
            Decision::KeepPlanning { words: message.clone() }
        }
        ApprovalDecision::Deny { message, interrupt } => Decision::Deny {
            words: message.clone(),
            then: if *interrupt { AfterDenial::Stop } else { AfterDenial::Continue },
            keep: None,
        },
        ApprovalDecision::AnswerByPosition { answers } => Decision::Answer { answers: answers.clone() },
        ApprovalDecision::AnswerByText { answers } => {
            let Asks::Questions { questions } = &request.asks else { return Err(unsupported()) };
            Decision::Answer {
                answers: questions
                    .iter()
                    .map(|q| QuestionAnswer { picked: Vec::new(), typed: answers.get(&q.text).cloned() })
                    .collect(),
            }
        }
        ApprovalDecision::Allow { .. } => return Err(unsupported()),
    };
    let control = Control::Decide { approval, decision };
    if !control.allowed(claims) {
        return Err(unsupported());
    }
    Ok(Some(control))
}

pub(super) fn redacted(body: &EventBody, decision: &ApprovalDecision) -> ApprovalDecision {
    let mut decision = decision.clone();
    for (index, question) in protocol::event::secret_questions(body) {
        match &mut decision {
            ApprovalDecision::AnswerByPosition { answers } => {
                if let Some(answer) = answers.get_mut(index) {
                    *answer = QuestionAnswer::default();
                }
            }
            ApprovalDecision::AnswerByText { answers } => {
                answers.remove(question);
            }
            ApprovalDecision::Allow { .. } | ApprovalDecision::Deny { .. } => {}
        }
    }
    decision
}

fn unsupported() -> ApiError {
    ApiError::new(ErrorCode::Unsupported, "the run cannot take that form of decision")
}
