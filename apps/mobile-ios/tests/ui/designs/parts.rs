//! What the designs share: a screen's frame, clear of a phone's status bar and home indicator, and the pieces several
//! screens draw, each made from the design system's parts.

#![cfg(test)]

use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, Hsla, IntoElement, ParentElement as _, Pixels, SharedString, Styled as _, StyledText, div, px,
};
use look::{
    BarKind, BarSize, ButtonKind, CodeLines, ControlSize, Corner, IconSize, IconSizing as _, Lined as _, RowSize,
    Shaped as _, Space, Type, Typeset as _, Work,
};

/// What of a phone's screen its status bar and its home indicator are drawn over.
pub(super) const STATUS_BAR: Pixels = px(62.);
pub(super) const HOME_INDICATOR: Pixels = px(34.);
/// How far apart the lines of what's read in a chat are, as the app sets them.
pub(super) const PROSE_LINE: Pixels = px(25.);

/// A screen: a column that fills the phone, on the pane's surface.
pub(super) fn screen(cx: &App) -> Div {
    let colors = look::colors(cx);
    v_flex().relative().size_full().bg(colors.pane).text_color(colors.text).typeset(Type::Body)
}

/// The bar across a screen's top, clear of the status bar.
pub(super) fn bar(title: impl Into<SharedString>) -> look::TopBar {
    look::top_bar(title).clear(STATUS_BAR)
}

/// A button that's only an icon, as big as what's tapped is.
pub(super) fn tap(id: &'static str, icon: IconName) -> look::Button {
    look::icon_button(id, ButtonKind::Borderless, ControlSize::Large, icon)
}

pub(super) fn back() -> look::Button {
    tap("back", IconName::ChevronLeft)
}

/// An icon, of a size and a colour.
pub(super) fn icon(name: IconName, size: IconSize, color: Hsla) -> Icon {
    Icon::new(name).sized(size).text_color(color)
}

/// The heading over some of a list's rows, with maybe something at its end.
pub(super) fn heading(words: impl Into<SharedString>, end: Option<AnyElement>, cx: &App) -> Div {
    let muted = look::colors(cx).text_muted;
    h_flex()
        .min_h(RowSize::Small.height())
        .pl(Space::Medium)
        .pr(Space::XSmall)
        .py(Space::XSmall)
        .child(div().flex_1().typeset(Type::Detail).emphasis().text_color(muted).child(words.into()))
        .children(end)
}

/// A name with a line under it, which take the room a row has left.
pub(super) fn names(name: impl IntoElement, under: impl IntoElement, cx: &App) -> Div {
    let muted = look::colors(cx).text_muted;
    v_flex()
        .flex_1()
        .min_w_0()
        .child(div().typeset(Type::Body).truncate().child(name))
        .child(div().typeset(Type::Detail).text_color(muted).truncate().child(under))
}

/// A column of a list's rows, a little in from the screen's edges.
pub(super) fn rows() -> Div {
    v_flex().w_full().px(look::ROWS_INSET)
}

/// A row that sets something, with what it's set to at its end.
pub(super) fn setting(id: &'static str, name: &'static str, set_to: impl IntoElement) -> look::ListRow {
    look::list_row(id, RowSize::Regular).child(div().flex_1().min_w_0().truncate().child(name)).child(set_to)
}

/// A row that opens a page, with an icon for what the page is about, and what the page says at a glance.
pub(super) fn page_row(
    id: &'static str,
    glyph: IconName,
    name: &'static str,
    says: &'static str,
    cx: &App,
) -> AnyElement {
    let colors = look::colors(cx);
    look::list_row(id, RowSize::Regular)
        .child(icon(glyph, IconSize::Large, colors.text_muted))
        .child(div().flex_none().child(name))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_right()
                .truncate()
                .typeset(Type::Detail)
                .text_color(colors.text_muted)
                .child(says),
        )
        .opens()
        .into_any_element()
}

/// The room under a screen's last thing that the home indicator is drawn over.
pub(super) fn foot() -> Div {
    div().flex_none().h(HOME_INDICATOR)
}

/// What stands in the composer's bar for the model and its effort, which opens the picker.
fn model(cx: &App) -> AnyElement {
    let muted = look::colors(cx).text_muted;
    look::button("model", ButtonKind::Borderless, ControlSize::Small, "Opus")
        .after(div().text_color(muted).child("Extra high"))
        .after(icon(IconName::ChevronDown, IconSize::Small, muted))
        .into_any_element()
}

