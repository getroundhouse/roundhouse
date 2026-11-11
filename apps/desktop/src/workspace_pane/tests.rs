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
fn closing_a_chat_moves_to_the_next_still_open_or_else_the_last() {
    let ids = |names: &[&str]| names.iter().map(|&name| SessionId::from(name)).collect::<Vec<_>>();
    let was_open = ids(&["a", "b", "c", "d", "e"]);

    assert_eq!(neighbor(&was_open, &"c".into(), &ids(&["a", "b", "d", "e"])), Some("d".into()));
    assert_eq!(neighbor(&was_open, &"c".into(), &ids(&["a", "d", "e"])), Some("d".into()), "with the one before");
    assert_eq!(neighbor(&was_open, &"e".into(), &ids(&["a", "b", "c", "d"])), Some("d".into()), "the last");
    assert_eq!(neighbor(&was_open, &"c".into(), &[]), None);
}
