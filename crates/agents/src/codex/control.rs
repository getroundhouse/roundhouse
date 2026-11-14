use std::sync::atomic::Ordering;

use async_trait::async_trait;
use protocol::attachment::AttachmentFile;
use protocol::capability::Control;
use protocol::event::{AgentRunState, ApprovalDecision, ApprovalOutcome, EventBody, TurnOutcome};
use protocol::model::{ApprovalId, PermissionMode, Seq, SessionSettingsChange};
use protocol::prompt::Prompt;
use serde_json::{Value, json};

use super::{
    approvals, notifications,
    running::{CodexProcess, Shared},
};
use crate::{AgentError, AgentProcess};

#[async_trait]
impl AgentProcess for CodexProcess {
    fn capabilities(&self) -> protocol::capability::Capabilities {
        super::claims::claims()
    }
    fn pid(&self) -> u32 {
        self.shared.pid
    }
    fn is_gone(&self) -> bool {
        self.shared.gone.load(Ordering::SeqCst)
    }

    async fn control(&self, control: &Control) -> Result<(), AgentError> {
        if !control.allowed(&self.capabilities()) {
            return Err(AgentError::UnknownRequest("Codex does not support that control".into()));
        }
        let _ordered = self.shared.controls.lock().await;
        match control {
            Control::Prompt { seq, prompt, .. } => prompt_to(&self.shared, *seq, prompt).await,
            Control::Cancel => cancel(&self.shared).await,
            Control::Decide { approval, decision } => approvals::decide(&self.shared, approval, decision).await,
            Control::Change(settings) => change(&self.shared, settings).await,
            Control::Note { .. }
            | Control::Aside { .. }
            | Control::Overrule { .. }
            | Control::Compact { .. }
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
            | Control::ListOutside => Err(AgentError::UnknownRequest("Codex does not support that control".into())),
        }
    }

    async fn send_prompt(&self, seq: Seq, prompt: &Prompt<AttachmentFile>) -> Result<(), AgentError> {
        self.control(&Control::Prompt {
            seq,
            prompt: prompt.clone(),
            extras: protocol::capability::prompts::Extras::default(),
        })
        .await
    }
    async fn resolve_approval(&self, id: &ApprovalId, decision: &ApprovalDecision) -> Result<(), AgentError> {
        let decision = approvals::legacy(&self.shared, id, decision)?;
        self.control(&Control::Decide { approval: id.clone(), decision }).await
    }
    async fn interrupt(&self) -> Result<(), AgentError> {
        self.control(&Control::Cancel).await
    }
    async fn compact(&self, _asked: Seq, _keeping: Option<&str>) -> Result<(), AgentError> {
        Err(AgentError::UnknownRequest("Codex compaction is not offered by this adapter".into()))
    }
    async fn configure(&self, options: &SessionSettingsChange) -> Result<(), AgentError> {
        let settings =
            options.for_capabilities(&self.capabilities()).map_err(|error| AgentError::Refused(error.to_string()))?;
        self.control(&Control::Change(settings)).await
    }
    async fn stop(&self) {
        self.shared.stop().await;
    }
}

async fn change(shared: &Shared, settings: &protocol::capability::settings::Settings) -> Result<(), AgentError> {
    let mode = {
        let mut state = shared.state.lock();
        let model = settings.model.as_ref().unwrap_or(&state.settings.model);
        let effort = settings.effort.unwrap_or_else(|| state.settings.effort.into());
        if super::description::models()
            .offered
            .iter()
            .find(|offer| offer.model == *model)
            .is_some_and(|offer| !offer.efforts.contains(&effort))
        {
            return Err(AgentError::Refused("This Codex model does not offer that effort".into()));
        }
        let effort = protocol::model::Effort::ALL
            .into_iter()
            .find(|level| protocol::capability::settings::Effort::from(*level) == effort)
            .ok_or(AgentError::UnknownEffort)?;
        if let Some(model) = &settings.model {
            state.settings.model.clone_from(model);
        }
        state.settings.effort = effort;
        settings.mode.map(|mode| {
            let mode = if mode.plan { PermissionMode::Plan } else { PermissionMode::FullAccess };
            state.settings.permission_mode = mode;
            mode
        })
    };
    if let Some(permission_mode) = mode {
        shared.event(EventBody::AgentMode { permission_mode }, None).await;
    }
    Ok(())
}

