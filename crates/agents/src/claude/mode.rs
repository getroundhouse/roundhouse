//! What the CLI calls each approval mode: what `--permission-mode` and `set_permission_mode` take, and what `init` and
//! `status` report. `docs/claude-cli.md` has how it behaves.

use protocol::capability::approvals::ApprovalMode;

use super::claims;

/// Full access.
pub(super) const FULL_ACCESS: &str = "bypassPermissions";
const PLAN: &str = "plan";

/// What the CLI calls `mode`, one this adapter claims. `None` for any other, which the CLI couldn't be asked into.
pub(super) fn name(mode: ApprovalMode) -> Option<&'static str> {
    if mode == claims::full_access() {
        Some(FULL_ACCESS)
    } else if mode == claims::plan() {
        Some(PLAN)
    } else {
        None
    }
}

/// The mode the CLI calls `name`. Any but full access and plan mode is one a chat can't have, such as `default`, which
/// asks before every tool, so it's unknown.
pub(super) fn mode(name: &str) -> ApprovalMode {
    match name {
        FULL_ACCESS => claims::full_access(),
        PLAN => claims::plan(),
        _ => ApprovalMode::UNKNOWN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_mode_a_chat_can_have_is_called_by_its_own_name() {
        for chat_mode in [claims::full_access(), claims::plan()] {
            assert_eq!(name(chat_mode).map(mode), Some(chat_mode));
        }
        let asks_first =
            ApprovalMode { unasked: protocol::capability::approvals::Unasked::Reads, ..claims::full_access() };
        assert_eq!(name(asks_first), None, "one it doesn't claim");
    }

    #[test]
    fn the_clis_other_modes_are_unknown() {
        for other in ["default", "acceptEdits", "dontAsk", "auto", ""] {
            assert_eq!(mode(other), ApprovalMode::UNKNOWN, "{other}");
        }
    }
}
