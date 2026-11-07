use super::chats::{inherited_mode, neighbor};
use super::view::{MODELS, model_label};

#[test]
fn a_new_chat_doesnt_inherit_plan_or_full_access() {
    assert_eq!(inherited_mode(Some("plan")), None);
    assert_eq!(inherited_mode(Some("bypassPermissions")), None);
    assert_eq!(inherited_mode(Some("acceptEdits")).as_deref(), Some("acceptEdits"));
    assert_eq!(inherited_mode(None), None);
}

#[test]
fn closing_a_chat_moves_to_the_next_or_else_the_last() {
    let open: Vec<String> = vec!["a".into(), "c".into()];
    assert_eq!(neighbor(&open, 1).as_deref(), Some("c"));
    assert_eq!(neighbor(&open, 2).as_deref(), Some("c"));
    assert_eq!(neighbor(&[], 0), None);
}

#[test]
fn a_model_with_no_name_here_still_shows() {
    assert_eq!(model_label(None), MODELS[0].1);
    assert_eq!(model_label(Some("opus")), "Opus");
    assert_eq!(model_label(Some("claude-something-new")), "claude-something-new");
}
