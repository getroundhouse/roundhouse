//! What a build from a checkout says about itself, so that several open at once can be told apart. It says what the
//! app is told it was built from, and nothing a hub holds, so each window opens on a fake hub.

#![cfg(test)]

use client_core::fake_hub::World;
use desktop::checkout::Checkout;
use gpui_kit::TestAppContext;

use crate::support::{FakeHub, Ui};

#[gpui_kit::test]
fn a_build_from_a_checkout_names_it_and_says_why_it_is_open(cx: &mut TestAppContext) {
    let fake = FakeHub::holding(World::default());
    let checkout = Checkout::new("/w/aspen".into(), Some("Check the diff".to_string()));

    let mut ui = Ui::open_on_from(cx, &fake, Some(checkout));

    assert!(ui.has("checkout-card"));
    assert!(ui.has("checkout-note"));
    assert!(ui.has("version"), "the version still shows");
}

#[gpui_kit::test]
fn a_build_opened_without_a_note_only_names_its_checkout(cx: &mut TestAppContext) {
    let fake = FakeHub::holding(World::default());
    let checkout = Checkout::new("/w/aspen".into(), None);

    let mut ui = Ui::open_on_from(cx, &fake, Some(checkout));

    assert!(ui.has("checkout-card"));
    assert!(!ui.has("checkout-note"));
}

#[gpui_kit::test]
fn a_stamped_build_has_no_checkout_to_name(cx: &mut TestAppContext) {
    let fake = FakeHub::holding(World::default());

    let mut ui = Ui::open_on(cx, &fake);

    assert!(!ui.has("checkout-card"));
}
