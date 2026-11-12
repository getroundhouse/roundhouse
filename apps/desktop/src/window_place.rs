//! Where the window was left: its place and size, the display it was on, and whether it filled the screen, so the app
//! opens as it was last used. It belongs to this Mac, as a setting does, but nobody chooses it in Settings, so it has a
//! file of its own in the app's data directory.

use std::path::Path;
use std::time::Duration;

use gpui_kit::{
    AnyWindowHandle, App, Bounds, Context, DisplayId, Global, Pixels, Task, Window, WindowBounds, point, px, size,
};
use serde::{Deserialize, Serialize};

use crate::saving::{Saver, write_json};

#[cfg(test)]
mod tests;

const FILE: &str = "window.json";

/// How long the window holds still before where it is gets saved. Dragging it moves it on every frame, and only where
/// it ends up is worth writing.
pub(crate) const SETTLES_AFTER: Duration = Duration::from_millis(500);

/// The size the window opens at, in the middle of the main display, until it has been left somewhere.
const USUAL_SIZE: (f32, f32) = (1320., 860.);

/// The height of the strip along the window's top, which it's moved by. For the window to open where it was, that
/// strip has to be on its display, top to bottom and for as far across. With less, it could open out of reach, as
/// after a display gets smaller.
const WITHIN_REACH: Pixels = px(40.);

/// Where the main window opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opens {
    /// Where it was when the app last ran, which the app goes on keeping, in its data directory.
    WhereItWasLeft,
    /// In its usual place every time, for a picture of the app, and nothing is kept.
    InItsUsualPlace,
}

/// The display and bounds to open the window with. For [`Opens::WhereItWasLeft`], that's read from `data_dir`, and
/// from then on [`follow`] keeps up with the window, and the app waits for where it is to be saved as it quits.
pub(crate) fn open_at(opens: Opens, data_dir: &Path, cx: &mut App) -> (Option<DisplayId>, WindowBounds) {
    let left = match opens {
        Opens::WhereItWasLeft => load(data_dir, cx),
        Opens::InItsUsualPlace => None,
    };
    if let Some(Opening { display, bounds }) = left {
        return (Some(display), bounds);
    }
    let usual = size(px(USUAL_SIZE.0), px(USUAL_SIZE.1));
    (None, WindowBounds::Windowed(Bounds::centered(None, usual, cx)))
}

/// Keeps up with where `window` is, and saves it once it has settled there. Does nothing for a window opened
/// [`Opens::InItsUsualPlace`].
pub(crate) fn follow<V: 'static>(window: &mut Window, cx: &mut Context<V>) {
    if !cx.has_global::<Kept>() {
        return;
    }
    let opened_at = Place::of(window, cx);
    let kept = cx.global_mut::<Kept>();
    (kept.window, kept.opened_at) = (Some(window.window_handle()), opened_at.filter(|_| kept.in_usual_place));
    // GPUI says the bounds changed whenever the window comes to the front or leaves it, too.
    cx.observe_window_bounds(window, |_, window, cx| moved(window, cx)).detach();
}

/// Notes where the window is as the app quits, or the window closes, which quits it. By then the window is gone, so
/// this is the last look at it. Without it, a window closed after leaving full screen would count as still in it, as it
/// was when it last moved.
pub(crate) fn last_look(window: &Window, cx: &mut App) {
    if cx.has_global::<Kept>() {
        see(window, cx);
    }
}

fn moved(window: &Window, cx: &mut App) {
    see(window, cx);
    let settling = cx.spawn(async move |cx| {
        cx.background_executor().timer(SETTLES_AFTER).await;
        cx.update(save);
    });
    cx.global_mut::<Kept>().settling = Some(settling);
}

/// Notes where the window is now.
fn see(window: &Window, cx: &mut App) {
    if let Some(place) = Place::of(window, cx) {
        cx.global_mut::<Kept>().last_seen = Some(place);
    }
}

/// A display's own ID, which is the same from one launch to the next.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
struct DisplayUuid(String);

/// How much of its display the window takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Mode {
    Windowed,
    /// As large as the display allows beside its menu bar or taskbar, where the system keeps that apart from a size
    /// someone dragged it to. A Mac doesn't, so a window that fills the screen there is `Windowed`, at that size.
    Maximized,
    FullScreen,
}

/// Where the window is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Place {
    /// `None` when the display has no ID to keep, which counts as the main display.
    display: Option<DisplayUuid>,
    /// Its corner and size as GPUI gives them, in points. In any other mode than `Windowed`, where it goes back to
    /// on leaving that mode.
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    mode: Mode,
}

/// A display that's attached now.
struct Attached {
    id: DisplayId,
    uuid: Option<DisplayUuid>,
    /// In the coordinates a window's bounds on it are in.
    bounds: Bounds<Pixels>,
    main: bool,
}

/// Where to open the window, as GPUI takes it.
#[derive(Debug, PartialEq)]
struct Opening {
    display: DisplayId,
    bounds: WindowBounds,
}

impl Place {
    /// `None` while the window is on no display, as it can be while it closes, when GPUI makes up its bounds.
    fn of(window: &Window, cx: &App) -> Option<Self> {
        let display = window.display(cx)?;
        let (bounds, mode) = match window.window_bounds() {
            WindowBounds::Windowed(bounds) => (bounds, Mode::Windowed),
            WindowBounds::Maximized(bounds) => (bounds, Mode::Maximized),
            WindowBounds::Fullscreen(bounds) => (bounds, Mode::FullScreen),
        };
        Some(Self {
            display: uuid(display.as_ref()),
            x: bounds.origin.x.into(),
            y: bounds.origin.y.into(),
            width: bounds.size.width.into(),
            height: bounds.size.height.into(),
            mode,
        })
    }

