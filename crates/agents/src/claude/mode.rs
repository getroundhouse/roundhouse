//! What the CLI calls each approval mode: what `--permission-mode` and `set_permission_mode` take, and what `init` and
//! `status` report. `docs/claude-cli.md` has how it behaves.

use protocol::model::PermissionMode;

/// Full access.
pub(super) const FULL_ACCESS: &str = "bypassPermissions";
const PLAN: &str = "plan";

/// `None` for a mode there's no name for here.
pub(super) fn name(mode: PermissionMode) -> Option<&'static str> {
    match mode {
        PermissionMode::FullAccess => Some(FULL_ACCESS),
        PermissionMode::Plan => Some(PLAN),
        PermissionMode::Unknown => None,
    }
}

/// The mode the CLI calls `name`. Any but full access and plan mode is one a chat can't have, such as `default`, which
/// asks before every tool, so it's unknown.
pub(super) fn mode(name: &str) -> PermissionMode {
    match name {
        FULL_ACCESS => PermissionMode::FullAccess,
        PLAN => PermissionMode::Plan,
        _ => PermissionMode::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_mode_a_chat_can_have_is_called_by_its_own_name() {
        for chat_mode in [PermissionMode::FullAccess, PermissionMode::Plan] {
            assert_eq!(name(chat_mode).map(mode), Some(chat_mode));
        }
        assert_eq!(name(PermissionMode::Unknown), None);
    }

    #[test]
    fn the_clis_other_modes_are_unknown() {
        for other in ["default", "acceptEdits", "dontAsk", "auto", ""] {
            assert_eq!(mode(other), PermissionMode::Unknown, "{other}");
        }
    }
}
