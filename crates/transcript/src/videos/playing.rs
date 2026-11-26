use gpui_kit::{
    App, AppContext as _, Context, Entity, IntoElement, ObjectFit, ParentElement as _, Render, RenderImage,
    Styled as _, StyledImage as _, Task, Window, div, img, px, size,
};
use look::{VideoShows, pixels, video_bar_height};
use std::sync::Arc;
use std::time::{Duration, Instant};
use video::{Decoded, Player, Shown};

use super::still::render_image;
use super::{Still, VideoSource};
use crate::images_that_open::{fitted, room};

/// How many frames the player's thread decodes ahead of the one shown, a sixth of a second at 30 frames a second,
/// enough to ride out a frame that takes long to decode, and few, since each is a whole picture.
const AHEAD: usize = 5;
/// How long a playing video can go undrawn before it's taken to have gone out of sight, as its row scrolls away, and
/// stops, which is several frames at the slowest a window draws.
const OUT_OF_SIGHT: Duration = Duration::from_millis(250);
/// How long a video that's paused, or sought while it isn't playing, keeps its decoder and the frames decoded ahead,
/// so it plays on at once if it's played again soon, before it lets them go, since they hold several whole pictures.
const KEPT_STOPPED: Duration = Duration::from_secs(10);

/// A video that plays where it's drawn, from its still, keeping its own time by GPUI's clock and taking each frame as
/// it's due. It's drawn as a view, so what its thread decodes draws it again. Its decoder, on a thread of its own, and
/// the frames decoded ahead, start when it's first played or sought, and go once it's played to its end, once it's
/// been stopped for [`KEPT_STOPPED`], or once it's gone undrawn for [`OUT_OF_SIGHT`] as it plays, as when its row's
/// scrolled out of sight or the window's hidden, when it stops where it was last drawn. Played again, they start again
/// from where it is.
pub struct VideoPlayer {
    source: VideoSource,
    still: Still,
    at: At,
    /// While it's decoded, which it starts being the first time it's played or sought.
    decoding: Option<Decoding>,
    /// The last frame taken, and its time, once one has been since it was made.
    frame: Option<(Duration, Arc<RenderImage>)>,
    /// Why it couldn't be played, until it's played again.
    failed: Option<&'static str>,
    /// When it was last drawn as it played, or stopped playing, which its decoder goes some time after.
    last_played: Option<Instant>,
    /// While it's decoded, what lets go of its decoder once it's been stopped, or undrawn, for too long.
    watching: Option<Task<()>>,
    _releasing: gpui_kit::Subscription,
}

/// Where a video is.
#[derive(Clone, Copy, Debug, PartialEq)]
enum At {
    /// Its still, not yet played.
    Still,
    Paused(Duration),
    /// Played to its end, where it shows its last frame, and played again from its start.
    Ended(Duration),
    /// Playing from `from`, since `since`, which is `None` until the first frame from there has come, so the time spent
    /// opening it, or reading where it goes, isn't played past.
    Playing {
        from: Duration,
        since: Option<Instant>,
    },
}

/// A video's frames as its thread decodes them.
struct Decoding {
    player: Player,
    /// Hears that the thread has decoded something, and draws the video again.
    _woken: Task<()>,
    /// The seek the frames taken now follow.
    seek: u64,
    /// Whether one of `seek`'s frames has been taken, before which the first is due whenever it comes.
    seen: bool,
    /// A frame taken that isn't due yet.
    next: Option<Shown>,
    /// Whether there are no frames after `next`.
    ended: bool,
}

/// What's taken of what the thread decoded, as the video's about to be drawn.
enum Taken {
    /// The newest frame that's due, if any.
    Frame(Option<Shown>),
    Failed,
}

impl VideoPlayer {
    pub fn new(source: VideoSource, still: Still, cx: &mut Context<Self>) -> Self {
        let releasing = cx.on_release(|player, cx| {
            for image in player.frame.take().map(|(_, image)| image).into_iter().chain([player.still.image.clone()]) {
                cx.defer(move |cx| cx.drop_image(image, None));
            }
        });
        VideoPlayer {
            source,
            still,
            at: At::Still,
            decoding: None,
            frame: None,
            failed: None,
            last_played: None,
            watching: None,
            _releasing: releasing,
        }
    }

    /// Its first frame, how big it is, and how long the file says it is.
    pub fn still(&self) -> &Still {
        &self.still
    }

    /// Plays it from where it is, or from its start if it's at its end.
    pub fn play(&mut self, cx: &mut Context<Self>) {
        let now = cx.background_executor().now();
        let from = if self.at_the_end(now) { Duration::ZERO } else { self.position(now) };
        self.failed = None;
        // On from the frame shown, or from where it's decoded from next.
        let carries_on = self.decoding.as_ref().is_some_and(|decoding| decoding.seen) && from == self.position(now);
        if !carries_on && !self.decode_from(from, cx) {
            cx.notify();
            return;
        }
        self.at = At::Playing { from, since: carries_on.then_some(now) };
        self.watch(now, cx);
        cx.notify();
    }

