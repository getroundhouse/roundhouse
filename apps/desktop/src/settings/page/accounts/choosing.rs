//! A pending request is local UI state; the selected account comes only from the hub record.

use gpui_kit::{Context, Task};
use protocol::model::{AccountId, AgentKind, Record};

use super::kept::Kept;

#[derive(Default)]
pub(super) struct Choosing {
    pub(super) pending: Option<Task<()>>,
    pub(super) error: Option<String>,
}

impl Kept {
    pub(super) fn choose_default(&mut self, agent: AgentKind, account: AccountId, cx: &mut Context<Self>) {
        let Some(asking) = self.asking.clone() else { return };
        if self.choosing.pending.is_some() {
            return;
        }
        self.choosing.error = None;
        let choosing = asking.client.start(move |client| client.account_set_default(agent, &account));
        self.choosing.pending = Some(cx.spawn(async move |this, cx| {
            let answer = choosing.recv().await;
            let _ = this.update(cx, |this, cx| {
                this.choosing.pending = None;
                match answer {
                    Ok(settings) => asking.feed.take_reply(Record::HubSettings(settings)),
                    Err(e) => this.choosing.error = Some(format!("Couldn't change the default: {e}")),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}
