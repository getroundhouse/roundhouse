//! A compaction: a rule across the chat where the agent swapped what it held for a summary, saying how much that
//! freed, which opens to the summary it carries on from, with what whoever asked for it said to keep under it.

use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, InteractiveElement as _, IntoElement, ParentElement, Role, StatefulInteractiveElement as _,
    Styled, TestSupportExt as _, div, px, relative,
};
use look::Hand as _;
use protocol::event::{CompactedBy, CompactionOutcome, EventBody};
use protocol::model::Seq;

use super::around::Around;
use super::{RULE_ICON, Shared, colored_line, row, rule_across, rule_middle, turn_times};
use crate::folds::Fold;
use crate::item_key::ItemKey;
use crate::prose::{TextAt, Written};
use crate::scroll_area::ScrollArea;

/// How tall an open summary gets before it scrolls: about fifteen of its lines, a little more than a tool's output, so
/// a long one doesn't push the rest of the chat out of sight.
const SUMMARY_HEIGHT: f32 = 320.0;

/// What a compaction says, and whether it's open to its summary.
struct Compacted<'a> {
    seq: Seq,
    by: CompactedBy,
    before_tokens: Option<u64>,
    after_tokens: Option<u64>,
    took_ms: Option<u64>,
    summary: Option<&'a str>,
    open: bool,
    /// What whoever asked for it said to keep.
    keeping: Option<&'a str>,
}

impl Compacted<'_> {
    /// Who compacted, what it freed and how long it took, with `to` between the token counts and `between` between the
    /// parts.
    fn said(&self, to: &str, between: &str) -> String {
        let freed = freed(self.before_tokens, self.after_tokens, to);
        let took = self.took_ms.map(|ms| turn_times::took(Duration::from_millis(ms)));
        [Some(who(self.by).to_string()), freed, took].into_iter().flatten().collect::<Vec<_>>().join(between)
    }
}

/// Who compacted, as a compaction's rule says it, and the outline's list.
pub(crate) fn who(by: CompactedBy) -> &'static str {
    match by {
        CompactedBy::Agent => "Compacted automatically",
        CompactedBy::Asked | CompactedBy::Unknown => "Compacted",
    }
}

/// How many tokens the context went from and to, with `to` between them, as a compaction's rule says it, and the
/// outline's list, if the agent said both.
pub(crate) fn freed(before_tokens: Option<u64>, after_tokens: Option<u64>, to: &str) -> Option<String> {
    let (before, after) = before_tokens.zip(after_tokens)?;
    Some(format!("{} {to} {} tokens", look::tokens(before), look::tokens(after)))
}

/// The rule, with what it freed and how long it took in the middle. With a summary, the middle opens it.
fn line(compacted: &Compacted, shared: &Shared, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let key = ItemKey::Event(compacted.seq, 0);
    let (open, seq) = (compacted.open, compacted.seq);
    let chevron = if open { IconName::ChevronDown } else { IconName::ChevronRight };
    // Only a summary makes it something to open.
    let opens = compacted.summary.is_some();
    let middle = rule_middle(key.element("compaction"))
        .role(if opens { Role::Button } else { Role::Label })
        // In words, since a screen reader would read the arrow out as one.
        .aria_label(compacted.said("to", ", "))
        .rounded(px(6.))
        .child(Icon::new(IconName::FoldVertical).size(px(RULE_ICON)))
        .child(compacted.said("→", " · "))
        .when(opens, |d| {
            let (this, key) = (shared.this.clone(), key.clone());
            let hover = theme.muted;
            let d = d.aria_expanded(open).hand().hover(move |d| d.bg(hover));
            d.child(Icon::new(chevron).size(px(RULE_ICON))).on_click(move |_, _, cx| {
                let _ = this.update(cx, |t, cx| t.set_open(seq, key.clone(), Fold::Output, !open, cx));
            })
        })
        .test_support();
    let across = rule_across(middle, cx);
    // In the words of whoever asked, in quotes, since they needn't start with "keep".
    let keeping = compacted.keeping.map(|keeping| {
        let said = div().max_w(relative(0.8)).text_center().text_sm().text_color(theme.muted_foreground);
        h_flex().w_full().justify_center().child(said.child(format!("“{keeping}”")))
    });
    let summary = compacted.summary.filter(|_| open).map(|summary| opened(compacted.seq, summary, shared, cx));
    row().pt_2().child(across).children(keeping).children(summary).into_any_element()
}

/// The summary, under a heading that stays put while it scrolls. The scrollbar sits in the gap on the right.
fn opened(seq: Seq, summary: &str, shared: &Shared, cx: &App) -> Div {
    let (theme, key) = (cx.theme(), ItemKey::Event(seq, 0));
    // What a compaction says isn't found. A drag from it scrolls on its area, as the wheel does.
    let area = key.element("summary-scroll");
    let written = Written::unfound(TextAt::only(seq, 0), key.element("summary"), summary).in_area(area.clone());
    let text = shared.prose.draw(&shared.selection, &written, None);
    let text = div().pr_3().text_sm().text_color(theme.muted_foreground).child(text);
    v_flex()
        .w_full()
        .gap_2()
        .py_3()
        .pl_3()
        .pr_1()
        .rounded_lg()
        .border_1()
        .border_color(theme.border)
        .child(div().text_xs().text_color(look::faint()).child("What the agent carries on from"))
        .child(ScrollArea::new(area, px(SUMMARY_HEIGHT), text).follows_drags(&shared.selection))
}

/// The rule for `body`, if it's a compaction, or a line saying why it couldn't.
pub(super) fn of(seq: Seq, body: &EventBody, around: &Around, shared: &Shared, cx: &App) -> AnyElement {
    let EventBody::AgentCompacted { keeping, outcome, .. } = body else {
        return super::empty();
    };
    let (by, before_tokens, after_tokens, took_ms, summary) = match outcome {
        CompactionOutcome::Compacted { by, before_tokens, after_tokens, took_ms, summary } => {
            (*by, *before_tokens, *after_tokens, *took_ms, summary.as_deref())
        }
        CompactionOutcome::Failed { error } => {
            let why = error.as_deref().map_or_else(String::new, |error| format!(" {error}"));
            return colored_line(format!("Couldn't compact the conversation.{why}"), cx.theme().danger);
        }
        CompactionOutcome::Unknown => (CompactedBy::Unknown, None, None, None, None),
    };
    let compacted = Compacted {
        seq,
        by,
        before_tokens,
        after_tokens,
        took_ms,
        summary,
        open: around.open,
        keeping: keeping.as_deref(),
    };
    line(&compacted, shared, cx)
}
