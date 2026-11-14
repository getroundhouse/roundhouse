//! The typed runtime boundary, translated into Claude's existing protocol operations.

use protocol::capability::{
    Control,
    approvals::{AfterDenial, Decision, Remember},
    asking::PlanContext,
    settings,
};
use protocol::event::ApprovalDecision;
use protocol::model::{Effort, PermissionMode, SessionSettingsChange};

use crate::{AgentError, AgentProcess};

pub(super) async fn apply(process: &dyn AgentProcess, control: &Control) -> Result<(), AgentError> {
    if !control.allowed(&process.capabilities()) {
        return Err(unsupported());
    }
    match control {
        Control::Prompt { seq, prompt, .. } => process.send_prompt(*seq, prompt).await,
        Control::Cancel => process.interrupt().await,
        Control::Compact { asked, keeping } => process.compact(*asked, keeping.as_deref()).await,
        Control::Change(settings) => {
            process
                .configure(&SessionSettingsChange {
                    model: settings.model.clone(),
                    permission_mode: settings
                        .mode
                        .map(|mode| if mode.plan { PermissionMode::Plan } else { PermissionMode::FullAccess }),
                    effort: settings.effort.map(effort).transpose()?,
                })
                .await
        }
        Control::Decide { approval, decision } => {
            process.resolve_approval(approval, &decision_for_cli(decision)?).await
        }
        Control::Note { .. }
        | Control::Aside { .. }
        | Control::Overrule { .. }
        | Control::Rewind { .. }
        | Control::UndoRewind
        | Control::RestoreFiles { .. }
        | Control::PreviewRestoreFiles { .. }
        | Control::RunCommand { .. }
        | Control::StopTask { .. }
        | Control::ReadTaskOutput { .. }
        | Control::Reconnect { .. }
        | Control::SignInToServer { .. }
        | Control::SetGoal { .. }
        | Control::PauseGoal
        | Control::ClearGoal
        | Control::Review { .. }
        | Control::ReadRules
        | Control::ListModels
        | Control::ListOutside => Err(unsupported()),
    }
}

fn decision_for_cli(decision: &Decision) -> Result<ApprovalDecision, AgentError> {
    Ok(match decision {
        Decision::Allow { edited: None, remember: Remember::Once }
        | Decision::ApprovePlan { context: PlanContext::Kept, .. } => {
            ApprovalDecision::Allow { updated_input: None, updated_permissions: None }
        }
        Decision::Deny { words, then, keep: None } => {
            ApprovalDecision::Deny { message: words.clone(), interrupt: *then == AfterDenial::Stop }
        }
        Decision::KeepPlanning { words } => ApprovalDecision::Deny { message: words.clone(), interrupt: false },
        Decision::Answer { answers } => ApprovalDecision::AnswerByPosition { answers: answers.clone() },
        Decision::Allow { .. }
        | Decision::Deny { .. }
        | Decision::ApprovePlan { .. }
        | Decision::Grant { .. }
        | Decision::Respond(_) => return Err(unsupported()),
    })
}

fn effort(effort: settings::Effort) -> Result<Effort, AgentError> {
    match effort {
        settings::Effort::Low => Ok(Effort::Low),
        settings::Effort::Medium => Ok(Effort::Medium),
        settings::Effort::High => Ok(Effort::High),
        settings::Effort::Xhigh => Ok(Effort::Xhigh),
        settings::Effort::Max => Ok(Effort::Max),
        settings::Effort::None
        | settings::Effort::Minimal
        | settings::Effort::Ultra
        | settings::Effort::Persistent
        | settings::Effort::Unknown => Err(AgentError::UnknownEffort),
    }
}

fn unsupported() -> AgentError {
    AgentError::UnknownRequest("Claude does not support that control".into())
}
