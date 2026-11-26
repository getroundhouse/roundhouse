//! A video the file tab shows. A WebM plays in the tab, as one in a reply does. Another, an MP4 or a QuickTime file,
//! which the app doesn't play, or a WebM it can't, says what kind it is, how big and how long, with Open in system
//! player. That opens the file where it is when it's on this machine and named for the kind it is, since the system
//! picks what opens a file by its name. Otherwise it opens a copy, read from the hub a part at a time and named for its
//! kind, of which the tab keeps one at a time, in the system's temporary files.

use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, Global, IntoElement, ParentElement as _, Styled as _, div, px,
};
use protocol::attachment::{MediaType, describe_size};
use std::io::Write as _;
use std::path::Path;
use tempfile::TempPath;
use transcript::{VideoPlayer, VideoSource};
use video::About;

use super::FileView;
use super::viewer::Contents;
use crate::file_image::{Placed, fitted_over};

/// A video the file tab shows.
pub(crate) struct FileVideo {
    media_type: MediaType,
    bytes: u64,
    about: About,
    plays: Plays,
}

/// Where a video plays.
enum Plays {
    /// In the tab.
    Here(Entity<VideoPlayer>),
    /// In the system's player, which Open in system player opens it in.
    Elsewhere(Opening),
}

/// Where Open in system player is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Opening {
    Not,
    /// Reading a copy of it from the hub.
    Copying,
    /// Why it couldn't be opened.
    Failed(String),
}

impl FileVideo {
    /// One the tab plays, of `bytes` bytes, whose still says how big and how long it is.
    pub(super) fn playing(media_type: MediaType, bytes: u64, player: Entity<VideoPlayer>, cx: &App) -> Self {
        let still = player.read(cx).still();
        let about = About { size: Some(still.covers), duration: still.length };
        FileVideo { media_type, bytes, about, plays: Plays::Here(player) }
    }

    /// One the system's player plays, of `bytes` bytes, which says `about` of itself.
    pub(super) fn elsewhere(media_type: MediaType, bytes: u64, about: About) -> Self {
        FileVideo { media_type, bytes, about, plays: Plays::Elsewhere(Opening::Not) }
    }

    /// What kind it is, how big, how long, and its size in bytes, as `MP4 · 1920 × 1080 · 0:12 · 24 MB`, as much as it
    /// says.
    pub(crate) fn describe(&self) -> String {
        let size = self.about.size.map(|(width, height)| format!("{width} × {height}"));
        let length = self.about.duration.map(look::length);
        [Some(kind(self.media_type).to_string()), size, length, Some(describe_size(self.bytes))]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ")
    }

    /// Stops the video playing in the tab where it was, as a new connection does, and says whether it's one that plays
    /// there.
    pub(super) fn stop_for_new_connection(&self, cx: &mut App) -> bool {
        let Plays::Here(player) = &self.plays else { return false };
        player.update(cx, VideoPlayer::stop_for_new_connection);
        true
    }

    /// Whether it plays in the tab.
    #[cfg(any(test, feature = "testing"))]
    pub(crate) fn plays_here(&self) -> bool {
        matches!(self.plays, Plays::Here(_))
    }

    /// What plays it in the tab, if it plays there.
    #[cfg(any(test, feature = "testing"))]
    pub(crate) fn player(&self) -> Option<Entity<VideoPlayer>> {
        let Plays::Here(player) = &self.plays else { return None };
        Some(player.clone())
    }

    /// It, in the middle of the tab, as big as it is, or smaller to fit, with its bar, or what it says and Open in
    /// system player.
    pub(super) fn drawn(&self, cx: &mut Context<FileView>) -> AnyElement {
        let body = match &self.plays {
            Plays::Here(player) => {
                let covers = self.about.size.unwrap_or((1, 1));
                let player = player.clone();
                let room = fitted_over(covers, look::video_bar_height(), Placed::Middle, move |scale, _| {
                    div().w(px(look::pixels(covers.0) * scale).max(look::VIDEO_LEAST_WIDTH)).child(player)
                });
                room.size_full().into_any_element()
            }
            Plays::Elsewhere(opening) => elsewhere(opening, cx),
        };
        div().flex_1().w_full().min_h_0().p(look::Space::XLarge).child(body).into_any_element()
    }
}

/// What a video the system plays says in the tab, and Open in system player.
fn elsewhere(opening: &Opening, cx: &mut Context<FileView>) -> AnyElement {
    let colors = look::colors(cx);
    let said = match opening {
        Opening::Failed(why) => format!("Couldn't open it: {why}"),
        Opening::Not | Opening::Copying => "This video plays in the system's player".to_string(),
    };
    let words = "Open in system player";
    let open = look::button("open-in-system-player", look::ButtonKind::Main, look::ControlSize::Regular, words)
        .waiting(*opening == Opening::Copying)
        .on_click(cx.listener(|view, _, _, cx| view.open_in_system_player(cx)));
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap(look::Space::Medium)
        .child(div().text_color(colors.text_muted).child(said))
        .child(open)
        .into_any_element()
}

