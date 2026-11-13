//! What a build from a checkout says about itself, so that several open at once can be told apart.

#![cfg(test)]

use desktop::checkout::Checkout;
use gpui_kit::TestAppContext;

use crate::support::{Stack, Ui};

#[gpui_kit::test]
fn a_build_from_a_checkout_names_it_and_says_why_it_is_open(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.checkout = Some(Checkout::new("/w/aspen".into(), Some("Check the diff".to_string())));

    let mut ui = Ui::open(cx, &stack);

    assert!(ui.has("checkout-card"));
    assert!(ui.has("checkout-note"));
    assert!(ui.has("version"), "the version still shows");
}

#[gpui_kit::test]
fn a_build_opened_without_a_note_only_names_its_checkout(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.checkout = Some(Checkout::new("/w/aspen".into(), None));

    let mut ui = Ui::open(cx, &stack);

    assert!(ui.has("checkout-card"));
    assert!(!ui.has("checkout-note"));
}

#[gpui_kit::test]
fn a_stamped_build_has_no_checkout_to_name(cx: &mut TestAppContext) {
    let stack = Stack::new();

    let mut ui = Ui::open(cx, &stack);

    assert!(!ui.has("checkout-card"));
}
