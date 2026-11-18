//! The icons the app draws: the kit's own, and the few more it uses that the kit doesn't bundle by default.

use std::borrow::Cow;

use gpui_kit::{AssetSource, Result, SharedString};

// Only these are embedded, rather than the whole catalog, which would add every icon to the app.
gpui_kit::assets::icon_assets!(
    Extra,
    [
        Activity,
        Archive,
        ArchiveRestore,
        ArrowUpDown,
        ArrowUpRight,
        Bell,
        Brain,
        ChevronsUpDown,
        CircleCheck,
        CircleFadingArrowUp,
        CircleQuestionMark,
        CodeXml,
        Columns2,
        CornerDownRight,
        Download,
        FilePen,
        FilePlus,
        Files,
        FoldVertical,
        GitBranch,
        GitPullRequest,
        HardDrive,
        House,
        ImagePlus,
        KeyRound,
        Laptop,
        Lightbulb,
        ListTree,
        LoaderCircle,
        Mail,
        MailOpen,
        MessageCircleQuestionMark,
        MessageSquare,
        MessageSquarePlus,
        Mic,
        NotebookPen,
        Paperclip,
        Pencil,
        PencilSparkles,
        Plug,
        QrCode,
        RotateCcw,
        RotateCcwClock,
        ScrollText,
        Server,
        SquareArrowRight,
        SquareDot,
        SquareMinus,
        SquarePlus,
        Terminal,
        TextAlignStart,
        Trash,
        TriangleAlert,
        Wrench,
    ]
);

/// What every window loads its icons from.
#[derive(Clone, Copy, Debug, Default)]
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(icon) = look::file_icon_asset(path) {
            return Ok(Some(Cow::Borrowed(icon)));
        }
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
