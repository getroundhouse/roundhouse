//! The icons the app draws: the one of its own that the kit doesn't bundle by default, in front of those of what every
//! client shares, which `look::Assets` has.

use std::borrow::Cow;

use gpui_kit::{AssetSource, Result, SharedString};

// Only these are embedded, rather than the whole catalog, which would add every icon to the app: the hub's button, and
// New chat in a chat's.
gpui_kit::assets::icon_assets!(Own, [Server, MessageSquarePlus]);

/// What the window loads its icons from.
#[derive(Clone, Copy, Debug, Default)]
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match Own.load(path)? {
            Some(icon) => Ok(Some(icon)),
            None => look::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut found = Own.list(path)?;
        found.extend(look::Assets.list(path)?);
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::assets::IconName;

    use super::*;

    /// An icon that isn't there draws nothing and says nothing, so each place one comes from is tried: the app's own,
    /// look's, as the transcript draws, and the kit's.
    #[test]
    fn has_the_icons_of_the_app_what_every_client_shares_and_the_kit() {
        for icon in
            [IconName::Server, IconName::Archive, IconName::Wrench, IconName::TriangleAlert, IconName::ChevronLeft]
        {
            let path = icon.path();
            assert!(Assets.load(&path).unwrap().is_some(), "{path}");
        }
    }
}
