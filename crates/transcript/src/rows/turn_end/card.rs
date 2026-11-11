//! The card a finished turn's line shows while the pointer rests on it: which models the agent's replies came from and
//! which agent it is, when the turn started and ended and how long it took, and what the agent's own requests asked
//! of the models, in tokens. Its subagents' are left out. It says only what the turn's end recorded, so a part it
//! doesn't say is left out.

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Div, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div, px,
};
use look::{InRegion as _, Region};
use protocol::event::TokenUsage;
use protocol::model::{AgentKind, ModelName};

use super::words::{self, Moment};

/// Wide enough for what each row says to sit apart from its name.
const MIN_WIDTH: Pixels = px(220.);

/// What a finished turn's end recorded, for its card.
pub(super) struct TurnCard {
    pub(super) agent: AgentKind,
    pub(super) models: Vec<ModelName>,
    /// By the hub's clock. `None` from a hub too old to say.
    pub(super) started_at_ms: Option<i64>,
    pub(super) ended_at_ms: i64,
    /// How long the turn took, as its line says it.
    pub(super) took: Option<String>,
    pub(super) tokens: Option<TokenUsage>,
}

impl TurnCard {
    /// Raised like the app's other cards. What answered is the heading, in medium weight, with the day under it, then
    /// a table of when the turn started and ended and how long it took, then one of what it asked of the models, each
    /// under a rule.
    pub(super) fn card(&self, cx: &App) -> AnyElement {
        let heading = words::heading(&self.models, self.agent);
        let times = Moment::at(self.ended_at_ms, cx).map(|ended| {
            let started = self.started_at_ms.and_then(|at_ms| Moment::at(at_ms, cx));
            words::times(started.as_ref(), &ended, self.took.clone())
        });
        let day = times.as_ref().and_then(|times| times.day.clone());
        let times = times.map(|times| table(times.rows, cx));
        let counts = self.tokens.map(|tokens| table(counts(tokens), cx));
        look::raised(cx)
            .region(Region::TurnCard)
            .flex()
            .flex_col()
            .min_w(MIN_WIDTH)
            .px_3()
            .py_2p5()
            .gap_1()
            .text_sm()
            .children(heading.map(|heading| div().font_weight(FontWeight::MEDIUM).child(heading)))
            .children(day.map(|day| div().whitespace_nowrap().text_color(cx.theme().muted_foreground).child(day)))
            .children(times)
            .children(counts)
            .into_any_element()
    }
}

/// What the agent's own requests asked of the models: what they read fresh, what the models wrote, and what they
/// read from the cache and wrote to it. Counted as the context's card counts.
fn counts(tokens: TokenUsage) -> Vec<(&'static str, String)> {
    let counts = [
        ("Input", tokens.input_tokens),
        ("Output", tokens.output_tokens),
        ("Cache read", tokens.cache_read_tokens),
        ("Cache write", tokens.cache_write_tokens),
    ];
    counts.into_iter().map(|(name, count)| (name, look::tokens(count))).collect()
}

/// Rows under a rule, each a name and what it says.
fn table(rows: Vec<(&'static str, String)>, cx: &App) -> Div {
    let rule = div().my_1().h(px(1.)).bg(cx.theme().border);
    v_flex().gap_1().child(rule).children(rows.into_iter().map(|(name, said)| row(name, said, cx)))
}

/// A name and what it says, at either end of a line, said together to assistive apps.
fn row(name: &'static str, said: String, cx: &App) -> impl IntoElement {
    h_flex()
        .id(name)
        .justify_between()
        .gap_6()
        .aria_label(format!("{name} {said}"))
        .child(div().text_color(cx.theme().muted_foreground).child(name))
        .child(div().whitespace_nowrap().child(said))
        .test_support()
}
