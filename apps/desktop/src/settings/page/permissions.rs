//! Permissions, the page that shows how each system permission the app uses stands, and fixes one that isn't
//! allowed, as `docs/spec/settings.md` says. Only where `HAS_PERMISSIONS`, so elsewhere there's no page rather than an
//! empty one.

use gpui_kit::assets::IconName as Lucide;

use super::{Body, Control, Page, PageId, Setting};
use crate::permissions::{HAS_PERMISSIONS, Permission};

pub(super) fn page() -> Option<Page> {
    if !HAS_PERMISSIONS {
        return None;
    }
    let settings = Permission::ALL.into_iter().map(|permission| Setting {
        name: permission.name(),
        does: permission.does(),
        control: Control::Permission(permission),
        status_box: None,
    });
    let body = Body::Settings(settings.collect());
    Some(Page { id: PageId::Permissions, name: "Permissions", icon: Lucide::ShieldCheck, body })
}
