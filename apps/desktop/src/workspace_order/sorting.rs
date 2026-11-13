use protocol::model::Workspace;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;

/// An order to list workspaces in. Saved by its name, as the sidebar's is in the app's settings.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Sort {
    /// The oldest first, as the hub lists them.
    Oldest,
    /// The newest first.
    Newest,
    /// By name, from A, in any case.
    Name,
    /// The one last worked in first, as the hub says with [`Workspace::last_active_at_ms`]. What the sidebar shows
    /// unless another is chosen, and an order this build doesn't know, saved by a newer one, reads as this, which is
    /// why it's last.
    #[default]
    #[serde(other)]
    LastActive,
}

impl Sort {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Sort::Oldest => "Oldest first",
            Sort::Newest => "Newest first",
            Sort::LastActive => "Last active",
            Sort::Name => "Name",
        }
    }

    /// Puts `listed`, which are places in `workspaces`, in this order. Those that come out the same keep the order
    /// they were in.
    pub(crate) fn sort(self, listed: &mut [usize], workspaces: &[Workspace]) {
        match self {
            Sort::Oldest => listed.sort_by_key(|&i| workspaces[i].created_at_ms),
            Sort::Newest => listed.sort_by_key(|&i| Reverse(workspaces[i].created_at_ms)),
            Sort::LastActive => listed.sort_by_key(|&i| Reverse(workspaces[i].last_active_at_ms())),
            Sort::Name => listed.sort_by_cached_key(|&i| workspaces[i].label().to_lowercase()),
        }
    }
}
