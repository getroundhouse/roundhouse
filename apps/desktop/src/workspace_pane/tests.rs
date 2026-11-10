use protocol::model::SessionId;

use super::chats::{inherited_mode, neighbor};

#[test]
fn a_new_chat_doesnt_inherit_plan_or_full_access() {
    assert_eq!(inherited_mode(Some("plan")), None);
    assert_eq!(inherited_mode(Some("bypassPermissions")), None);
    assert_eq!(inherited_mode(Some("acceptEdits")).as_deref(), Some("acceptEdits"));
    assert_eq!(inherited_mode(None), None);
}

#[test]
fn closing_a_chat_moves_to_the_next_or_else_the_last() {
    let open: Vec<SessionId> = vec!["a".into(), "c".into()];
    assert_eq!(neighbor(&open, 1), Some("c".into()));
    assert_eq!(neighbor(&open, 2), Some("c".into()));
    assert_eq!(neighbor(&[], 0), None);
}
