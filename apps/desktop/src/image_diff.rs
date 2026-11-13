//! A changed file that isn't text, as the workspace's file tab shows it in place of the lines it hasn't got: an image as
//! it was and as it is, side by side, one side at a time in one place, flipped between, or as what changed between
//! them, and any other file saying why it can't be shown. The diff says which sides there are, and the hub reads each:
//! the one before from the commit the diff was taken from, and the one after from the worktree. Which file is the file
//! diff's to say.

use std::rc::Rc;

use client_core::ClientError;
use gpui_kit::{App, AppContext as _, Context, Entity, Global, SharedString, Subscription, Task};
use protocol::model::{CommitId, FileContents};

use crate::file_image::{self, FileImage};
use crate::file_view::{Read, Unshown, hash, what_it_gives};
use crate::review::{Review, ReviewEvent};
use difference::Difference;

mod difference;
mod drawing;
mod stacked;

/// The key context the keys of a changed image are bound in, which the file diff has only while it shows a file that
/// isn't text.
pub const KEY_CONTEXT: &str = "ImageDiff";

/// How the two sides of a changed image are shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Comparison {
    #[default]
    SideBySide,
    /// One side at a time, in one place, flipped between, so what changed jumps out as it flips. It's chosen as the
    /// side to show, Before or After.
    Flip,
    /// The image after, faded, with what changed from the one before tinted a colour of its own, and an outline around
    /// each place that changed.
    Difference,
}

/// Which side [`Comparison::Flip`] shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shown {
    Before,
    #[default]
    After,
}

/// How far comparing the two images shown has got.
#[derive(Clone, Copy)]
enum Outcome<'a> {
    Comparing,
    /// Either couldn't be decoded.
    Failed,
    Made(&'a Rc<Difference>),
}

/// What two images were compared to make, by the hashes of their bytes, before and after.
struct Compared {
    of: (u64, u64),
    /// `None` when they couldn't be compared.
    difference: Option<Rc<Difference>>,
}

/// The comparison chosen, which every workspace shares until the app quits, as the layout of the changes is.
struct Chosen(Comparison);

impl Global for Chosen {}

pub fn comparison(cx: &App) -> Comparison {
    cx.try_global::<Chosen>().map_or_default(|chosen| chosen.0)
}

fn choose(comparison: Comparison, cx: &mut App) {
    cx.set_global(Chosen(comparison));
}

/// One side of the change.
#[derive(Clone)]
enum Side {
    /// Being read for the first time.
    Loading,
    /// The change hasn't got it: the side before, for a file it adds, or after, for one it removes.
    Absent,
    Image(FileImage),
    /// It can't be drawn, and this says why.
    Other(SharedString),
}

impl Side {
    /// The hash of the image it shows, if it shows one.
    fn hash(&self) -> Option<u64> {
        let Side::Image(image) = self else { return None };
        Some(image.hash)
    }

    /// Shows `next` in its place, letting the image it showed go, if it showed one.
    fn set(&mut self, next: Side, cx: &mut App) {
        if let Side::Image(image) = std::mem::replace(self, next) {
            image.release(cx);
        }
    }
}

pub struct ImageDiff {
    review: Entity<Review>,
    /// The file it shows, by the path the diff gives it, while the file tab shows one.
    path: Option<SharedString>,
    before: Side,
    after: Side,
    /// The commit and the path the image before was read from, since what a commit holds can't change, so it isn't
    /// read again while they're the same.
    before_from: Option<(CommitId, String)>,
    /// How many times the sides have been read, so only the newest answer is shown.
    reads: u64,
    shown: Shown,
    /// What the two images were last compared to make, which is only done once the comparison is chosen.
    compared: Option<Compared>,
    /// The two being compared, while they are, and the comparing, which stops when it's dropped.
    comparing: Option<((u64, u64), Task<()>)>,
    _subscriptions: [Subscription; 3],
}

