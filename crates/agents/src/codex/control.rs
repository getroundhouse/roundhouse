use std::sync::atomic::Ordering;

use async_trait::async_trait;
use protocol::attachment::AttachmentFile;
use protocol::capability::Control;
use protocol::event::{AgentRunState, EventBody, TurnOutcome};
use protocol::model::Seq;
use protocol::prompt::Prompt;
use serde_json::{Value, json};

use super::{
    approvals, notifications,
    running::{CodexProcess, Shared},
    state::{AskedCompaction, Ended},
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
            // Codex reads nothing to keep, which its claims say, so none is sent.
            Control::Compact { asked, .. } => compact(&self.shared, *asked).await,
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
            | Control::ListOutside
            | Control::Interaction { .. } => {
                Err(AgentError::UnknownRequest("Codex does not support that control".into()))
            }
        }
    }

    async fn stop(&self) {
        self.shared.stop().await;
    }
}

async fn change(shared: &Shared, settings: &protocol::capability::settings::Settings) -> Result<(), AgentError> {
    let mode = {
        let mut state = shared.state.lock();
        if let Some(effort) = &settings.effort {
            state.settings.effort = effort.known().ok_or(AgentError::UnknownEffort)?;
        }
        if let Some(model) = &settings.model {
            state.settings.model.clone_from(model);
        }
        settings.mode.inspect(|&mode| state.settings.mode = mode)
    };
    if let Some(mode) = mode {
        shared.event(EventBody::AgentMode { mode }, None).await;
    }
    Ok(())
}

/// Whether Codex said it took nothing of a prompt: it refused it, or it takes it only once the turn ends, so the hub can
/// send it again.
fn took_nothing(error: &AgentError) -> bool {
    super::rpc::refused(error) || matches!(error, AgentError::AfterTheTurn)
}