impl FileView {
    /// Opens the video it shows in the system's player: where it is, if it's on this machine and named for its kind,
    /// and otherwise a copy, named for its kind, read from the hub first.
    pub(crate) fn open_in_system_player(&mut self, cx: &mut Context<Self>) {
        let (Some(path), Contents::Video(video)) = (self.path.clone(), &mut self.contents) else { return };
        let Plays::Elsewhere(opening) = &mut video.plays else { return };
        if *opening == Opening::Copying {
            return;
        }
        let media_type = video.media_type;
        let here = self.worktree_here.as_ref().filter(|_| self.hub.on_this_machine());
        if let Some(worktree) = here.filter(|_| named_for(&path, media_type)) {
            open_with_system(&worktree.join(path.as_ref()), cx);
            return;
        }
        *opening = Opening::Copying;
        cx.notify();
        let (hub, workspace) = (self.hub.clone(), self.workspace_id.clone());
        let source = VideoSource::Worktree { hub, workspace, path: path.to_string() };
        cx.spawn(async move |this, cx| {
            let copied = cx.background_spawn(async move { copy(&source, media_type) }).await;
            let _ = this.update(cx, |view, cx| {
                // Another file took its place while it was copied. The same file read again, as at a turn's end, is
                // still the one it was copied for.
                if view.path.as_ref() != Some(&path) {
                    return;
                }
                let Contents::Video(FileVideo { plays: Plays::Elsewhere(opening), .. }) = &mut view.contents else {
                    return;
                };
                match copied {
                    Ok(copy) => {
                        *opening = Opening::Not;
                        open_with_system(&copy, cx);
                        // The copy before it goes, so the tab keeps one at a time.
                        view.copy = Some(copy);
                    }
                    Err(why) => *opening = Opening::Failed(why),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

/// What opens a file in another app, as the system picks it. Only the app as it runs has one, since a test's platform
/// can't, so a test or a picture of the app opens nothing.
pub(crate) struct SystemOpens;

impl Global for SystemOpens {}

/// Opens `path` in the app the system picks for it.
fn open_with_system(path: &Path, cx: &mut App) {
    #[cfg(any(test, feature = "testing"))]
    cx.default_global::<Opened>().0.push(path.to_path_buf());
    if cx.has_global::<SystemOpens>() {
        cx.open_with_system(path);
    }
}

/// The files opened in another app, oldest first, for a test to read.
#[cfg(any(test, feature = "testing"))]
#[derive(Default)]
struct Opened(Vec<std::path::PathBuf>);

#[cfg(any(test, feature = "testing"))]
impl Global for Opened {}

/// The files opened in another app so far, oldest first, whether or not there was one to open them.
#[cfg(any(test, feature = "testing"))]
pub fn opened_with_system(cx: &App) -> Vec<std::path::PathBuf> {
    cx.try_global::<Opened>().map_or_default(|opened| opened.0.clone())
}

/// A copy of the video from `source`, named for `media_type`, in the system's temporary files, read a part at a time.
/// It goes when what's returned is dropped.
fn copy(source: &VideoSource, media_type: MediaType) -> Result<TempPath, String> {
    let mut copy = tempfile::Builder::new()
        .prefix("video-")
        .suffix(&format!(".{}", extension(media_type)))
        .tempfile()
        .map_err(|e| e.to_string())?;
    source.copy_to(&mut copy).map_err(|e| e.to_string())?;
    copy.flush().map_err(|e| e.to_string())?;
    Ok(copy.into_temp_path())
}

/// Whether `path` is named for `media_type`, by its extension, so the system opens it as that kind.
fn named_for(path: &str, media_type: MediaType) -> bool {
    let Some((_, named)) = path.rsplit_once('.') else { return false };
    let named = named.to_ascii_lowercase();
    match media_type {
        MediaType::Mp4 => ["mp4", "m4v"].contains(&named.as_str()),
        MediaType::Mov => ["mov", "qt"].contains(&named.as_str()),
        MediaType::Webm => named == "webm",
        _ => false,
    }
}

/// The extension a copy of a video of `media_type` is named with.
fn extension(media_type: MediaType) -> &'static str {
    match media_type {
        MediaType::Mov => "mov",
        MediaType::Webm => "webm",
        _ => "mp4",
    }
}

/// What kind of video it is, as the tab's header says.
fn kind(media_type: MediaType) -> &'static str {
    match media_type {
        MediaType::Webm => "WebM",
        MediaType::Mov => "QuickTime",
        _ => "MP4",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_file_opens_where_it_is_only_when_it_s_named_for_its_kind() {
        assert!(named_for("docs/demo.mp4", MediaType::Mp4));
        assert!(named_for("demo.MOV", MediaType::Mov));
        assert!(!named_for("demo.mov", MediaType::Mp4), "a QuickTime name on an MP4");
        assert!(!named_for("demo.mp4.command", MediaType::Mp4), "a name the system would run");
        assert!(!named_for("demo", MediaType::Mp4));
    }

    #[test]
    fn a_video_says_what_it_can_of_itself() {
        let about = About { size: Some((1920, 1080)), duration: Some(Duration::from_secs(12)) };

        assert_eq!(FileVideo::elsewhere(MediaType::Mp4, 2048, about).describe(), "MP4 · 1920 × 1080 · 0:12 · 2 KB");
        assert_eq!(FileVideo::elsewhere(MediaType::Mov, 2048, About::default()).describe(), "QuickTime · 2 KB");
    }
}