    /// Where this opens the window, or `None` when its display is gone, or too little of it would be on that display
    /// to reach, in which case the window opens in its usual place.
    fn opening(&self, attached: &[Attached]) -> Option<Opening> {
        let display = attached.iter().find(|display| match &self.display {
            Some(uuid) => display.uuid.as_ref() == Some(uuid),
            None => display.main,
        })?;
        // What overlaps a display is worked out with `min` and `max`, which pass over what isn't a number.
        if ![self.x, self.y, self.width, self.height].into_iter().all(f32::is_finite) {
            return None;
        }
        let bounds = Bounds::new(point(px(self.x), px(self.y)), size(px(self.width), px(self.height)));
        let strip = Bounds::new(bounds.origin, size(bounds.size.width, WITHIN_REACH));
        let showing = strip.intersect(&display.bounds).size;
        let within_reach =
            bounds.size.height >= WITHIN_REACH && showing.width >= WITHIN_REACH && showing.height >= WITHIN_REACH;
        within_reach.then_some(Opening {
            display: display.id,
            bounds: match self.mode {
                Mode::Windowed => WindowBounds::Windowed(bounds),
                Mode::Maximized => WindowBounds::Maximized(bounds),
                Mode::FullScreen => WindowBounds::Fullscreen(bounds),
            },
        })
    }

    /// Whether this is worth saving: it isn't what's `saved`, and the window has moved from `opened_at`, where it
    /// opened if that was its usual place. A window still in its usual place isn't saved, so one that opened there
    /// because its display was gone goes back to that display once it's attached again.
    fn is_news(&self, saved: Option<&Self>, opened_at: Option<&Self>) -> bool {
        Some(self) != saved && Some(self) != opened_at
    }
}

fn uuid(display: &dyn gpui_kit::PlatformDisplay) -> Option<DisplayUuid> {
    display.uuid().ok().map(|uuid| DisplayUuid(uuid.to_string()))
}

fn attached(cx: &App) -> Vec<Attached> {
    let main = cx.primary_display().map(|display| display.id());
    let attached = |display: &std::rc::Rc<dyn gpui_kit::PlatformDisplay>| Attached {
        id: display.id(),
        uuid: uuid(display.as_ref()),
        bounds: display.bounds(),
        main: Some(display.id()) == main,
    };
    cx.displays().iter().map(attached).collect()
}

/// What's saved, or `None` when nothing is, or what is can't be read.
fn read(path: &Path) -> Option<Place> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text)
        .map_err(|e| {
            eprintln!(
                "couldn't read where the window was left, in {}, so it opens in its usual place: {e}",
                path.display()
            );
        })
        .ok()
}

/// Where the window is, and what saves that.
struct Kept {
    saver: Saver<Place>,
    /// What the file holds, or will once the saver gets to it.
    saved: Option<Place>,
    /// Whether the window opened in its usual place, rather than where it was left.
    in_usual_place: bool,
    /// Where a window that opened in its usual place opened, until it's saved somewhere else.
    opened_at: Option<Place>,
    /// The window, once it's open.
    window: Option<AnyWindowHandle>,
    /// Where the window was when it was last looked at, for once it has closed.
    last_seen: Option<Place>,
    /// Saves where the window is once it has held still for [`SETTLES_AFTER`]. Each move replaces it, which stops the
    /// one before.
    settling: Option<Task<()>>,
}

impl Global for Kept {}

/// Saves where the window is, if that's somewhere new. It looks at the window again rather than going by where it
/// last moved to, because GPUI says nothing once the window has finished leaving full screen, and until then it still
/// counts as in full screen. A window that has closed is where it was last seen.
fn save(cx: &mut App) {
    if let Some(window) = cx.global::<Kept>().window {
        let _ = window.update(cx, |_, window, cx| see(window, cx));
    }
    let kept = cx.global_mut::<Kept>();
    let Some(now) = kept.last_seen.clone() else { return };
    if !now.is_news(kept.saved.as_ref(), kept.opened_at.as_ref()) {
        return;
    }
    kept.opened_at = None;
    kept.saved = Some(now.clone());
    kept.saver.save(now);
}

/// Reads where the window was left, and says where that opens it now, if anywhere. The first time, it also starts
/// what saves where the window is.
fn load(data_dir: &Path, cx: &mut App) -> Option<Opening> {
    let path = data_dir.join(FILE);
    let left = read(&path);
    let opening = left.as_ref().and_then(|place| place.opening(&attached(cx)));
    let in_usual_place = opening.is_none();
    if cx.has_global::<Kept>() {
        cx.global_mut::<Kept>().in_usual_place = in_usual_place;
        return opening;
    }
    let (saver, failed) = Saver::start(move |place| write_json(&path, place));
    let (opened_at, window, last_seen, settling) = (None, None, None, None);
    cx.set_global(Kept { saver, saved: left, in_usual_place, opened_at, window, last_seen, settling });
    // Nobody is told, since the only cost is the window opening somewhere else next time.
    cx.spawn(async move |_| {
        while let Ok(failure) = failed.recv_async().await {
            eprintln!("couldn't save where the window is: {failure}");
        }
    })
    .detach();
    // Quitting doesn't give the wait for the window to settle its turn, so this saves where it is now.
    cx.on_app_quit(|cx| {
        let all_saved = cx.has_global::<Kept>().then(|| {
            save(cx);
            cx.global::<Kept>().saver.saved()
        });
        async move {
            if let Some(all_saved) = all_saved {
                let _ = all_saved.recv_async().await;
            }
        }
    })
    .detach();
    opening
}
