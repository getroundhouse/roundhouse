//! Chats put on accounts automatically: on the one with usage to spare, and, with none to spare, stopped until the
//! person puts the chat on one.

use protocol::client::accounts::{AccountSetAutomatic, AccountSetReserve};
use protocol::model::{AccountId, AgentKind, Reserve};

use super::two_accounts::{Chats, TwoAccounts};
use crate::logins::NO_ACCOUNT_TO_SPARE;

async fn reserve_all(chats: &Chats, account: &AccountId) {
    let asked = AccountSetReserve { account: account.clone(), reserve: Some(Reserve::ALL) };
    chats.hub.account_set_reserve(asked).await.unwrap();
}

#[tokio::test]
async fn a_new_chat_goes_on_the_account_with_usage_to_spare_and_with_none_stops_until_its_put_on_one() {
    let dir = tempfile::tempdir().unwrap();
    let accounts = TwoAccounts::start(dir.path()).await;
    let (chats, work, personal) = (&accounts.chats, &accounts.work.id, &accounts.personal.id);
    reserve_all(chats, work).await;
    chats.hub.account_set_automatic(AccountSetAutomatic { agent: AgentKind::Claude }).await.unwrap();
    let (placed, stopped) = (accounts.another_chat().await, accounts.another_chat().await);

    let placed_under = chats.token_in_turn(&placed, None).await;
    reserve_all(chats, personal).await;
    let failed = chats.refused_launch(&stopped).await;
    let needing = chats.record(&stopped);
    let moved = chats.assign(&stopped, work, false).await.unwrap();
    let stopped_under = chats.token_in_turn(&stopped, None).await;

    assert_eq!(placed_under.as_deref(), Some("personal-token"), "all of Work is reserved");
    assert!(chats.record(&placed).placed_automatically);
    assert!(needing.needs_an_account && needing.hub_account().is_none());
    assert_eq!(needing.status, protocol::model::SessionStatus::Interrupted, "as any launch that fails leaves it");
    assert!(failed.contains(NO_ACCOUNT_TO_SPARE), "{failed}");
    assert!(!moved.needs_an_account && moved.hub_account() == Some(work));
    assert_eq!(stopped_under.as_deref(), Some("work-token"), "a chat put on an account spends what it reserves");
    assert!(!chats.record(&stopped).placed_automatically, "the person chose it");
}
