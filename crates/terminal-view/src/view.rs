//! A terminal someone has open, as a view: its grid, which takes the keys while it's focused, pastes, turns the wheel
//! through its history, and selects with the pointer, and copies what's selected.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use client_core::OpenTerminal;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::input::{Copy, Paste, SelectAll};
use gpui_kit::{
    App, ClipboardItem, Context, FocusHandle, Focusable, Hsla, InteractiveElement, IntoElement, KeyBinding,
    KeyDownEvent, MouseButton, MouseDownEvent, NoAction, OperatingSystem, ParentElement, Pixels, Render, ScrollEvent,
    Styled, Task, TestSupportExt as _, Window, div, font, px,
};
use terminal_state::Size;
use terminal_state::drawn::{Frame, Rgb};

use crate::grid::{Glyphs, Grid, Metrics, Pointing};
use crate::keys::key_press;

/// Binds a terminal's own keys: copy, paste and select all, with Command on a Mac, and with Control and Shift
/// elsewhere, as terminal apps there have them, since a terminal's Control keys are its program's. They're the kit's
/// actions, which the text boxes take too, so the Edit menu's items work in a terminal as well. And keeps the kit's
/// from keys that are the program's: Tab and Shift-Tab, which it has move the keys to what's next on screen, and Ctrl-C
/// off the Mac, which it has copy. On a Mac, it gives the keys Ghostty gives a meaning there that meaning: Command with
/// Left, Right or Backspace, which would otherwise be the app's, and Option with Left or Right, which would otherwise be
/// sent as a key few shells read.
pub fn bind_keys(cx: &mut App) {
    let mac = cx.platform_info().is_mac();
    let [copy, paste, select_all] =
        if mac { ["cmd-c", "cmd-v", "cmd-a"] } else { ["ctrl-shift-c", "ctrl-shift-v", "ctrl-shift-a"] };
    cx.bind_keys([
        KeyBinding::new(copy, Copy, Some(KEY_CONTEXT)),
        KeyBinding::new(paste, Paste, Some(KEY_CONTEXT)),
        KeyBinding::new(select_all, SelectAll, Some(KEY_CONTEXT)),
        // Deeper than the kit's, so its are passed over, and the key goes on to the view.
        KeyBinding::new("tab", NoAction, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-tab", NoAction, Some(KEY_CONTEXT)),
    ]);
    if mac {
        let send = |keys, text| KeyBinding::new(keys, SendText(text), Some(KEY_CONTEXT));
        cx.bind_keys([
            // Ctrl-A and Ctrl-E, to the start of the line and its end, and Ctrl-U, which bash and fish read as deleting
            // back to its start, and zsh as deleting the line.
            send("cmd-left", "\x01"),
            send("cmd-right", "\x05"),
            send("cmd-backspace", "\x15"),
            // Escape and `b`, back a word, and Escape and `f`, on a word.
            send("alt-left", "\x1bb"),
            send("alt-right", "\x1bf"),
        ]);
    } else {
        cx.bind_keys([KeyBinding::new("ctrl-c", NoAction, Some(KEY_CONTEXT))]);
    }
}

/// Sends its text to the program as it is, whatever modes the program set, for a key bound to it.
#[derive(Clone, PartialEq, Debug, gpui_kit::Action)]
#[action(namespace = terminal, no_json)]
struct SendText(&'static str);

/// The context a terminal's keys are bound in, which the app leaves some of its own shortcuts out of.
pub const KEY_CONTEXT: &str = "Terminal";
/// The view's ID once it has drawn what its terminal shows.
pub const SHOWN_ID: &str = "terminal-shown";

/// A terminal someone has open, drawn from client-core's copy of what it shows.
pub struct TerminalView {
    open: OpenTerminal,
    frame: Option<Arc<Frame>>,
    background: Rgb,
    /// The theme's, for what's selected.
    selection: Hsla,
    metrics: Metrics,
    glyphs: Glyphs,
    focus: FocusHandle,
    /// The size the view last said it had room for.
    fit: Rc<Cell<Option<Size>>>,
    /// The part of a line the wheel has turned that hasn't scrolled a line yet.
    wheel: Pixels,
    /// Draws the view again as client-core's copy changes, until the view has gone.
    _changes: Task<()>,
    /// Whether the pointer's button went down on the grid and hasn't come up.
    dragging: Rc<Cell<bool>>,
    os: OperatingSystem,
}

impl TerminalView {
    /// A view of `open`, in the theme's code font, on its background.
    pub fn new(open: OpenTerminal, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let theme = cx.theme();
        let foreground = rgb_of(theme.foreground);
        let (background, selection) = (rgb_of(theme.background), theme.selection);
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
        let focus = cx.focus_handle();
        let dragging = Rc::new(Cell::new(false));
        // A drag whose button comes up while the grid isn't drawn, as when another tab comes to the front, is over,
        // and a press on the grid gives it the keys again first.
        let lost = dragging.clone();
        cx.on_blur(&focus, window, move |_, _, _| lost.set(false)).detach();
        Self {
            open,
            frame: None,
            background,
            selection,
            metrics,
            glyphs: Glyphs::default(),
            focus,
            fit: Rc::new(Cell::new(None)),
            wheel: px(0.),
            _changes: drawing_again,
            dragging,
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

    fn send_text(&mut self, text: &SendText, _: &mut Window, _: &mut Context<Self>) {
        self.open.send_text(text.0.to_owned());
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().as_ref().and_then(ClipboardItem::text) {
            self.open.paste(text);
        }
    }

    /// Copies what's selected, which only the thread client-core keeps the terminal's copy on can read, so it goes on
    /// the clipboard once that thread has said what it is. Nothing selected copies nothing, and leaves the clipboard as
    /// it was.
    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        let selected = self.open.selected_text();
        cx.spawn(async move |_, cx| {
            if let Ok(text) = selected.recv_async().await {
                cx.update(|cx| cx.write_to_clipboard(ClipboardItem::new_string(text)));
            }
        })
        .detach();
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, _: &mut Context<Self>) {
        self.open.select_all();
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

#[cfg(feature = "testing")]
impl TerminalView {
    /// Each row it last drew, as text, with a space for each cell that draws nothing.
    pub fn rows(&self) -> Vec<String> {
        let Some(frame) = &self.frame else { return Vec::new() };
        frame.rows.iter().map(|row| row.text()).collect()
    }

    /// Whether what it last drew has anything selected.
    pub fn shows_a_selection(&self) -> bool {
        self.frame.as_ref().is_some_and(|frame| frame.rows.iter().any(|row| row.selected.is_some()))
    }

    /// How big each cell is, so a test can point at one. The grid's top left is the view's.
    pub fn cell_size(&self) -> gpui_kit::Size<Pixels> {
        gpui_kit::size(self.metrics.cell_width, self.metrics.line_height)
    }
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
            selection: self.selection,
            pointing: Pointing { open: self.open.clone(), dragging: self.dragging.clone() },
            fits: Box::new(move |size| {
                if fit.replace(Some(size)) != Some(size) {
                    open.fit(size);
                }
            }),
        };
        // Says once it has drawn what the terminal shows, which a picture of it waits for.
        div()
            .id(if self.frame.is_some() { SHOWN_ID } else { "terminal-waiting" })
            .test_support()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus)
            .size_full()
            .overflow_hidden()
            .on_key_down(cx.listener(Self::key_down))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::send_text))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::select_all))
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