impl ImageDiff {
    pub fn new(review: Entity<Review>, cx: &mut Context<Self>) -> Self {
        // The diff is loaded again when what it changes may have, mostly as a turn ends.
        let loading = cx.subscribe(&review, |t, _, event: &ReviewEvent, cx| {
            if matches!(event, ReviewEvent::Loaded) {
                t.read(cx);
            }
        });
        // Chosen here or in another workspace, which this one shows too. Comparing waits until it's drawn, so a
        // workspace that isn't shown doesn't.
        let choosing = cx.observe_global::<Chosen>(|_, cx| cx.notify());
        let releasing = cx.on_release(|t, cx| {
            t.forget_difference(cx);
            t.before.set(Side::Absent, cx);
            t.after.set(Side::Absent, cx);
        });
        let (before, after, shown) = (Side::Loading, Side::Loading, Shown::default());
        let subscriptions = [loading, choosing, releasing];
        Self {
            review,
            path: None,
            before,
            after,
            before_from: None,
            reads: 0,
            shown,
            compared: None,
            comparing: None,
            _subscriptions: subscriptions,
        }
    }

    /// Shows a file, in place of the one it showed, or none, as the file diff says. Only a file the diff says isn't
    /// text is read.
    pub fn show(&mut self, path: Option<SharedString>, cx: &mut Context<Self>) {
        if self.path == path {
            return;
        }
        self.path = path;
        // So what's being read of the file before is dropped when it comes.
        self.reads += 1;
        self.before.set(Side::Loading, cx);
        self.after.set(Side::Loading, cx);
        (self.before_from, self.shown) = (None, Shown::default());
        self.forget_difference(cx);
        self.read(cx);
        cx.notify();
    }

    /// The two images, while both sides are one.
    fn images(&self) -> Option<(&FileImage, &FileImage)> {
        let (Side::Image(before), Side::Image(after)) = (&self.before, &self.after) else { return None };
        Some((before, after))
    }

    /// Shows the other side, while one image is flipped between two.
    pub fn flip(&mut self, cx: &mut Context<Self>) {
        let next = match self.shown {
            Shown::Before => Shown::After,
            Shown::After => Shown::Before,
        };
        self.show_side(next, cx);
    }

    fn show_side(&mut self, shown: Shown, cx: &mut Context<Self>) {
        if self.images().is_some() && comparison(cx) == Comparison::Flip {
            self.shown = shown;
            cx.notify();
        }
    }

    /// Which side it shows while it's flipped between them.
    #[cfg(feature = "testing")]
    pub fn shown(&self) -> Shown {
        self.shown
    }

    /// What the strip under its bar says of what it shows, part by part: each side, by its name and then what it is,
    /// and how much changed, in Difference. Empty when there's no strip.
    #[cfg(feature = "testing")]
    pub fn said_in_strip(&self, cx: &App) -> Vec<String> {
        let Some(told) = self.told_now(cx) else { return Vec::new() };
        let sides = told.sides.into_iter().map(|(name, about)| match about {
            Some(about) => format!("{name} {about}"),
            None => name.to_string(),
        });
        sides.chain(told.changed).collect()
    }

    /// What it says changed between the two images, once they've been compared.
    #[cfg(feature = "testing")]
    pub fn difference(&self) -> Option<String> {
        let compared = self.compared.as_ref()?;
        Some(compared.difference.as_ref().map_or_default(|difference| difference.describe()))
    }

    /// The pictures it has to draw: each side's image, and what changed, once that's known.
    #[cfg(feature = "testing")]
    pub fn pictures(&self) -> Vec<std::sync::Arc<gpui_kit::RenderImage>> {
        let sides = [&self.before, &self.after].into_iter().filter_map(|side| match side {
            Side::Image(image) => Some(image.picture()),
            Side::Loading | Side::Absent | Side::Other(_) => None,
        });
        let changed = self.compared.as_ref().and_then(|compared| compared.difference.as_ref());
        sides.chain(changed.map(|difference| difference.picture.clone())).collect()
    }

