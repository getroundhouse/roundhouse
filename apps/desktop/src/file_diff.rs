//! One changed file's diff, as the workspace's file tab shows it, with the comments on it and the one being written, or
//! a file that isn't text, as an [`ImageDiff`]. Which file is the pane's to say, since the file tab is the pane's, and
//! the files and comments are the workspace's [`Review`]'s.
//!
//! The app draws the diff itself, rather than with the kit's `Diff`, which shows only the lines in git's patch, and
//! keeps what's open, where it's scrolled to and what's selected with what it draws, so a diff loaded again lost them.
//! Here what's open of each file, and where the list is, are this view's, and the lines git left out are read from
//! the file as it was. `docs/gpui.md` has why.

use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext, Context, DispatchPhase, Entity, EventEmitter, FocusHandle, Focusable, Hsla,
    InteractiveElement as _, IntoElement, MouseButton, MouseMoveEvent, MouseUpEvent, ParentElement, Render,
    SharedString, Styled, Subscription, Window, canvas, div, font, px,
};

use crate::FlipImage;
use crate::file_header::file_header;
use crate::file_view::{LINE_HEIGHT, numbers_width};
use crate::image_diff::{self, ImageDiff};
use crate::patch::FilePatch;
use crate::review::{Review, ReviewEvent};
pub use draft::{Draft, LineAt};
use draw::{Drawing, FOLD_HEIGHT, SIGN};
use layout::{Note, Old, Row, Shape};
use list::{Code, DiffList, Height, Place};
use opened::Opened;
pub(crate) use selection::KEY_CONTEXT;
use selection::{DrawnLines, Selection};
use shown::{Making, Shown};
use sides::Extent;
pub use view::DiffView;

mod annotations;
mod draft;
mod draw;
mod layout;
mod list;
mod opened;
mod selection;
mod shown;
mod sides;
mod view;

#[derive(Clone, Debug)]
pub enum FileDiffEvent {
    /// Put this in the chat's composer.
    AddToChat(String),
}

pub struct FileDiff {
    review: Entity<Review>,
    /// The file the file tab shows, while it's open.
    pub path: Option<SharedString>,
    shown: Option<Shown>,
    /// The comment being written. It's kept while another file shows, and shows again with its own.
    pub draft: Option<Draft>,
    /// What shows in place of the lines for a file that isn't text, which has none.
    pub image: Entity<ImageDiff>,
    /// What's been opened of each file that's shown, by path, which stays open while other files show.
    opened: HashMap<SharedString, Opened>,
    list: DiffList,
    /// The rows as they were last laid out, and whether what they're made from has changed since.
    rows: Rc<[Row]>,
    stale: bool,
    making: Making,
    selection: Option<Selection>,
    /// The button that started the selection is still down.
    selecting: bool,
    drawn: DrawnLines,
    /// Where keys go while the file tab shows, when no comment is being written, so the window's keys reach the app.
    focus: FocusHandle,
    _subscriptions: [Subscription; 3],
}

impl EventEmitter<FileDiffEvent> for FileDiff {}

impl Focusable for FileDiff {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl FileDiff {
    pub fn new(review: Entity<Review>, cx: &mut Context<Self>) -> Self {
        // Comments show as they come and go, and the file again once the diff has loaded again.
        let observing = cx.observe(&review, |t, _, cx| t.lay_out_again(cx));
        let loading = cx.subscribe(&review, |t, _, event: &ReviewEvent, cx| {
            if matches!(event, ReviewEvent::Loaded) {
                t.show_file(cx);
            }
        });
        // Every workspace's diff shows as the one chosen last.
        let choosing = cx.observe_global::<view::Chosen>(Self::lay_out_again);
        let image = cx.new(|cx| ImageDiff::new(review.clone(), cx));
        Self {
            review,
            path: None,
            shown: None,
            draft: None,
            image,
            opened: HashMap::new(),
            list: DiffList::new(cx),
            rows: Rc::from([]),
            stale: true,
            making: Making::default(),
            selection: None,
            selecting: false,
            drawn: DrawnLines::default(),
            focus: cx.focus_handle(),
            _subscriptions: [observing, loading, choosing],
        }
    }

    /// Shows a file, from its top, in place of the one it showed, or none, as the file tab says.
    pub fn show(&mut self, path: Option<SharedString>, cx: &mut Context<Self>) {
        if self.path != path {
            self.path = path;
            self.rows = Rc::from([]);
            self.selection = None;
            self.list.go_to_start();
            self.stale = true;
            self.image.update(cx, |image, cx| image.show(self.path.clone(), cx));
            self.show_file(cx);
        }
        cx.notify();
    }

