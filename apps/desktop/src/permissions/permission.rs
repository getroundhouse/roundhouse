//! Each system permission the app uses, what it makes work, and how one can stand.

/// Whether this platform asks the person for permissions the app shows, as only a Mac does, so elsewhere Settings has
/// no Permissions page rather than an empty one.
pub(crate) const HAS_PERMISSIONS: bool = cfg!(target_os = "macos");

/// A permission the app uses, in the order the page lists them: what tells the person, what agents need, then what
/// dictating needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Permission {
    /// Notifications, with sounds and the Dock's badge.
    Notifications,
    /// Every folder macOS guards, such as Desktop, Documents and Downloads, which agents open.
    FullDiskAccess,
    /// Recording the screen, which an agent's screenshot needs.
    ScreenRecording,
    Microphone,
    /// Seeing keys pressed while another app is in front, which holding Space to dictate needs.
    InputMonitoring,
}

/// How a permission stands, as the system says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standing {
    Allowed,
    /// The system hasn't asked the person yet, and asks once the app does.
    NotAsked,
    /// Not allowed, which only System Settings changes: refused, or one the system never asks for, such as Full Disk
    /// Access.
    Off,
    /// The system won't say, as it won't of notifications to a build run from a checkout, which shows none.
    Unknown,
}

/// Where asking a Mac for the Dock's badge leaves it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Badges {
    /// Just granted, so the badge shows from now on.
    Granted,
    /// Nothing more to ask: they show already, the person turned them off, or the app isn't run from its bundle, so
    /// it shows no notifications at all.
    Settled,
    /// Notifications aren't allowed yet, so the badge can't be asked for yet.
    NotYet,
}

impl Permission {
    pub(crate) const ALL: [Permission; 5] = [
        Permission::Notifications,
        Permission::FullDiskAccess,
        Permission::ScreenRecording,
        Permission::Microphone,
        Permission::InputMonitoring,
    ];

    /// What System Settings calls it.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Permission::Notifications => "Notifications",
            Permission::FullDiskAccess => "Full Disk Access",
            Permission::ScreenRecording => "Screen Recording",
            Permission::Microphone => "Microphone",
            Permission::InputMonitoring => "Input Monitoring",
        }
    }

    /// What it makes work.
    pub(crate) fn does(self) -> &'static str {
        match self {
            Permission::Notifications => {
                "Tells you when an agent needs you, and counts the workspaces that do on the Dock's icon."
            }
            Permission::FullDiskAccess => {
                "Lets agents open any folder on this Mac without a dialog for each, such as Desktop, Documents and \
                 Downloads."
            }
            Permission::ScreenRecording => "Lets agents take screenshots, such as to check what they've built.",
            Permission::Microphone => "Hears what you dictate, on this Mac.",
            Permission::InputMonitoring => {
                "Keeps dictating while you hold Space, even if something else comes in front for a moment."
            }
        }
    }

    /// What its row's parts are known by, for tests and a screen reader.
    pub(crate) fn id(self) -> &'static str {
        match self {
            Permission::Notifications => "notifications",
            Permission::FullDiskAccess => "full-disk-access",
            Permission::ScreenRecording => "screen-recording",
            Permission::Microphone => "microphone",
            Permission::InputMonitoring => "input-monitoring",
        }
    }
}