    /// What changed between the two images it shows, so far.
    fn difference_shown(&self) -> Outcome<'_> {
        let Some((before, after)) = self.images() else { return Outcome::Comparing };
        let compared = self.compared.as_ref().filter(|compared| compared.of == (before.hash, after.hash));
        match compared.map(|compared| compared.difference.as_ref()) {
            None => Outcome::Comparing,
            Some(None) => Outcome::Failed,
            Some(Some(difference)) => Outcome::Made(difference),
        }
    }

    /// Compares the two images, off the main thread, if the comparison of what changed is chosen and they haven't been.
    /// Comparing two others in their place stops it.
    fn compare_if_wanted(&mut self, cx: &mut Context<Self>) {
        let Some((before, after)) = self.images() else { return };
        let pair = (before.hash, after.hash);
        let made = self.compared.as_ref().is_some_and(|compared| compared.of == pair);
        let comparing = self.comparing.as_ref().is_some_and(|(of, _)| *of == pair);
        if comparison(cx) != Comparison::Difference || made || comparing {
            return;
        }
        let (before, after, colour) = (before.clone(), after.clone(), difference::colour(look::colors(cx).modified));
        let task = cx.spawn(async move |this, cx| {
            let difference = cx.background_spawn(async move { Difference::between(&before, &after, colour) }).await;
            let _ = this.update(cx, |t, cx| {
                t.forget_difference(cx);
                t.compared = Some(Compared { of: pair, difference: difference.map(Rc::new) });
                cx.notify();
            });
        });
        self.comparing = Some((pair, task));
    }

    /// Lets the picture of what changed go, and stops comparing.
    fn forget_difference(&mut self, cx: &mut App) {
        self.comparing = None;
        if let Some(difference) = self.compared.take().and_then(|compared| compared.difference) {
            file_image::release(difference.picture.clone(), cx);
        }
    }

    /// What each side shows, before and after: an image's kind and size, `none` for a side the change hasn't got,
    /// `loading`, or why it can't be drawn.
    #[cfg(feature = "testing")]
    pub fn described(&self) -> (String, String) {
        let describe = |side: &Side| match side {
            Side::Loading => "loading".to_string(),
            Side::Absent => "none".to_string(),
            Side::Image(image) => image.describe(),
            Side::Other(why) => why.to_string(),
        };
        (describe(&self.before), describe(&self.after))
    }

    /// Reads both sides again, keeping what's shown until they're read, and what's shown of one that hasn't changed.
    fn read(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.path.as_ref() else { return };
        let review = self.review.read(cx);
        let Some(file) = review.file(path).filter(|file| file.is_binary()) else { return };
        let (before, after) = (file.old_path().map(ToString::to_string), file.new_path().map(ToString::to_string));
        let ((client, id), base) = (review.source(), review.base_commit.clone());
        let from = base.clone().zip(before.clone());
        let read_before = from.is_none() || from != self.before_from || !matches!(self.before, Side::Image(_));
        let shown = (self.before.hash(), self.after.hash());
        self.reads += 1;
        let read = self.reads;
        cx.spawn(async move |this, cx| {
            let sides = cx.background_spawn(async move {
                let before = match (before, base) {
                    _ if !read_before => None,
                    (None, _) => Some(Side::Absent),
                    (Some(path), Some(base)) => side(client.workspace_file_read_at(&id, &base, &path), shown.0),
                    // The diff names its commit unless it's from a worker too old to.
                    (Some(_), None) => Some(Side::Other(Unshown::Unsupported.said().into())),
                };
                let after =
                    after.map_or(Some(Side::Absent), |path| side(client.workspace_file_read(&id, &path), shown.1));
                (before, after)
            });
            let (before, after) = sides.await;
            let _ = this.update(cx, |t, cx| {
                if read != t.reads {
                    return;
                }
                if let Some(before) = before {
                    t.before_from = from.filter(|_| matches!(before, Side::Image(_)));
                    t.before.set(before, cx);
                }
                if let Some(after) = after {
                    t.after.set(after, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }
}

/// What a read gives one side, made ready to draw, which decodes an image. `None` when what's shown stays: when the
/// read was lost with the connection, and the side is read again with the diff on the next one, or when it's `shown`,
/// the hash of the image shown, read again.
fn side(contents: Result<FileContents, ClientError>, shown: Option<u64>) -> Option<Side> {
    let why = match what_it_gives(contents) {
        Read::Lost => return None,
        Read::Image(media_type, data) => {
            let hash = hash(&data);
            if shown == Some(hash) {
                return None;
            }
            let image = FileImage::read(media_type, &data, hash);
            return Some(image.map_or_else(|| Side::Other(Unshown::Unreadable.said().into()), Side::Image));
        }
        Read::Text(_) => Unshown::Text,
        Read::Instead(why) => why,
    };
    Some(Side::Other(why.said().into()))
}

#[cfg(test)]
mod tests;
