//! Every spacing of code's lines, in one picture, as `look::CodeLines` has them: each by its name, as a block of the
//! same code at its height, and how far apart its lines are, as the spec's table writes it.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, PlatformTextSystem, Render,
    Styled as _, TestSupportExt as _, Window, div, px, size,
};
use look::{CodeLines, Type, Typeset as _};

use super::capture::Picture;
use super::headless;
use super::places::{self, Places};

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "code-lines";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "look.md";
/// What the spec's table calls the type that names each spacing.
const KIND: &str = "look::CodeLines";
const NAME_WIDTH: f32 = 80.;
const SAMPLE_WIDTH: f32 = 360.;
const WRITTEN_WIDTH: f32 = 60.;
const MARGIN: f32 = 16.;
const BETWEEN: f32 = 16.;
/// The same lines in each, so only how far apart they are differs.
const CODE: [&str; 4] =
    ["pub struct Workspace {", "    /// As the hub last saw it.", "    pub pull_request: Option<PullRequest>,", "}"];

/// A block of `CODE` with its lines `lines` apart, found by its name.
fn sample(lines: CodeLines, cx: &Context<Sheet>) -> AnyElement {
    let colors = look::colors(cx);
    let [name, apart, _] = lines.written();
    let block = v_flex()
        .id(places::id(KIND, &name))
        .w(px(SAMPLE_WIDTH))
        .px(look::Space::Medium)
        .py(look::Space::Small)
        .bg(colors.inset)
        .typeset_code(cx)
        .line_height(lines.height())
        .children(CODE.map(|line| div().child(line)))
        .test_support();
    let label = div().w(px(NAME_WIDTH)).typeset(Type::Detail).text_color(colors.text_muted).child(name);
    let written = div().w(px(WRITTEN_WIDTH)).pl_3().typeset(Type::Detail).text_color(colors.text_faint).child(apart);
    h_flex().items_start().child(label).child(block).child(written).into_any_element()
}

struct Sheet;

impl Render for Sheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        v_flex()
            .size_full()
            .gap(px(BETWEEN))
            .bg(colors.pane)
            .text_color(colors.text)
            .p(px(MARGIN))
            .children(CodeLines::ALL.iter().map(|&lines| sample(lines, cx)))
    }
}

/// At the app's scale, as the screens are, with where it draws each spacing's block.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let mut cx = headless::app(text_system);
    let names: Vec<String> = CodeLines::ALL.iter().map(|lines| lines.written()[0].clone()).collect();
    let tall: f32 = CodeLines::ALL.iter().map(|lines| f32::from(lines.height()) * 4. + 16.).sum();
    let size = size(px(MARGIN * 2. + NAME_WIDTH + SAMPLE_WIDTH + WRITTEN_WIDTH), px(MARGIN * 2. + tall + BETWEEN));
    headless::draw_and_measure(&mut cx, size, None, |_| Sheet, |window| Places::of(window, &[(KIND, &names)]))
}
