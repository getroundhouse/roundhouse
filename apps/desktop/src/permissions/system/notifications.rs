//! What a Mac allows of the app's notifications, through `UNUserNotificationCenter`, and having it ask for them. The
//! center aborts an app that isn't run from its bundle, as a build from a checkout isn't, so nothing here touches it
//! then, as GPUI's own notifications don't.

use std::ptr::NonNull;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2_foundation::{NSBundle, NSError};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNAuthorizationStatus, UNNotificationSetting, UNNotificationSettings,
    UNUserNotificationCenter,
};

use super::super::{Badges, Standing};

/// The center, or `None` for an app not run from its bundle.
fn center() -> Option<Retained<UNUserNotificationCenter>> {
    NSBundle::mainBundle().bundleIdentifier()?;
    Some(UNUserNotificationCenter::currentNotificationCenter())
}

/// What's allowed of notifications, and of the Dock's badge with them, or `None` for an app not run from its bundle.
async fn allowed() -> Option<(UNAuthorizationStatus, UNNotificationSetting)> {
    asked_what_is_allowed()?.recv_async().await.ok()
}

/// Has the center say what's allowed, on a thread of its own, as the channel it returns will. Apart from the wait, so
/// nothing of Objective-C's is held across it, which can't go between threads.
fn asked_what_is_allowed() -> Option<flume::Receiver<(UNAuthorizationStatus, UNNotificationSetting)>> {
    let center = center()?;
    let (send, said) = flume::bounded(1);
    let block = RcBlock::new(move |settings: NonNull<UNNotificationSettings>| {
        // SAFETY: the center hands the block settings it keeps alive until the block returns.
        let settings = unsafe { settings.as_ref() };
        let _ = send.try_send((settings.authorizationStatus(), settings.badgeSetting()));
    });
    center.getNotificationSettingsWithCompletionHandler(&block);
    Some(said)
}

/// How notifications stand. Provisional and ephemeral ones are allowed, quietly or for a while.
pub(super) async fn standing() -> Standing {
    match allowed().await {
        Some((UNAuthorizationStatus::NotDetermined, _)) => Standing::NotAsked,
        Some((UNAuthorizationStatus::Denied, _)) => Standing::Off,
        Some(_) => Standing::Allowed,
        None => Standing::Unknown,
    }
}

/// Has the system ask the person for notifications with sounds and the Dock's badge, if it hasn't, and says whether
/// they're allowed. Once notifications are allowed, it grants the badge without asking the person.
pub(super) async fn ask() -> bool {
    let Some(answer) = asked() else { return false };
    answer.recv_async().await.unwrap_or(false)
}

/// Has the center ask, as the channel it returns will say, apart from the wait, as `asked_what_is_allowed` is.
fn asked() -> Option<flume::Receiver<bool>> {
    let center = center()?;
    let (send, said) = flume::bounded(1);
    let block = RcBlock::new(move |granted: Bool, _: *mut NSError| {
        let _ = send.try_send(granted.as_bool());
    });
    let wanted = UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound | UNAuthorizationOptions::Badge;
    center.requestAuthorizationWithOptions_completionHandler(wanted, &block);
    Some(said)
}

/// Asks for the Dock's badge if notifications are allowed and the badge was never asked for, as GPUI's own request
/// leaves it, and says where that leaves it. A badge the person turned off themselves is left off.
pub(super) async fn ask_for_badges() -> Badges {
    let Some((status, badge)) = allowed().await else { return Badges::Settled };
    let allowed = matches!(status, UNAuthorizationStatus::Authorized | UNAuthorizationStatus::Provisional);
    if !allowed {
        Badges::NotYet
    } else if badge != UNNotificationSetting::NotSupported {
        Badges::Settled
    } else if ask().await {
        Badges::Granted
    } else {
        Badges::NotYet
    }
}

/// Where System Settings shows the app's notifications, which it finds by the app's bundle.
pub(super) fn settings_page() -> Option<String> {
    let bundle = NSBundle::mainBundle().bundleIdentifier()?;
    Some(format!("x-apple.systempreferences:com.apple.Notifications-Settings.extension?id={bundle}"))
}