async fn prompt_to(shared: &Shared, seq: Seq, prompt: &Prompt<AttachmentFile>) -> Result<(), AgentError> {
    let input = turn_input(prompt);
    {
        let mut state = shared.state.lock();
        if state.asking() {
            return Err(AgentError::Refused("Answer Codex's open request before sending another prompt".into()));
        }
        state.untaken.push_back(seq);
        state.sending = Some(seq);
    }
    let result = turn(shared, input).await;
    {
        let mut state = shared.state.lock();
        state.sending = None;
        if result.as_ref().is_err_and(took_nothing) {
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
        Err(error) if took_nothing(&error) => Err(error),
        Err(error) => {
            // A failed write or missing reply does not prove nonacceptance. Stop and drain native observations;
            // supervision settles this delivery once, instead of telling the hub it is safe to send again.
            shared
                .event(
                    EventBody::Notice {
                        level: protocol::event::NoticeLevel::Error,
                        message: error.to_string(),
                        subagent: None,
                    },
                    None,
                )
                .await;
            shared.stop().await;
            Ok(())
        }
    }
}

/// Asks Codex to compact the thread, the request recorded at `asked`, which it does in a turn of its own once it has
/// answered. That turn starting takes the request, as [`notifications`] says. Only between turns, as the hub asks.
async fn compact(shared: &Shared, asked: Seq) -> Result<(), AgentError> {
    let thread = {
        let mut state = shared.state.lock();
        if state.asking() || state.turn.is_some() || state.compaction.is_some() {
            return Err(AgentError::Refused("Codex compacts only between turns".into()));
        }
        let thread = state.thread.clone().ok_or(AgentError::Exited)?;
        state.compaction = Some(AskedCompaction { asked, turn: None, answered: false });
        thread
    };
    match shared.rpc.ask("thread/compact/start", json!({"threadId":thread})).await {
        Ok(_) => Ok(()),
        Err(error) if super::rpc::refused(&error) => {
            shared.state.lock().compaction = None;
            Err(error)
        }
        // As for a prompt, a failed write or a missing reply doesn't prove Codex didn't take it, so the process is
        // stopped, and its exit settles the request.
        Err(error) => {
            let message = error.to_string();
            shared
                .event(EventBody::Notice { level: protocol::event::NoticeLevel::Error, message, subagent: None }, None)
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
        if state.turn.is_none() && !state.asking() && state.untaken.is_empty() {
            return Ok(());
        }
        state.cancelling = true;
        (state.thread.clone(), state.turn.clone())
    };
    let mut done = shared.turn_done.subscribe();
    if let (Some(thread), Some(turn)) = (&thread, &turn) {
        let interrupted = shared.rpc.ask("turn/interrupt", json!({"threadId":thread,"turnId":turn})).await;
        // The turn can have ended before Codex read the interrupt, which it then refuses as having no turn going. Its
        // end follows or has come, and is waited for as an accepted interrupt's is. Any other error leaves a turn that
        // may still be going, so the process is stopped.
        if let Err(error) = interrupted
            && !super::rpc::no_active_turn(&error)
        {
            shared.stop().await;
            return Err(error);
        }
        if tokio::time::timeout(super::turn::END_WAIT, done.wait_for(|ended| ended.as_ref() == Some(turn)))
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
    settle(shared, None).await;
    let held = {
        let mut state = shared.state.lock();
        state.turn = None;
        state.cancelling = false;
        state.turn_mode = None;
        state.plan = None;
        state.held_end.take()
    };
    match held {
        Some(Ended { outcome, error, raw }) => notifications::record_end(shared, outcome, error, Some(raw)).await,
        None => notifications::record_end(shared, TurnOutcome::Cancelled, None, None).await,
    }
    Ok(())
}

pub(super) async fn settle(shared: &Shared, parent: Option<&protocol::model::SubagentId>) {
    let (prompts, requests, calls) = {
        let mut state = shared.state.lock();
        // One Codex hasn't started compacting is dropped with the prompts it hasn't taken.
        let untaken_compaction =
            parent.is_none().then(|| state.compaction.take_if(|asked| asked.turn.is_none())).flatten();
        // Child commands can finish after their turn, including after a follow-up begins. Keep their pending
        // calls until that completion, so its genuine output is recorded once under the old invocation.
        let calls = if parent.is_none() {
            state.work.remove(&None).into_iter().flat_map(|work| work.calls.into_keys()).collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        (
            {
                let sending = state.sending;
                let mut prompts: Vec<_> =
                    state.untaken.iter().copied().filter(|seq| parent.is_none() && Some(*seq) != sending).collect();
                state.untaken.retain(|seq| parent.is_some() || Some(*seq) == sending);
                prompts.extend(untaken_compaction.map(|asked| asked.asked));
                prompts
            },
            state
                .waiting
                .extract_if(|_, waiting| waiting.parent.as_ref() == parent)
                .map(|(id, _)| id)
                .collect::<Vec<_>>(),
            calls,
        )
    };
    for prompt in prompts {
        shared.event(EventBody::PromptDropped { prompt }, None).await;
    }
    for approval_id in requests {
        shared.event(EventBody::ApprovalResolved { approval_id, decision: None, by_setting: false }, None).await;
    }
    // SubagentEnded ends a child's unfinished work. ToolCallDropped is a root-only event, used when the
    // agent went on without a call; inventing child results here would claim output the CLI never supplied.
    if parent.is_none() {
        for tool_use_id in calls {
            shared.event(EventBody::ToolCallDropped { tool_use_id }, None).await;
        }
    }
}

/// Reconcile approval changes with the root turn; the hub combines Idle with its background-task list.
pub(super) async fn report_state(shared: &Shared) {
    let state = {
        let state = shared.state.lock();
        if !state.waiting.is_empty() {
            AgentRunState::RequiresAction
        } else if state.turn.is_some() {
            AgentRunState::Running
        } else {
            AgentRunState::Idle
        }
    };
    shared.event(EventBody::AgentState { state }, None).await;
}

/// What a turn is started with for `prompt`: how to talk, if it was spoken, then what it says, then its files.
fn turn_input(prompt: &Prompt<AttachmentFile>) -> Vec<Value> {
    let told = crate::spoken::how_to_talk(prompt).map(|told| json!({"type":"text","text":told,"text_elements":[]}));
    let mut input: Vec<Value> = told.into_iter().collect();
    input.push(json!({"type":"text","text":prompt.text,"text_elements":[]}));
    for file in &prompt.files {
        input.push(if file.attachment.media_type.is_bitmap() {
            json!({"type":"localImage","path":file.path})
        } else {
            json!({"type":"text","text":format!("Attached file: {}",file.path),"text_elements":[]})
        });
    }
    input
}

#[cfg(test)]
mod tests {
    use protocol::prompt::Prompt;

    use super::turn_input;

    #[test]
    fn a_spoken_prompt_starts_its_turn_with_how_to_talk_and_a_typed_one_doesnt() {
        let texts = |prompt: Prompt<_>| -> Vec<String> {
            turn_input(&prompt).iter().map(|block| block["text"].as_str().unwrap().to_string()).collect()
        };

        let spoken = texts(Prompt::new("is it done?").said_out_loud());

        assert!(spoken[0].starts_with("(Roundhouse: the person said this out loud"), "{spoken:?}");
        assert_eq!(spoken[1..], ["is it done?"]);
        assert_eq!(texts(Prompt::new("is it done?")), ["is it done?"]);
    }
}
