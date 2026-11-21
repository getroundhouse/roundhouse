//! Quitting once the commands started on the hub have finished, and what's typed in each chat is kept, so a prompt
//! sent or typed just before quitting still reaches it. Quitting holds the main thread, so the wait doesn't need that
//! thread. It's the app's rather than the view's, since closing the window quits, and takes the view with it. That
//! wait is short, so quitting from the menu, from Cmd-Q or by closing the window first waits a little longer for
//! what's on its way to the hub, then asks about whatever quitting would still lose. A quit macOS starts, such as from
//! the Dock, can't be held back.

use std::time::{Duration, Instant};

use client_core::{Client, Commands, Drafts};
use gpui_kit::{App, AppContext as _, Context, Global, WeakEntity, Window};

use super::{AppView, restarts};
use crate::asking::{self, Question};
use crate::window_place;

/// The longest quitting, or restarting to update, waits for what's on its way to the hub, such as a file attached just
/// before, before asking about whatever it would still lose. GPUI gives the app's own wait as it quits a tenth of this.
/// *Unconfirmed.*
pub(super) const KEEPING_WAIT: Duration = Duration::from_secs(2);

/// How long quitting, or restarting to update, waits for what's on its way to the hub: [`KEEPING_WAIT`], unless a test
/// has said otherwise.
#[cfg(any(test, feature = "testing"))]
fn keeping_wait(cx: &App) -> Duration {
    cx.try_global::<StandIn>().map_or(KEEPING_WAIT, |StandIn(wait)| *wait)
}

#[cfg(not(any(test, feature = "testing")))]
fn keeping_wait(_: &App) -> Duration {
    KEEPING_WAIT
}

/// What a test has quitting wait in place of [`KEEPING_WAIT`].
#[cfg(any(test, feature = "testing"))]
struct StandIn(Duration);

#[cfg(any(test, feature = "testing"))]
impl Global for StandIn {}

/// Has quitting, and restarting to update, wait at most `wait` from now on, for a test whose hub is held still until
/// they've given up on it. The wait is by the system's clock rather than a GPUI timer, so a test can't move it, and would
/// otherwise sit through all of [`KEEPING_WAIT`].
#[cfg(any(test, feature = "testing"))]
pub fn the_keeping_wait_is(wait: Duration, cx: &mut App) {
    cx.set_global(StandIn(wait));
}

/// How far quitting has got.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Quitting {
    #[default]
    No,
    /// Waiting for what's on its way to the hub before quitting or restarting, or asking whether to quit.
    Waiting,
    /// Gone ahead, so the system quits the app from here.
    Quit,
}

/// The current connection's commands, and what keeps drafts on whichever connection there is. Commands on a
/// connection that has gone failed when it went.
struct Started(std::collections::HashMap<gpui_kit::EntityId, (Commands, Drafts)>);

impl Global for Started {}

/// The view that quitting asks first, for Quit in the menu and Cmd-Q, which aren't the view's.
struct Quits(WeakEntity<AppView>);

impl Global for Quits {}

/// Quits as [`AppView::quit`] says, or at once without the app's view.
pub(crate) fn quit(cx: &mut App) {
    match cx.try_global::<Quits>().and_then(|quits| quits.0.upgrade()) {
        Some(view) => view.update(cx, AppView::quit),
        None => cx.quit(),
    }
}

/// Has Quit in the menu, Cmd-Q and closing the window go through [`AppView::quit`].
pub(super) fn ask_first(window: &Window, cx: &mut Context<AppView>) {
    let view = cx.weak_entity();
    cx.set_global(Quits(view.clone()));
    window
        .on_window_should_close(cx, move |window, cx| view.update(cx, |t, cx| t.may_close(window, cx)).unwrap_or(true));
}

impl AppView {
    fn all_unsent(&self, cx: &Context<Self>) -> (usize, usize) {
        super::hubs::other_views(cx.entity_id(), cx).iter().fold(self.unsent(cx), |(a, b), view| {
            let (c, d) = view.read(cx).unsent(cx);
            (a + c, b + d)
        })
    }

    /// Quits, as Cmd-Q, Quit in the menu and closing the window do, once what's on its way to the hub has reached it,
    /// as [`Self::once_kept`] waits for, or else once the person says to.
    /// Quitting again while it waits or asks changes nothing.
    pub fn quit(&mut self, cx: &mut Context<Self>) {
        self.once_kept(cx, |t, unsent, cx| match unsent {
            (0, 0) => t.quit_now(cx),
            (chats, comments) => t.ask_to_quit(chats, comments, cx),
        });
    }

