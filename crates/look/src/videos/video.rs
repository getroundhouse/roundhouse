use gpui_kit::assets::IconName;
use gpui_kit::{
    AnyElement, App, Bounds, ElementId, HitboxBehavior, Hsla, InteractiveElement as _, IntoElement, MouseButton,
    MouseDownEvent, ParentElement as _, Pixels, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    Styled as _, TestSupportExt as _, Window, canvas, div, fill, px,
};
use std::rc::Rc;
use std::time::Duration;

use super::length::length;
use crate::{
    ButtonKind, ControlSize, Corner, EDGE, Hand as _, Shaped as _, Space, Type, Typeset as _, colors, icon_button,
};

/// How tall the track under a playing video is, which a click goes along.
const TRACK: Pixels = px(4.);

/// The least a video is drawn across, a small one's picture made bigger to fill it, so its bar has room for all it
/// holds.
pub const VIDEO_LEAST_WIDTH: Pixels = px(240.);

/// How much taller a video is than its picture: its bar, and its edge at the top and the bottom, which fitting a video
/// in some room leaves room for.
pub fn video_bar_height() -> Pixels {
    ControlSize::Small.height() + Space::XSmall.size() * 2. + EDGE * 2.
}

/// What a handler is given.
type Handler<T> = Rc<dyn Fn(T, &mut Window, &mut App)>;

/// Where a video is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoShows {
    /// Its first frame, before it's been played, with the play mark over it.
    Still,
    /// Playing, this far in.
    Playing(Duration),
    /// Stopped this far in, by a pause or at its end.
    Paused(Duration),
}

/// A video, found by `id`, showing `picture` where it is, of `length` if the file says how long it is. What
/// [`Video::on_play`] and the rest are given say what each press does, and whatever holds the video draws it again.
pub fn video(id: impl Into<ElementId>, picture: AnyElement, shows: VideoShows, length: Option<Duration>) -> Video {
    Video { id: id.into(), picture, shows, length, says: None, play: None, pause: None, seek: None, expand: None }
}

/// A picture of a video, with a bar under it, which [`video`] makes.
#[derive(IntoElement)]
pub struct Video {
    id: ElementId,
    picture: AnyElement,
    shows: VideoShows,
    length: Option<Duration>,
    says: Option<SharedString>,
    play: Option<Handler<()>>,
    pause: Option<Handler<()>>,
    seek: Option<Handler<f32>>,
    expand: Option<Handler<()>>,
}

impl Video {
    /// A line over the picture, in place of the play mark, such as why it couldn't be played.
    #[must_use]
    pub fn says(mut self, says: impl Into<SharedString>) -> Self {
        self.says = Some(says.into());
        self
    }

    /// What a press on the play mark or the bar's play does.
    #[must_use]
    pub fn on_play(mut self, play: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.play = Some(Rc::new(move |(), window, cx| play(window, cx)));
        self
    }

    /// What a press on the bar's pause does.
    #[must_use]
    pub fn on_pause(mut self, pause: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.pause = Some(Rc::new(move |(), window, cx| pause(window, cx)));
        self
    }

    /// What a press on the track does, given how far along it the press was, from 0 at its start to 1 at its end.
    #[must_use]
    pub fn on_seek(mut self, seek: impl Fn(f32, &mut Window, &mut App) + 'static) -> Self {
        self.seek = Some(Rc::new(seek));
        self
    }

    /// What Expand does. Without it, the bar has no Expand, as a video already as big as the window can show it.
    #[must_use]
    pub fn on_expand(mut self, expand: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.expand = Some(Rc::new(move |(), window, cx| expand(window, cx)));
        self
    }

