//! What's typed and not sent, since the floor. Each has its place in `MIGRATIONS`.

/// When what's in each session's composer was typed, in milliseconds by the clock of the device it was typed on, so
/// the hub keeps whichever change was typed last, as `SessionDraft` says. It's apart from the draft, which is NULL when
/// nothing's typed, since a composer emptied by sending was typed then too. NULL for a draft kept before it, which any
/// change replaces. No record shows it, so the trigger doesn't name it, and a time that moves alone moves no revision.
pub(super) const DRAFT_TYPED_AT: &str = r"
    ALTER TABLE sessions ADD COLUMN draft_typed_at_ms INTEGER;
";