    /// Whether the window can close at once, which quits the app, because nothing would be lost. If something would,
    /// it stays open, and quitting goes as [`Self::quit`] says.
    pub(super) fn may_close(&mut self, window: &Window, cx: &mut Context<Self>) -> bool {
        if self.quitting == Quitting::Quit || self.all_unsent(cx) == (0, 0) {
            window_place::last_look(window, cx);
            return true;
        }
        self.quit(cx);
        false
    }

    /// Waits, at most [`KEEPING_WAIT`], for what's been typed and attached to reach the hub, and for prompts and other
    /// calls on their way there, then hands `then` how many chats and comments still have something that going ahead
    /// would lose. With nothing on its way, there's no wait. One waits at a time, so pressing Quit or Restart to update
    /// again meanwhile changes nothing.
    pub(super) fn once_kept(
        &mut self,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, (usize, usize), &mut Context<Self>) + 'static,
    ) {
        if self.quitting != Quitting::No {
            return;
        }
        self.quitting = Quitting::Waiting;
        let pending: Vec<_> = cx.try_global::<Started>().map_or_else(Vec::new, |started| {
            started.0.values().map(|(commands, drafts)| (drafts.flushed(), commands.finished())).collect()
        });
        let wait = keeping_wait(cx);
        cx.spawn(async move |this, cx| {
            // On one of the executor's threads rather than racing a timer, since a test's timers only move when the
            // test moves its clock.
            cx.background_spawn(async move {
                let by = Instant::now() + wait;
                for (flushed, finished) in pending {
                    let _ = flushed.recv_deadline(by);
                    let _ = finished.recv_deadline(by);
                }
            })
            .await;
            let _ = this.update(cx, |t, cx| {
                t.quitting = Quitting::No;
                let unsent = t.all_unsent(cx);
                then(t, unsent, cx);
            });
        })
        .detach();
    }

    /// Asks in a dialog, as archiving anyway does, with Keep working as what Return answers, so it loses nothing.
    /// Quitting is filled in the error's colour, since what's unsent can't be had back.
    fn ask_to_quit(&mut self, chats: usize, comments: usize, cx: &mut Context<Self>) {
        let asked = Question {
            id: "quit",
            title: "Quit Roundhouse?".into(),
            says: restarts::unsent_note(chats, comments, "quitting").unwrap_or_default().into(),
            keep: "Keep working",
            act: "Quit",
            removes: true,
        };
        let (quitting, kept) = (cx.weak_entity(), cx.weak_entity());
        let asking = self.window.update(cx, |_, window, cx| {
            let quit = move |_: &mut Window, cx: &mut App| {
                let _ = quitting.update(cx, AppView::quit_now);
            };
            // However it's answered, after quitting if it was, so keeping working lets quitting ask again.
            let closed = move |_: &mut Window, cx: &mut App| {
                let _ = kept.update(cx, |t, _| {
                    if t.quitting == Quitting::Waiting {
                        t.quitting = Quitting::No;
                    }
                });
            };
            asking::ask_and_hear(asked, window, cx, quit, closed);
        });
        if asking.is_ok() {
            // So quitting again while it asks doesn't ask twice.
            self.quitting = Quitting::Waiting;
        }
    }

    fn quit_now(&mut self, cx: &mut Context<Self>) {
        self.quitting = Quitting::Quit;
        let _ = self.window.update(cx, |_, window, cx| window_place::last_look(window, cx));
        cx.quit();
    }

    /// What keeps drafts on the hub, for a test to wait until nothing is waiting to be kept.
    #[cfg(any(test, feature = "testing"))]
    pub fn drafts(&self) -> Option<Drafts> {
        self.drafts.clone()
    }
}

/// Has quitting wait for the commands started on `client`, rather than on the connection before it, and for `drafts`.
pub(super) fn wait_for(client: &Client, drafts: Drafts, cx: &mut Context<AppView>) {
    if !cx.has_global::<Started>() {
        let app: &mut App = cx;
        app.on_app_quit(|cx| {
            let started: Vec<_> = cx.try_global::<Started>().map_or_else(Vec::new, |started| {
                started.0.values().map(|(commands, drafts)| (commands.finished(), drafts.flushed())).collect()
            });
            async move {
                for (finished, flushed) in started {
                    let _ = flushed.recv_async().await;
                    let _ = finished.recv_async().await;
                }
            }
        })
        .detach();
    }
    let id = cx.entity_id();
    let mut pending =
        cx.try_global::<Started>().map_or_else(std::collections::HashMap::new, |started| started.0.clone());
    pending.insert(id, (client.commands(), drafts));
    cx.set_global(Started(pending));
}

/// A new window has no outstanding work from views in a window that has closed.
pub(super) fn new_window(cx: &mut App) {
    if cx.has_global::<Started>() {
        cx.set_global(Started(std::collections::HashMap::new()));
    }
}
