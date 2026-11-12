//! Why a build from a checkout quit, said on stderr as it goes. Whoever opened it there, often an agent showing
//! someone a change, sees only that it exited and with what code, which is 0 whether someone quit it or it finished
//! by itself. A stamped build says nothing, since it's the app someone uses, and nobody reads what it prints.

use gpui_kit::{App, Global};

use crate::checkout::Checkout;

/// What the app had done when it quit by itself. With none, someone quit it.
struct Done(&'static str);

impl Global for Done {}

/// Has the app say why it quit, if it's from a checkout, which the line ends by naming as the window's title does.
pub fn say(checkout: Option<&Checkout>, cx: &mut App) {
    say_to(checkout, |line| eprintln!("{line}"), cx);
}

/// Quits because the app has done what it was opened for, such as `look` once its probe is over, and `what` says
/// which, after "since".
pub fn done(what: &'static str, cx: &mut App) {
    cx.set_global(Done(what));
    cx.quit();
}

/// Every way out runs what waits on the app quitting: the menu, its key, the Dock, and closing the window. An app
/// that's killed or that panics says nothing here, and its exit code tells those apart.
fn say_to(checkout: Option<&Checkout>, mut say: impl FnMut(String) + 'static, cx: &mut App) {
    let Some(title) = checkout.map(Checkout::window_title) else { return };
    cx.on_app_quit(move |cx| {
        let why = match cx.try_global::<Done>() {
            Some(Done(what)) => format!("Quit by itself, since {what}"),
            None => "Quit by whoever had it open".to_string(),
        };
        say(format!("{why}: {title}"));
        async {}
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui_kit::TestAppContext;

    use super::*;

    /// What an app from `checkout` says as it quits, after `before` has happened in it.
    fn said(checkout: Option<&Checkout>, before: impl FnOnce(&mut App), cx: &mut TestAppContext) -> Vec<String> {
        let lines = Rc::new(RefCell::new(Vec::new()));
        let heard = lines.clone();
        cx.update(|cx| say_to(checkout, move |line| heard.borrow_mut().push(line), cx));

        cx.update(before);
        cx.update(App::shutdown);

        lines.take()
    }

    fn noted() -> Checkout {
        Checkout::new("/w/aspen".into(), Some("Check the diff".to_string()))
    }

    #[gpui_kit::test]
    fn an_app_someone_quit_says_so_and_which_it_is(cx: &mut TestAppContext) {
        assert_eq!(said(Some(&noted()), |_| {}, cx), ["Quit by whoever had it open: aspen — Check the diff"]);
    }

    #[gpui_kit::test]
    fn an_app_that_quit_by_itself_says_what_it_had_done(cx: &mut TestAppContext) {
        let lines = said(Some(&noted()), |cx| done("its probe finished", cx), cx);

        assert_eq!(lines, ["Quit by itself, since its probe finished: aspen — Check the diff"]);
    }

    #[gpui_kit::test]
    fn a_stamped_build_says_nothing(cx: &mut TestAppContext) {
        assert_eq!(said(None, |_| {}, cx), Vec::<String>::new());
    }
}