    /// The bar under the picture: play or pause, where it is of how long, the track, and Expand.
    fn bar(&self, cx: &App) -> AnyElement {
        let colors = colors(cx);
        let (playing, at) = match self.shows {
            VideoShows::Still => (false, Duration::ZERO),
            VideoShows::Playing(at) => (true, at),
            VideoShows::Paused(at) => (false, at),
        };
        let (icon, press, label) = if playing {
            (IconName::Pause, self.pause.clone(), "Pause")
        } else {
            (IconName::Play, self.play.clone(), "Play")
        };
        let toggle = icon_button("play", ButtonKind::Quiet, ControlSize::Small, icon)
            .accessibility_label(label)
            .on_click(move |_, window, cx| press.iter().for_each(|press| press((), window, cx)));
        let said = match self.length {
            Some(of) => format!("{} / {}", length(at), length(of)),
            None => length(at),
        };
        let shown = self.length.map_or(0., |of| (at.as_secs_f32() / of.as_secs_f32().max(f32::EPSILON)).min(1.));
        let bar = div()
            .flex()
            .items_center()
            .gap(Space::Small)
            .px(Space::Small)
            .py(Space::XSmall)
            .h(video_bar_height() - EDGE * 2.)
            .bg(colors.raised)
            .child(toggle)
            .child(
                div()
                    .id("time")
                    .role(Role::Label)
                    .aria_label(said.clone())
                    .test_support()
                    .typeset(Type::Detail)
                    .text_color(colors.text_muted)
                    .child(said),
            )
            .child(track(shown, colors.text_faint, colors.text, self.seek.clone()));
        match self.expand.clone() {
            Some(expand) => bar
                .child(
                    icon_button("expand", ButtonKind::Quiet, ControlSize::Small, IconName::Maximize)
                        .accessibility_label("Expand")
                        .tooltip("Expand")
                        .on_click(move |_, window, cx| expand((), window, cx)),
                )
                .into_any_element(),
            None => bar.into_any_element(),
        }
    }
}

/// The line along the bar, `shown` of it filled in, which a press goes along, as far as it's pressed, unless what's
/// pressed is drawn over it, such as a dialog.
fn track(shown: f32, empty: Hsla, filled: Hsla, seek: Option<Handler<f32>>) -> AnyElement {
    let line = canvas(
        |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
        move |bounds: Bounds<Pixels>, hitbox, window, _| {
            let middle = bounds.origin.y + (bounds.size.height - TRACK) / 2.;
            let whole = Bounds::new(gpui_kit::point(bounds.origin.x, middle), gpui_kit::size(bounds.size.width, TRACK));
            window.paint_quad(fill(whole, empty).corner_radii(TRACK / 2.));
            let done = Bounds::new(whole.origin, gpui_kit::size(whole.size.width * shown, TRACK));
            window.paint_quad(fill(done, filled).corner_radii(TRACK / 2.));
            let Some(seek) = seek.clone() else { return };
            window.on_mouse_event(move |press: &MouseDownEvent, phase, window, cx| {
                if phase.bubble() && press.button == MouseButton::Left && hitbox.is_hovered(window) {
                    let along = (press.position.x - bounds.origin.x) / bounds.size.width;
                    seek(along.clamp(0., 1.), window, cx);
                }
            });
        },
    );
    let track = div().id("track").role(Role::Slider).aria_label("Position").test_support();
    track.flex_1().h(Space::Medium).hand().child(line.size_full()).into_any_element()
}

impl RenderOnce for Video {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = colors(cx);
        let bar = self.bar(cx);
        let mut picture = div().relative().w_full().child(self.picture);
        let over = |shown: AnyElement| div().absolute().inset_0().flex().items_center().justify_center().child(shown);
        if let Some(says) = self.says {
            let line = div()
                .px(Space::Small)
                .py(Space::XSmall)
                .bg(colors.raised)
                .corners(Corner::Control)
                .typeset(Type::Detail)
                .text_color(colors.text_muted)
                .child(says);
            picture = picture.child(over(line.into_any_element()));
        } else if self.shows == VideoShows::Still
            && let Some(play) = self.play.clone()
        {
            let mark = icon_button("play-mark", ButtonKind::Main, ControlSize::Large, IconName::Play)
                .accessibility_label("Play")
                .on_click(move |_, window, cx| play((), window, cx));
            picture = picture.child(over(mark.into_any_element()));
        }
        div()
            .id(self.id)
            .flex()
            .flex_col()
            .w_full()
            .overflow_hidden()
            .border(EDGE)
            .border_color(colors.edge)
            .corners(Corner::Card)
            .child(picture)
            .child(bar)
    }
}
