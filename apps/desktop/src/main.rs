use desktop::checkout::{Checkout, take_note};

fn main() {
    desktop::updates::run_hooks();
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    desktop::run(Checkout::of_this_build(take_note(&mut args)));
}
