use protocol::model::SessionId;

/// The session every test here drafts in, which `store_with_session` makes.
pub(super) fn id() -> SessionId {
    "s".into()
}
