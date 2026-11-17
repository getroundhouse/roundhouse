//! The typed runtime boundary, translated into Claude's existing protocol operations.

use protocol::capability::Control;
use protocol::model::{PermissionMode, SessionSettingsChange};

use super::process::ClaudeProcess;
use crate::{AgentError, AgentProcess};

pub(super) async fn apply(process: &ClaudeProcess, control: &Control) -> Result<(), AgentError> {
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
                    effort: settings.effort.clone(),
                })
                .await
        }
        Control::Decide { approval, decision } => process.decide(approval, decision).await,
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

fn unsupported() -> AgentError {
    AgentError::UnknownRequest("Claude does not support that control".into())
}
