//! A terminal someone has open, as a view: its grid, which takes the keys while it's focused, pastes, and turns the
//! wheel through its history.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use client_core::OpenTerminal;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::input::Paste;
use gpui_kit::{
    App, ClipboardItem, Context, FocusHandle, Focusable, InteractiveElement, IntoElement, KeyDownEvent, MouseButton,
    MouseDownEvent, OperatingSystem, ParentElement, Pixels, Render, ScrollEvent, Styled, Task, Window, div, font, px,
};
use terminal_state::Size;
use terminal_state::drawn::{Frame, Rgb};

use crate::grid::{Glyphs, Grid, Metrics};
use crate::keys::key_press;

/// The key context of a terminal's view, which takes the kit's `Paste`, as the text boxes do, so the Edit menu's Paste
/// pastes in a terminal too. The app binds its key in it.
pub const KEY_CONTEXT: &str = "Terminal";

/// A terminal someone has open, drawn from client-core's copy of what it shows.
pub struct TerminalView {
    open: OpenTerminal,
    frame: Option<Arc<Frame>>,
    background: Rgb,
    metrics: Metrics,
    glyphs: Glyphs,
    focus: FocusHandle,
    /// The size the view last said it had room for.
    fit: Rc<Cell<Option<Size>>>,
    /// The part of a line the wheel has turned that hasn't scrolled a line yet.
    wheel: Pixels,
    /// Draws the view again as client-core's copy changes, until the view has gone.
    _changes: Task<()>,
    os: OperatingSystem,
}

impl TerminalView {
    /// A view of `open`, in the theme's code font, on its background.
    pub fn new(open: OpenTerminal, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let theme = cx.theme();
        let foreground = rgb_of(theme.foreground);
        let background = rgb_of(theme.background);
        let metrics = Metrics::of(font(theme.mono_font_family.clone()), theme.mono_font_size, window);
        open.set_colors(foreground, background);
        // Draws again once there's a terminal's frame to take, or the shell ends.
        let changed = open.changed().clone();
        let drawing_again = cx.spawn(async move |view, cx| {
            while changed.recv_async().await.is_ok() {
                if view.update(cx, |_, cx| cx.notify()).is_err() {
                    return;
                }
            }
        });
        Self {
            open,
            frame: None,
            background,
            metrics,
            glyphs: Glyphs::default(),
            focus: cx.focus_handle(),
            fit: Rc::new(Cell::new(None)),
            wheel: px(0.),
            _changes: drawing_again,
            os: cx.platform_info().os,
        }
    }

    pub fn open_terminal(&self) -> &OpenTerminal {
        &self.open
    }

    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(press) = key_press(event, self.os) else { return };
        self.open.key(press);
        cx.stop_propagation();
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().as_ref().and_then(ClipboardItem::text) {
            self.open.paste(text);
        }
    }

    fn scroll_wheel(&mut self, event: &ScrollEvent, _: &mut Window, cx: &mut Context<Self>) {
        let line = self.metrics.line_height;
        let lines = turned(&mut self.wheel, event.delta.pixel_delta(line).y, line);
        if lines != 0 {
            self.open.wheel(lines);
        }
        cx.stop_propagation();
    }

    fn mouse_down(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
    }
}

/// How many whole lines a turn of the wheel by `delta_y` scrolls, up through the history for fewer than none, with
/// `rest` the part of a line the turns before it left, which keeps what this one leaves.
fn turned(rest: &mut Pixels, delta_y: Pixels, line: Pixels) -> isize {
    // GPUI's wheel goes down the page for more than none, which is up through the history.
    *rest -= delta_y;
    let lines = (*rest / line).trunc();
    *rest -= line * lines;
    whole_lines(lines)
}

#[expect(clippy::cast_possible_truncation, reason = "whole lines a turn of the wheel scrolls, which are few")]
fn whole_lines(lines: f32) -> isize {
    lines as isize
}

impl Focusable for TerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(frame) = self.open.take_frame() {
            self.frame = Some(frame);
        }
        let (open, fit) = (self.open.clone(), self.fit.clone());
        let grid = Grid {
            frame: self.frame.clone(),
            background: self.background,
            metrics: self.metrics.clone(),
            glyphs: self.glyphs.clone(),
            // The window has to be in front too, as the keys go to another app's while it isn't.
            focused: self.focus.is_focused(window) && window.is_window_active(),
            fits: Box::new(move |size| {
                if fit.replace(Some(size)) != Some(size) {
                    open.fit(size);
                }
            }),
        };
        div()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus)
            .size_full()
            .overflow_hidden()
            .on_key_down(cx.listener(Self::key_down))
            .on_action(cx.listener(Self::paste))
            .on_scroll(cx.listener(Self::scroll_wheel))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .child(grid)
    }
}

fn rgb_of(color: gpui_kit::Hsla) -> Rgb {
    let rgba = color.to_rgb();
    Rgb { r: channel(rgba.r), g: channel(rgba.g), b: channel(rgba.b) }
}

#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a colour's channel, from 0 to 1")]
fn channel(value: f32) -> u8 {
    (value.clamp(0., 1.) * 255.).round() as u8
}

#[cfg(test)]
mod tests {
    use gpui_kit::px;
    use terminal_state::drawn::Rgb;

    use super::{rgb_of, turned};

    #[test]
    fn a_colour_keeps_its_channels() {
        assert_eq!(rgb_of(gpui_kit::rgb(0x15_1312).into()), Rgb { r: 0x15, g: 0x13, b: 0x12 });
    }

    /// A trackpad turns it by parts of a line, which add up.
    #[test]
    fn the_wheel_scrolls_whole_lines_and_keeps_the_rest() {
        let (mut rest, line) = (px(0.), px(10.));

        assert_eq!(turned(&mut rest, px(25.), line), -2, "down the page is up through the history");
        assert_eq!(turned(&mut rest, px(4.), line), 0, "with half a line left, and this, not yet one");
        assert_eq!(turned(&mut rest, px(1.), line), -1);
        assert_eq!(turned(&mut rest, px(-10.), line), 1, "and back");
        assert_eq!(rest, px(0.));
    }
}
