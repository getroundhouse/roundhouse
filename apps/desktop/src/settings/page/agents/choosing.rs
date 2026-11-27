//! A pending request is local UI state; the selected account comes only from the hub record.

use gpui_kit::{Context, Task};
use protocol::client::accounts::{AccountSetAutomatic, AccountSetDefault};
use protocol::model::{AccountId, AgentKind};

use super::kept::Kept;

#[derive(Default)]
pub(super) struct Choosing {
    pub(super) pending: Option<Task<()>>,
    pub(super) error: Option<String>,
}

/// What the person chose for a kind's chats on none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Choice {
    /// The account, as the kind's default.
    Account(AccountId),
    /// Automatic, which puts each on the account with the most of its week to spare.
    Automatic,
}

impl Kept {
    pub(super) fn choose_default(&mut self, agent: AgentKind, choice: Choice, cx: &mut Context<Self>) {
        let Some(asking) = self.asking.clone() else { return };
        if self.choosing.pending.is_some() {
            return;
        }
        self.choosing.error = None;
        let choosing = match choice {
            Choice::Account(account) => asking.hub.ask(AccountSetDefault { agent, account }),
            Choice::Automatic => asking.hub.ask(AccountSetAutomatic { agent }),
        };
        self.choosing.pending = Some(cx.spawn(async move |this, cx| {
            let answer = choosing.recv().await;
            let _ = this.update(cx, |this, cx| {
                this.choosing.pending = None;
                if let Err(e) = answer {
                    this.choosing.error = Some(format!("Couldn't change the default: {e}"));
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}
