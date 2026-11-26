use tool_sign_in::ToolSignIn;

use super::check_calls::Checks;
use super::in_doubt::InDoubt;
use super::renewing::Renewing;
use super::waiting::Waiting;

/// Signing in to tool servers, renewing that, and checking them, in the hub's memory.
pub(in super::super) struct ToolSignIns {
    /// What the hub asks tool servers and their authorization servers as.
    pub(super) client: ToolSignIn,
    /// Those that aren't finished.
    pub(super) waiting: Waiting,
    pub(super) renewing: Renewing,
    pub(super) in_doubt: InDoubt,
    pub(super) checks: Checks,
}

impl ToolSignIns {
    pub(in super::super) fn new(client: ToolSignIn) -> Self {
        Self {
            client,
            waiting: Waiting::default(),
            renewing: Renewing::default(),
            in_doubt: InDoubt::default(),
            checks: Checks::default(),
        }
    }
}