/// How a composer stands.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Composer {
    /// Nothing typed, and nothing going on.
    Empty,
    /// The agent is at work, so Stop is in Send's place.
    Working,
    /// Planning, with something typed.
    Planning,
    /// Hearing what's said, which shows as it's heard.
    Dictating,
    /// With files attached to what's typed.
    Attached,
}

/// The composer's bar: what's added to a prompt, the model, what the chat has used, dictation, and Send, or Stop while
/// the agent works.
fn tools(stands: Composer, typed: bool, cx: &App) -> Div {
    let colors = look::colors(cx);
    let send = match stands {
        Composer::Working => {
            let square = div().size(px(11.)).corners(Corner::Glyph).bg(colors.on_main);
            look::glyph_button("stop", ButtonKind::Main, ControlSize::Regular, square)
        }
        Composer::Empty | Composer::Planning | Composer::Dictating | Composer::Attached => {
            look::icon_button("send", ButtonKind::Main, ControlSize::Regular, IconName::ArrowUp).off(!typed)
        }
    };
    let quiet =
        |id: &'static str, glyph: IconName| look::icon_button(id, ButtonKind::Borderless, ControlSize::Regular, glyph);
    let dictate = match stands {
        Composer::Dictating => quiet("dictate", IconName::AudioLines)
            .icon(icon(IconName::AudioLines, IconSize::Regular, colors.brand))
            .on(true),
        Composer::Empty | Composer::Working | Composer::Planning | Composer::Attached => {
            quiet("dictate", IconName::Mic)
        }
    };
    // What the chat has used of its context, which opens its usage and its account.
    let ring = div().size(px(16.)).rounded_full().border_2().border_color(colors.text_muted);
    let context = look::glyph_button("context", ButtonKind::Borderless, ControlSize::Regular, ring);
    h_flex()
        .gap(Space::XSmall)
        .child(quiet("add", IconName::Plus))
        .child(model(cx))
        .child(div().flex_1())
        .child(context)
        .child(dictate)
        .child(send)
}

/// The composer, as the desktop's is laid out: what's typed, over a bar with what's added to a prompt, the model,
/// dictation, and Send.
pub(super) fn composer(stands: Composer, typed: Option<&'static str>, cx: &App) -> Div {
    let colors = look::colors(cx);
    let (surface, edge) = match stands {
        Composer::Planning => look::plan_colors(colors.inset, cx),
        Composer::Dictating => (colors.inset, colors.brand),
        Composer::Empty | Composer::Working | Composer::Attached => (colors.inset, colors.control_edge),
    };
    let words = match typed {
        // What's heard is muted until it's settled, as the desktop draws it.
        Some(typed) if stands == Composer::Dictating => div().text_color(colors.text_muted).child(typed),
        Some(typed) => div().child(typed),
        None => div().text_color(colors.text_faint).child(match stands {
            Composer::Working => "Send to the agent as it works",
            Composer::Dictating => "Listening…",
            Composer::Empty | Composer::Planning | Composer::Attached => "Ask the agent to do something",
        }),
    };
    let tools = tools(stands, typed.is_some(), cx);
    let attached = (stands == Composer::Attached).then(|| {
        h_flex()
            .relative()
            .flex_wrap()
            .gap(Space::Small)
            .px(Space::Small)
            .child(look::chip(look::ChipKind::File, look::ChipSize::Regular, "sidebar-narrow.png"))
            .child(look::chip(look::ChipKind::File, look::ChipSize::Regular, "hub.log").lines(212, Some(240)))
    });
    let composer = v_flex()
        .relative()
        .w_full()
        .gap(Space::Small)
        .p(Space::Small)
        .corners(Corner::Card)
        .border(look::EDGE)
        .border_color(edge)
        .bg(surface)
        .when(stands == Composer::Planning, |composer| {
            composer.child(div().absolute().inset_0().child(look::plan_texture(cx, Corner::Card)))
        })
        .children(attached)
        .child(div().relative().min_h(px(44.)).px(Space::Small).pt(Space::XSmall).child(words))
        .child(div().relative().child(tools));
    div().w_full().flex_none().px(Space::Medium).pt(Space::Small).pb(Space::Small).child(composer)
}