    pub fn pause(&mut self, cx: &mut Context<Self>) {
        let now = cx.background_executor().now();
        self.at = At::Paused(self.position(now));
        self.watch(now, cx);
        cx.notify();
    }

    /// Goes `along` its length, from 0 at its start to 1 at its end, playing on from there if it was playing.
    pub fn seek(&mut self, along: f32, cx: &mut Context<Self>) {
        let Some(length) = self.length() else { return };
        // Never past its length, however long the file says that is.
        let to = Duration::try_from_secs_f64(length.as_secs_f64() * f64::from(along.clamp(0., 1.)))
            .map_or(length, |to| to.min(length));
        self.failed = None;
        if !self.decode_from(to, cx) {
            cx.notify();
            return;
        }
        self.at = match self.at {
            At::Playing { .. } => At::Playing { from: to, since: None },
            At::Still | At::Paused(_) | At::Ended(_) => At::Paused(to),
        };
        self.watch(cx.background_executor().now(), cx);
        cx.notify();
    }

    /// Stops where it was, since what was being read went with the connection before.
    pub fn stop_for_new_connection(&mut self, cx: &mut Context<Self>) {
        if let At::Playing { .. } = self.at {
            self.at = At::Paused(self.position(cx.background_executor().now()));
        }
        self.decoding = None;
        cx.notify();
    }

    fn length(&self) -> Option<Duration> {
        self.still.length
    }

    /// Whether it's played as far as it goes at `now`: its length, if the file says it, or else its last frame.
    fn at_the_end(&self, now: Instant) -> bool {
        match (self.at, self.length()) {
            (At::Ended(_), _) => true,
            (_, Some(length)) => self.position(now) >= length,
            (_, None) => self.decoding.as_ref().is_some_and(|decoding| decoding.ended && decoding.next.is_none()),
        }
    }

    /// Lets go of its decoder once it's gone undrawn for [`OUT_OF_SIGHT`] as it plays, stopping it where it was last
    /// drawn, or once it's been stopped for [`KEPT_STOPPED`], counting from `now`. A timer looks, since nothing draws a
    /// video out of sight, or one that's stopped.
    fn watch(&mut self, now: Instant, cx: &mut Context<Self>) {
        self.last_played = Some(now);
        self.watching = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(OUT_OF_SIGHT).await;
                if !this.update(cx, VideoPlayer::keeps_decoding).unwrap_or(false) {
                    break;
                }
            }
        }));
    }

    /// Whether it still keeps its decoder, letting go of it if it's been undrawn or stopped for too long.
    fn keeps_decoding(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(last_played) = self.last_played.filter(|_| self.decoding.is_some()) else { return false };
        let playing = matches!(self.at, At::Playing { .. });
        let kept = if playing { OUT_OF_SIGHT } else { KEPT_STOPPED };
        if cx.background_executor().now().saturating_duration_since(last_played) <= kept {
            return true;
        }
        if playing {
            self.at = At::Paused(self.position(last_played));
        }
        self.decoding = None;
        cx.notify();
        false
    }

    /// Where it is at `now`, which is never past its length, if the file says it.
    fn position(&self, now: Instant) -> Duration {
        let position = match self.at {
            At::Still => Duration::ZERO,
            At::Paused(at) | At::Ended(at) | At::Playing { from: at, since: None } => at,
            At::Playing { from, since: Some(since) } => from.saturating_add(now.saturating_duration_since(since)),
        };
        self.length().map_or(position, |length| position.min(length))
    }

    /// Has its frames decoded from `time` on, starting its thread if it isn't decoded now, which opens it. Says whether
    /// it could, which it can't only if the system won't start a thread.
    fn decode_from(&mut self, time: Duration, cx: &mut Context<Self>) -> bool {
        if self.decoding.is_none() {
            let (woke, woken) = flume::unbounded();
            let source = self.source.clone();
            let woke = move || {
                let _ = woke.send(());
            };
            let Ok(player) = Player::start(move || source.open(), AHEAD, woke) else {
                self.failed = Some("The video couldn't be played");
                return false;
            };
            let woken = cx.spawn(async move |this, cx| {
                while woken.recv_async().await.is_ok() {
                    if this.update(cx, |_, cx| cx.notify()).is_err() {
                        break;
                    }
                }
            });
            self.decoding = Some(Decoding { player, _woken: woken, seek: 0, seen: false, next: None, ended: false });
            if time.is_zero() {
                return true;
            }
        }
        if let Some(decoding) = &mut self.decoding {
            decoding.seek = decoding.player.seek(time);
            (decoding.seen, decoding.next, decoding.ended) = (false, None, false);
        }
        true
    }

    /// Takes what's due of what the thread decoded, as it's about to be drawn, and asks for the next frame while it
    /// plays, stopping it at its end.
    fn advance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = cx.background_executor().now();
        let position = self.position(now);
        let taken = self.decoding.as_mut().map(|decoding| decoding.take(position));
        match taken {
            Some(Taken::Failed) => {
                self.failed = Some("The video couldn't be played");
                self.at = At::Paused(position);
                self.decoding = None;
            }
            Some(Taken::Frame(Some(shown))) => {
                if let At::Playing { since: since @ None, .. } = &mut self.at {
                    *since = Some(now);
                }
                let (time, image) = (shown.time, render_image(shown).map(Arc::new));
                if let Some((_, old)) = std::mem::replace(&mut self.frame, image.map(|image| (time, image))) {
                    cx.drop_image(old, Some(window));
                }
            }
            Some(Taken::Frame(None)) | None => {}
        }
        if let At::Playing { .. } = self.at
            && self.at_the_end(now)
        {
            self.at = At::Ended(self.position(now));
            self.decoding = None;
        }
        if let At::Playing { .. } = self.at {
            self.last_played = Some(now);
            window.request_animation_frame();
        }
    }

    /// It as it's drawn: as wide as it is, up to what it's in, at its own shape, with its bar, and Expand if
    /// `expandable`.
    fn drawn(&mut self, expandable: bool, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.advance(window, cx);
        let (width, height) = (pixels(self.still.covers.0), pixels(self.still.covers.1));
        let image = self.frame.as_ref().map_or_else(|| self.still.image.clone(), |(_, image)| image.clone());
        let picture = div()
            .w_full()
            .aspect_ratio(width / height)
            .child(img(image).size_full().object_fit(ObjectFit::Contain))
            .into_any_element();
        let shows = match self.at {
            At::Still => VideoShows::Still,
            At::Paused(at) | At::Ended(at) => VideoShows::Paused(at),
            At::Playing { .. } => VideoShows::Playing(self.position(cx.background_executor().now())),
        };
        let this = cx.entity();
        let (play, pause, seek) = (this.clone(), this.clone(), this.clone());
        let mut video = look::video("video", picture, shows, self.length())
            .on_play(move |_, cx| play.update(cx, VideoPlayer::play))
            .on_pause(move |_, cx| pause.update(cx, VideoPlayer::pause))
            .on_seek(move |along, _, cx| seek.update(cx, |player, cx| player.seek(along, cx)));
        if let Some(failed) = self.failed {
            video = video.says(failed);
        }
        if expandable {
            video = video.on_expand(move |window, cx| expand(this.clone(), window, cx));
        }
        div().w_full().max_w(px(width).max(look::VIDEO_LEAST_WIDTH)).child(video)
    }
}

