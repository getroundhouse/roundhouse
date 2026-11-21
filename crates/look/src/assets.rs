//! What a window loads the icons of what every client shares from: the kit's own, the icons of kinds of file, and the
//! few more that look's components, the transcript and `client-ui` draw and the kit doesn't bundle by default. An app
//! puts its own in front, and `tools/policy` checks that every icon named is embedded where its window loads it.

use std::borrow::Cow;

use gpui_kit::{AssetSource, Result, SharedString};

use crate::file_icon_asset;

// Only these are embedded, rather than the whole catalog, which would add every icon to the app.
gpui_kit::assets::icon_assets!(
    Icons,
    [
        Activity,
        Archive,
        ArrowUpRight,
        AudioLines,
        Brain,
        CircleDot,
        CircleFadingArrowUp,
        CircleSlash,
        CornerDownRight,
        FilePen,
        FilePlus,
        Files,
        FoldVertical,
        GitBranch,
        GitMerge,
        GitPullRequest,
        GitPullRequestClosed,
        GitPullRequestDraft,
        ImagePlus,
        Lightbulb,
        Link,
        Mail,
        MailOpen,
        MessageCircleQuestionMark,
        MessageSquare,
        NotebookPen,
        Pencil,
        PencilSparkles,
        Plug,
        Split,
        Terminal,
        Trash,
        Volume2,
        VolumeX,
        Wrench,
    ]
);

/// The icons of what every client shares, after an app's own.
#[derive(Clone, Copy, Debug, Default)]
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(icon) = file_icon_asset(path) {
            return Ok(Some(Cow::Borrowed(icon)));
        }
        match Icons.load(path)? {
            Some(icon) => Ok(Some(icon)),
            None => gpui_kit::assets::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut found = Icons.list(path)?;
        found.extend(gpui_kit::assets::Assets.list(path)?);
        Ok(found)
    }
}