/// The pull request's bar, as the desktop draws it: its number, where it stands, and what's done next.
pub(super) fn pull_request(
    number: Option<&'static str>,
    work: Work,
    stands: &'static str,
    next: Option<&'static str>,
    cx: &App,
) -> AnyElement {
    let colors = look::colors(cx);
    let color = match work {
        Work::Ready => colors.pr_open,
        Work::Queued => colors.pr_queued,
        Work::Merged => colors.pr_merged,
        Work::Closed => colors.pr_closed,
        Work::Draft => colors.pr_draft,
        Work::Open | Work::NoPr => colors.text,
    };
    let number = number
        .map(|number| look::button("pull-request", ButtonKind::Outlined, ControlSize::Regular, number).of_work(work));
    let bar = look::bar(BarKind::Words, BarSize::Regular)
        .of_work(work)
        .children(number)
        .child(div().min_w_0().truncate().emphasis().text_color(color).child(stands));
    match next {
        Some(next) => bar.end(look::button("next", ButtonKind::Main, ControlSize::Regular, next).of_work(work)),
        None => bar,
    }
    .into_any_element()
}

/// What a chat's screen holds the transcript in: the rest of the screen, with the chat's lines as far apart as the
/// app sets them.
pub(super) fn chat_room() -> Div {
    div().flex_1().min_h_0().w_full().line_height(PROSE_LINE)
}

/// A field as it stands with nothing typed in it, or with `typed`, which a design draws in place of one that's typed in.
pub(super) fn field(glyph: Option<IconName>, typed: Option<&'static str>, placeholder: &'static str, cx: &App) -> Div {
    let colors = look::colors(cx);
    let words = match typed {
        Some(typed) => div().child(typed),
        None => div().text_color(colors.text_faint).child(placeholder),
    };
    h_flex()
        .w_full()
        .h(ControlSize::Large.height())
        .gap(Space::Small)
        .px(Space::Medium)
        .corners(Corner::Control)
        .border(look::EDGE)
        .border_color(colors.control_edge)
        .bg(colors.inset)
        .children(glyph.map(|glyph| icon(glyph, IconSize::Regular, colors.text_muted)))
        .child(words.flex_1().min_w_0().truncate())
}

/// How a line of code stands in what's shown of a file.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Stands {
    Same,
    Added,
    Removed,
}

/// A line of a file's code, after its number, with its sign if it was added or removed. It isn't wrapped, so a long
/// line runs off the screen's end, where a real one would scroll sideways.
pub(super) fn code_line(number: u32, stands: Stands, code: &'static str, path: &str, cx: &App) -> Div {
    let colors = look::colors(cx);
    let (sign, tone) = match stands {
        Stands::Same => ("", None),
        Stands::Added => ("+", Some(colors.added)),
        Stands::Removed => ("−", Some(colors.removed)),
    };
    let text = StyledText::new(code).with_highlights(look::highlight(look::language(path), code));
    h_flex()
        .flex_none()
        .w_full()
        .h(CodeLines::File.height())
        .typeset_code(cx)
        .when_some(tone, |line, tone| line.bg(tone.opacity(0.12)))
        .child(div().flex_none().w(px(38.)).text_right().text_color(colors.text_faint).child(number.to_string()))
        .child(div().flex_none().w(px(22.)).text_center().when_some(tone, gpui_kit::Styled::text_color).child(sign))
        .child(div().flex_none().whitespace_nowrap().child(text))
}

/// A setting: what it's called, over what it does, with its control at its end, and a line under it.
pub(super) fn described(name: &'static str, does: &'static str, control: impl IntoElement, cx: &App) -> Div {
    let muted = look::colors(cx).text_muted;
    h_flex()
        .w_full()
        .gap(Space::Medium)
        .px(Space::Medium)
        .py(Space::Medium)
        .line_under(cx)
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(Space::XSmall)
                .child(name)
                .child(div().typeset(Type::Detail).text_color(muted).child(does)),
        )
        .child(div().flex_none().child(control))
}

/// What's said under a heading, about the rows that follow.
pub(super) fn said(words: &'static str, cx: &App) -> Div {
    let muted = look::colors(cx).text_muted;
    div().px(Space::Medium).pb(Space::Small).typeset(Type::Detail).text_color(muted).child(words)
}