    /// The file it shows, as the diff last loaded has it. `None` once it no longer changes anything.
    pub fn file<'a>(&self, cx: &'a App) -> Option<&'a FilePatch> {
        let path = self.path.as_ref()?;
        self.review.read(cx).file(path)
    }

    /// How far its code is scrolled sideways.
    #[cfg(feature = "testing")]
    pub fn scrolled_across(&self) -> gpui_kit::Pixels {
        self.list.across()
    }

    /// The text of line `new` of the new side, if a row shows it.
    #[cfg(feature = "testing")]
    pub fn shows_line(&self, new: u32) -> Option<SharedString> {
        let shows = self
            .rows
            .iter()
            .any(|row| matches!(row, Row::Line { new: Some(n), .. } | Row::Pair { new: Some(n), .. } if *n == new));
        let line = self.shown.as_ref()?.sides.new.line(new).filter(|_| shows)?;
        Some(line.text.clone())
    }

    /// How it shows a diff, as chosen last in any workspace.
    #[cfg(feature = "testing")]
    pub fn shown_as(&self, cx: &App) -> DiffView {
        view::chosen(cx)
    }

    /// Opens `lines` of the file that shows, by their numbers on the old side.
    fn open(&mut self, lines: Range<u32>, cx: &mut Context<Self>) {
        let Some(path) = self.path.clone() else { return };
        let commit = self.review.read(cx).base_commit.clone();
        self.opened.entry(path).or_default().open(commit.as_ref(), lines);
        self.lay_out_again(cx);
    }

    /// Lays the rows out again before they're next drawn, after something they're made from has changed.
    fn lay_out_again(&mut self, cx: &mut Context<Self>) {
        self.stale = true;
        cx.notify();
    }

    /// The rows, laid out again if anything they're made from has changed, keeping the row at the top where it is.
    /// Lines of the file as it was that they'd show, and haven't been read, are read, and show once they have been.
    fn rows(&mut self, cx: &mut Context<Self>) -> Rc<[Row]> {
        if !std::mem::take(&mut self.stale) {
            return self.rows.clone();
        }
        let Some(shown) = &self.shown else { return Rc::from([]) };
        let (view, notes, commit) = (view::chosen(cx), self.notes(cx), self.review.read(cx).base_commit.clone());
        let none = Opened::default();
        let opened = self.path.as_ref().and_then(|path| self.opened.get(path)).unwrap_or(&none);
        let known = shown.sides.known().unwrap_or_default();
        let old = match shown.sides.extent {
            Extent::Hunks => Old::Hunks,
            Extent::Whole(lines) => Old::Whole(lines),
            Extent::InPart(lines) => Old::InPart { lines, known: &known },
        };
        let shape = Shape {
            file: &shown.file,
            old,
            opened,
            commit: commit.as_ref(),
            split: view.split(),
            everything: view.everything(),
            notes: &notes,
        };
        let laid = layout::rows(&shape);
        let rows: Rc<[Row]> = laid.rows.into();
        let top = self.list.top();
        let place = layout::same_row(&self.rows, &rows, top.row)
            .map_or_else(Place::default, |row| Place { row, offset: top.offset });
        self.list.set_rows(rows.iter().map(height).collect(), place);
        let was = std::mem::replace(&mut self.rows, rows.clone());
        self.keep_selection(&was, &rows);
        self.read_lines(&laid.missing);
        rows
    }

    /// The comments on the file, and the one being written if it's on this file. One being edited shows only as
    /// it's being written.
    fn notes(&self, cx: &App) -> Vec<Note> {
        let Some(path) = self.path.as_ref() else { return Vec::new() };
        let editing = self.draft.as_ref().and_then(|draft| draft.editing.as_ref());
        let comments =
            self.review.read(cx).comments.iter().filter(|c| c.path == path.as_ref() && editing != Some(&c.id));
        let mut notes: Vec<Note> =
            comments.map(|c| Note { side: c.side, line: c.line, row: Row::Comment(c.id.clone()) }).collect();
        if let Some(draft) = self.draft.as_ref().filter(|draft| &draft.position.path == path) {
            notes.push(Note { side: draft.position.side, line: draft.position.line, row: Row::Draft });
        }
        notes
    }

    /// The file's lines, in the list, in the code's font.
    fn lines(&mut self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let rows = self.rows(cx);
        self.drawn.borrow_mut().clear();
        let Some(shown) = &self.shown else { return div().into_any_element() };
        if rows.is_empty() {
            return placeholder("Nothing in this file's lines changed", cx.theme().muted_foreground);
        }
        let theme = cx.theme();
        let (mono, size) = (theme.mono_font_family.clone(), theme.mono_font_size);
        let split = view::chosen(cx).split();
        let numbers =
            numbers_width(usize::try_from(highest(&shown.file, shown.sides.extent.lines())).unwrap_or(usize::MAX));
        let gutter = if split { numbers } else { numbers * 2. };
        let advance = window.text_system().advance(window.text_system().resolve_font(&font(mono.clone())), size, 'm');
        let widest = advance.map_or(px(0.), |advance| advance.width) * count(shown.sides.widest) + SIGN;
        self.list.set_code(Code { widest, columns: if split { 2 } else { 1 }, gutter: gutter + SIGN });
        let review = self.review.read(cx);
        let drawing = Drawing {
            sides: shown.sides.clone(),
            path: self.path.clone().unwrap_or_default(),
            numbers,
            gutter,
            marked: self.draft.as_ref().map(|draft| (draft.position.side, draft.position.line)),
            comments: review.comments.clone(),
            draft: self.draft.as_ref().map(|draft| (draft.input.clone(), draft.editing.is_some())),
            across: self.list.across(),
            selection: self.selection,
            drawn: self.drawn.clone(),
            view: cx.weak_entity(),
        };
        let list = self.list.render("diff-lines", move |ix, _, cx| match rows.get(ix) {
            Some(row) => draw::row(ix, row, &drawing, cx),
            None => div().into_any_element(),
        });
        div()
            .relative()
            .size_full()
            .font_family(mono)
            .text_size(size)
            .line_height(LINE_HEIGHT)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::select_from))
            .child(list)
            .child(Self::follow_the_pointer(cx))
            .into_any_element()
    }

    /// Takes the selection on with the pointer while its button is down, wherever the pointer goes, and stops once
    /// it comes up.
    fn follow_the_pointer(cx: &Context<Self>) -> AnyElement {
        let view = cx.weak_entity();
        let painted = move |_, (), window: &mut Window, _: &mut App| {
            let (moving, lifting) = (view.clone(), view.clone());
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                if phase == DispatchPhase::Bubble && event.pressed_button == Some(MouseButton::Left) {
                    let _ = moving.update(cx, |t, cx| t.select_to(event.position, cx));
                }
            });
            window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                if phase == DispatchPhase::Bubble {
                    let _ = lifting.update(cx, |t, _| t.stop_selecting());
                }
            });
        };
        div().absolute().size_0().child(canvas(|_, _, _| (), painted)).into_any_element()
    }
}