async fn prompt_to(shared: &Shared, seq: Seq, prompt: &Prompt<AttachmentFile>) -> Result<(), AgentError> {
    let mut input = vec![json!({"type":"text","text":prompt.text,"text_elements":[]})];
    for file in &prompt.files {
        input.push(if file.attachment.media_type.is_image() {
            json!({"type":"localImage","path":file.path})
        } else {
            json!({"type":"text","text":format!("Attached file: {}",file.path),"text_elements":[]})
        });
    }
    {
        let mut state = shared.state.lock();
        if !state.waiting.is_empty() {
            return Err(AgentError::Refused("Answer Codex's open request before sending another prompt".into()));
        }
        state.untaken.push_back(seq);
        state.sending = Some(seq);
    }
    let result = turn(shared, input).await;
    {
        let mut state = shared.state.lock();
        state.sending = None;
        if matches!(result, Err(AgentError::Refused(_) | AgentError::UnknownRequest(_))) {
            state.untaken.retain(|pending| *pending != seq);
        }
    }
    match result {
        Ok(()) => {
            let dropped = {
                let mut state = shared.state.lock();
                let dropped = state.turn.is_none() && state.untaken.contains(&seq);
                if dropped {
                    state.untaken.retain(|pending| *pending != seq);
                }
                dropped
            };
            if dropped {
                shared.event(EventBody::PromptDropped { prompt: seq }, None).await;
            }
            Ok(())
        }
        Err(error @ (AgentError::Refused(_) | AgentError::UnknownRequest(_))) => Err(error),
        Err(error) => {
            // A failed write or missing reply does not prove nonacceptance. Stop and drain native observations;
            // supervision settles this delivery once, instead of telling the hub it is safe to send again.
            shared
                .event(
                    EventBody::Notice {
                        level: protocol::event::NoticeLevel::Error,
                        message: error.to_string(),
                        parent_tool_use_id: None,
                    },
                    None,
                )
                .await;
            shared.stop().await;
            Ok(())
        }
    }
}

pub(super) async fn turn(shared: &Shared, input: Vec<Value>) -> Result<(), AgentError> {
    super::turn::send(shared, input).await
}

async fn cancel(shared: &Shared) -> Result<(), AgentError> {
    let (thread, turn) = {
        let mut state = shared.state.lock();
        if state.turn.is_none() && state.waiting.is_empty() && state.untaken.is_empty() {
            return Ok(());
        }
        state.cancelling = true;
        (state.thread.clone(), state.turn.clone())
    };
    let mut done = shared.turn_done.subscribe();
    if let (Some(thread), Some(turn)) = (&thread, &turn) {
        if let Err(error) = shared.rpc.ask("turn/interrupt", json!({"threadId":thread,"turnId":turn})).await {
            shared.stop().await;
            return Err(error);
        }
        if tokio::time::timeout(std::time::Duration::from_secs(30), done.wait_for(|ended| ended.as_ref() == Some(turn)))
            .await
            .is_err()
        {
            shared.stop().await;
            return Err(AgentError::NoAnswer);
        }
    }
    if let Err(error) = super::terminals::stop(shared, thread.as_ref()).await {
        shared.stop().await;
        return Err(error);
    }
    settle(shared).await;
    {
        let mut state = shared.state.lock();
        state.turn = None;
        state.cancelling = false;
        state.turn_mode = None;
        state.plan = None;
        state.reply = None;
    }
    shared.event(notifications::ended(TurnOutcome::Cancelled, None), None).await;
    shared.event(EventBody::AgentState { state: AgentRunState::Idle }, None).await;
    Ok(())
}

pub(super) async fn settle(shared: &Shared) {
    let (prompts, requests, calls) = {
        let mut state = shared.state.lock();
        state.terminals.clear();
        (
            {
                let sending = state.sending;
                let prompts: Vec<_> = state.untaken.iter().copied().filter(|seq| Some(*seq) != sending).collect();
                state.untaken.retain(|seq| Some(*seq) == sending);
                prompts
            },
            state.waiting.drain().map(|(id, _)| id).collect::<Vec<_>>(),
            state.calls.drain().map(|(id, _)| id).collect::<Vec<_>>(),
        )
    };
    for prompt in prompts {
        shared.event(EventBody::PromptDropped { prompt }, None).await;
    }
    for approval_id in requests {
        shared
            .event(
                EventBody::ApprovalResolved {
                    approval_id,
                    outcome: ApprovalOutcome::Cancelled,
                    answers_by_text: None,
                    answers_by_position: None,
                    by_setting: false,
                },
                None,
            )
            .await;
    }
    for tool_use_id in calls {
        shared.event(EventBody::ToolCallDropped { tool_use_id }, None).await;
    }
}
