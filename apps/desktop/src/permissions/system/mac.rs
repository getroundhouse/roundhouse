//! What a Mac says of the app's permissions, and having it ask the person. A permission a Mac doesn't ask for with a
//! dialog lists the app in System Settings, switched off, once the app has tried what it guards, so the app tries it
//! before opening the place there, where the person then only switches it on.

use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::Path;

use gpui_kit::{App, AppContext as _, Task};
use objc2_core_graphics::{CGPreflightScreenCaptureAccess, CGRequestScreenCaptureAccess};
use objc2_foundation::NSHomeDirectory;
use sound::Permission as Said;

use super::super::{Badges, Permission, Standing};
use super::notifications;

/// Folders in the home folder that only Full Disk Access opens, which a Mac refuses without a dialog, listing the app
/// under Full Disk Access as it does: Safari's, then Mail's, for a Mac whose Safari has never made its own. Both were
/// seen refused so. Some others it guards, such as its own records of what's allowed, it hides instead, as if they
/// weren't there, which can't tell refused from missing.
const ONLY_FULL_DISK_ACCESS_OPENS: [&str; 2] = ["Library/Safari", "Library/Mail"];

/// Where System Settings lists the apps allowed what's guarded, by the anchor of each permission's list.
const PRIVACY: &str = "x-apple.systempreferences:com.apple.preference.security?";

pub(in crate::permissions) fn read(cx: &mut App) -> Task<HashMap<Permission, Standing>> {
    cx.background_spawn(async move {
        let notifications = notifications::standing().await;
        Permission::ALL
            .into_iter()
            .map(|permission| {
                let standing = match permission {
                    Permission::Notifications => notifications,
                    Permission::FullDiskAccess => full_disk_access(),
                    Permission::ScreenRecording => screen_recording(),
                    Permission::Microphone => from(sound::microphone_permission()),
                    Permission::InputMonitoring => from(dictation_ui::input_monitoring()),
                };
                (permission, standing)
            })
            .collect()
    })
}

pub(in crate::permissions) fn ask(permission: Permission, cx: &mut App) -> Task<()> {
    match permission {
        Permission::Notifications => cx.background_spawn(async {
            notifications::ask().await;
        }),
        Permission::Microphone => {
            let (send, answered) = flume::bounded(1);
            sound::ask_for_microphone(move |_| {
                let _ = send.try_send(());
            });
            cx.background_spawn(async move {
                let _ = answered.recv_async().await;
            })
        }
        // The answer comes from System Settings, where the app is now listed, and is read as the window comes in
        // front again.
        Permission::InputMonitoring => {
            dictation_ui::ask_to_monitor_input();
            Task::ready(())
        }
        // Never not asked, as far as a Mac says, so it's only ever opened in System Settings.
        Permission::FullDiskAccess | Permission::ScreenRecording => {
            open_settings(permission, cx);
            Task::ready(())
        }
    }
}

pub(in crate::permissions) fn open_settings(permission: Permission, cx: &mut App) {
    let page = match permission {
        Permission::Notifications => notifications::settings_page(),
        Permission::FullDiskAccess => {
            // Tried, so the app is listed to switch on.
            full_disk_access();
            Some(format!("{PRIVACY}Privacy_AllFiles"))
        }
        Permission::ScreenRecording => {
            // Lists the app, and the first time has the system ask too, which offers the same place.
            let _ = CGRequestScreenCaptureAccess();
            Some(format!("{PRIVACY}Privacy_ScreenCapture"))
        }
        Permission::Microphone => Some(format!("{PRIVACY}Privacy_Microphone")),
        Permission::InputMonitoring => Some(format!("{PRIVACY}Privacy_ListenEvent")),
    };
    if let Some(page) = page {
        cx.open_url(&page);
    }
}

pub(in crate::permissions) fn ask_for_badges(cx: &mut App) -> Task<Badges> {
    cx.background_spawn(notifications::ask_for_badges())
}

/// Whether the first of the folders only Full Disk Access opens that's there can be read. A refusal is the system's,
/// and a Mac with none of them leaves it unknown.
fn full_disk_access() -> Standing {
    let home = NSHomeDirectory().to_string();
    for folder in ONLY_FULL_DISK_ACCESS_OPENS {
        match std::fs::read_dir(Path::new(&home).join(folder)) {
            Ok(_) => return Standing::Allowed,
            Err(e) if e.kind() == ErrorKind::PermissionDenied => return Standing::Off,
            Err(_) => {}
        }
    }
    Standing::Unknown
}

/// A Mac says only whether the screen may be recorded, so one never asked reads as off.
fn screen_recording() -> Standing {
    if CGPreflightScreenCaptureAccess() { Standing::Allowed } else { Standing::Off }
}

fn from(said: Said) -> Standing {
    match said {
        Said::Allowed => Standing::Allowed,
        Said::NotAsked => Standing::NotAsked,
        Said::Denied => Standing::Off,
    }
}