/// How tall a row is drawn.
fn height(row: &Row) -> Height {
    match row {
        Row::Line { .. } | Row::Pair { .. } => Height::Fixed(LINE_HEIGHT),
        Row::Fold(_) => Height::Fixed(FOLD_HEIGHT),
        Row::Comment(id) => Height::Measured(format!("comment-{id}").into()),
        Row::Draft => Height::Measured("draft".into()),
    }
}

/// The highest line number the file's diff can show, which the columns of numbers are as wide as.
fn highest(file: &FilePatch, whole: Option<u32>) -> u32 {
    let last = file.hunks().last().map_or((1, 1), |hunk| (hunk.old_end(), hunk.new_end()));
    let after = whole.map_or(0, |length| length.saturating_add(1).saturating_sub(last.0));
    last.0.max(last.1) + after
}

fn placeholder(text: &'static str, color: Hsla) -> AnyElement {
    div().size_full().flex().items_center().justify_center().text_color(color).child(text).into_any_element()
}

#[expect(clippy::cast_precision_loss, reason = "a line has far fewer characters than a float counts exactly")]
fn count(characters: usize) -> f32 {
    characters as f32
}

impl Render for FileDiff {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let loaded = self.review.read(cx).files.is_some();
        let file = self.file(cx).cloned();
        let binary = file.as_ref().is_some_and(FilePatch::is_binary);
        let content = match file {
            // The image's comparison has a bar of its own, with its choices on it.
            Some(_) if binary => self.image.clone().into_any_element(),
            Some(file) if self.shown.is_some() => {
                let renamed = file.old_path().filter(|old| *old != file.path()).map(|old| format!("from {old}"));
                v_flex()
                    .size_full()
                    .child(file_header(file.path(), renamed, Some(view::switch(cx)), cx))
                    .child(div().flex_1().min_h_0().child(self.lines(window, cx)))
                    .into_any_element()
            }
            None if loaded => placeholder("This file has no changes now", muted),
            Some(_) | None => placeholder("Loading…", muted),
        };
        // Space flips a changed image, while it's flipped between its two sides. Only for a file that isn't text,
        // which has no lines to comment on, since the binding would take Space from what a comment is typed in.
        div()
            .track_focus(&self.focus)
            .when(binary, |root| {
                root.key_context(image_diff::KEY_CONTEXT)
                    .on_action(cx.listener(|t, _: &FlipImage, _, cx| t.image.update(cx, ImageDiff::flip)))
            })
            .when(!binary, |root| {
                root.key_context(KEY_CONTEXT)
                    .on_action(cx.listener(Self::copy))
                    .on_action(cx.listener(Self::select_all))
            })
            .size_full()
            .child(content)
    }
}
