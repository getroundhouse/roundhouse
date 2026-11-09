//! Code's colours, worked out once for each piece of code and kept, since a call's code is drawn on every frame
//! while it's open.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use gpui_kit::{HighlightStyle, SharedString};

/// Above this, code isn't highlighted, since it's highlighted where it's drawn.
const UP_TO: usize = 16 * 1024;
/// How many highlighted pieces of code are kept.
const KEPT: usize = 256;

pub(super) type Spans = Rc<[(Range<usize>, HighlightStyle)]>;

thread_local! {
    static KEPT_SPANS: RefCell<HashMap<(&'static str, SharedString), Spans>> = RefCell::new(HashMap::new());
}

/// The colours for `text` as `language`. Until the highlighter has loaded, which it does at launch, there are none,
/// and nothing's kept, so the code is coloured once it has.
pub(super) fn spans(language: &'static str, text: &SharedString) -> Spans {
    if text.len() > UP_TO {
        return Rc::from(Vec::new());
    }
    KEPT_SPANS.with_borrow_mut(|kept| {
        if let Some(spans) = kept.get(&(language, text.clone())) {
            return spans.clone();
        }
        let Some(spans) = look::try_highlight(language, text) else { return Rc::from(Vec::new()) };
        if kept.len() >= KEPT {
            kept.clear();
        }
        let spans: Spans = spans.into();
        kept.insert((language, text.clone()), spans.clone());
        spans
    })
}