/// What a test reads of a player, which the window shows only as pixels.
#[cfg(any(test, feature = "testing"))]
impl VideoPlayer {
    /// The time of the frame drawn, or `None` while its still is.
    pub fn frame_drawn(&self) -> Option<Duration> {
        self.frame.as_ref().map(|(time, _)| *time)
    }

    /// Whether it keeps a decoder.
    pub fn decodes(&self) -> bool {
        self.decoding.is_some()
    }
}

impl Render for VideoPlayer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.drawn(true, window, cx)
    }
}

impl Decoding {
    /// The newest frame that's due at `position`, taking the rest of what the thread decoded up to the first that
    /// isn't, which waits in `next`. The first frame after a seek is due whenever it comes.
    fn take(&mut self, position: Duration) -> Taken {
        let mut due = None;
        loop {
            let next = match self.next.take() {
                Some(next) => next,
                None => match self.player.take() {
                    Some(Decoded::Frame(shown)) if shown.seek == self.seek => shown,
                    Some(Decoded::End { seek }) if seek == self.seek => {
                        self.ended = true;
                        continue;
                    }
                    Some(Decoded::Failed { seek }) if seek == self.seek => return Taken::Failed,
                    Some(Decoded::Unopened) => return Taken::Failed,
                    // Before the last seek.
                    Some(Decoded::Frame(_) | Decoded::End { .. } | Decoded::Failed { .. }) => continue,
                    None => break,
                },
            };
            if self.seen && next.time > position {
                self.next = Some(next);
                break;
            }
            self.seen = true;
            due = Some(next);
        }
        Taken::Frame(due)
    }
}

/// The video over the window, which an [`expand`]ed player is drawn in, with no Expand of its own.
struct Expanded(Entity<VideoPlayer>);

impl Render for Expanded {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.0.update(cx, |player, cx| player.drawn(false, window, cx).into_any_element())
    }
}

/// Opens `player` over the window, as big as it is, up to what fits with its bar, carrying on from where it is.
pub(crate) fn expand(player: Entity<VideoPlayer>, window: &mut Window, cx: &mut App) {
    let covers = player.read(cx).still.covers;
    let expanded = cx.new(|cx| {
        cx.observe(&player, |_, _, cx| cx.notify()).detach();
        Expanded(player)
    });
    look::open_plain_dialog(window, cx, move |window, _| {
        // Fitted again as the window is resized.
        let room = room(window);
        let width = fitted(covers, size(room.width, room.height - video_bar_height())).width;
        look::plain_dialog(div().w(width).child(expanded.clone()), width).centered()
    });
}
