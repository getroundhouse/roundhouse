//! The icons the app draws: the few more of its own that the kit doesn't bundle by default, in front of those of what
//! every client shares, which `look::Assets` has.

use std::borrow::Cow;

use gpui_kit::{AssetSource, Result, SharedString};

// Only these are embedded, rather than the whole catalog, which would add every icon to the app.
gpui_kit::assets::icon_assets!(
    Extra,
    [
        ArchiveRestore,
        ArrowUpDown,
        CircleDashed,
        CircleQuestionMark,
        CodeXml,
        Columns2,
        Download,
        House,
        Keyboard,
        KeyRound,
        Laptop,
        ListTree,
        MessageSquarePlus,
        Paperclip,
        QrCode,
        RotateCcwClock,
        ScrollText,
        Server,
        ShieldCheck,
        SquareArrowRight,
        SquareDot,
        SquareMinus,
        SquarePlus,
        TextAlignStart,
    ]
);

/// What every window loads its icons from.
#[derive(Clone, Copy, Debug, Default)]
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match Extra.load(path)? {
            Some(icon) => Ok(Some(icon)),
            None => look::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut found = Extra.list(path)?;
        found.extend(look::Assets.list(path)?);
        Ok(found)
    }
}
