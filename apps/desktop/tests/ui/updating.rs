//! What the app shows about updating itself. Only an app Velopack installed updates for real, so the failure here
//! is made up.

#![cfg(test)]

use desktop::updates::{Kind, Problem};
use gpui_kit::TestAppContext;

use crate::support::{Stack, Ui};

#[gpui_kit::test]
fn a_failed_update_shows_in_the_sidebar(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open(cx, &stack);
    assert!(!ui.has("update-problem"));

    let problem = Problem { kind: Kind::Unreachable, detail: "connection refused".to_string() };
    ui.cx.update(|cx| desktop::updates::show_problem(problem, cx));

    assert!(ui.has("update-problem"));
}
