//! The icons the app draws: the kit's own, and the few more it uses that the kit doesn't bundle by default.

use std::borrow::Cow;

use gpui_kit::{AssetSource, Result, SharedString};

// Only these are embedded, rather than the whole catalog, which would add every icon to the app.
gpui_kit::assets::icon_assets!(
    Extra,
    [
        CornerDownRight,
        FilePen,
        FilePlus,
        ListTree,
        MessageSquarePlus,
        Pencil,
        Plug,
        SquareArrowRight,
        SquareDot,
        SquareMinus,
        SquarePlus,
        Terminal,
        Trash,
        Wrench,
    ]
);

/// What every window loads its icons from.
#[derive(Clone, Copy, Debug, Default)]
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match Extra.load(path)? {
            Some(icon) => Ok(Some(icon)),
            None => gpui_kit::assets::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut found = Extra.list(path)?;
        found.extend(gpui_kit::assets::Assets.list(path)?);
        Ok(found)
    }
}
