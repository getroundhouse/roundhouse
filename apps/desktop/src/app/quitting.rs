//! Quitting once the commands started on the hub have finished, and what's typed in each chat is kept, so a prompt
//! sent or typed just before quitting still reaches it. Quitting holds the main thread, so the wait doesn't need that
//! thread. It's the app's rather than the view's, since closing the window quits, and takes the view with it. That
//! wait is short, so quitting from the menu, from Cmd-Q or by closing the window first waits a little longer for
//! what's on its way to the hub, then asks about whatever quitting would still lose. A quit macOS starts, such as from
//! the Dock, can't be held back.

use std::time::{Duration, Instant};

use client_core::{Client, Commands, Drafts};
use gpui_kit::{App, AppContext as _, Context, Global, PromptLevel, WeakEntity, Window};

use super::{AppView, view};
use crate::window_place;
use crate::workspace_pane::WorkspacePane;

/// The longest quitting, or restarting to update, waits for what's on its way to the hub, such as a file attached just
/// before, before asking about whatever it would still lose. GPUI gives the app's own wait as it quits a fifth of this.
/// *Unconfirmed.*
pub(super) const KEEPING_WAIT: Duration = Duration::from_secs(2);

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
struct Started(Commands, Drafts);

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

/// Has each pane show a draft made elsewhere that it held back while one from here was on its way, once that one has
/// reached the hub.
fn follow_drafts(drafts: &Drafts, cx: &mut Context<AppView>) {
    let caught_up = drafts.caught_up().clone();
    cx.spawn(async move |this, cx| {
        while caught_up.recv_async().await.is_ok() {
            let shown = this.update(cx, |t, cx| {
                for pane in t.panes.values() {
                    pane.update(cx, WorkspacePane::take_drafts_from_elsewhere);
                }
            });
            if shown.is_err() {
                return;
            }
        }
    })
    .detach();
}

/// Has Quit in the menu, Cmd-Q and closing the window go through [`AppView::quit`].
pub(super) fn ask_first(window: &Window, cx: &mut Context<AppView>) {
    let view = cx.weak_entity();
    cx.set_global(Quits(view.clone()));
    window
        .on_window_should_close(cx, move |window, cx| view.update(cx, |t, cx| t.may_close(window, cx)).unwrap_or(true));
}

impl AppView {
    /// Keeps drafts on `client`'s hub from here on, carrying on with any the last connection couldn't, and has quitting
    /// wait for them.
    pub(super) fn keep_drafts_on(&mut self, client: &Client, cx: &mut Context<Self>) {
        let drafts = if let Some(drafts) = &self.drafts {
            drafts.reconnect(client);
            drafts.clone()
        } else {
            let drafts = Drafts::start(client);
            follow_drafts(&drafts, cx);
            self.drafts.insert(drafts).clone()
        };
        wait_for(client, drafts, cx);
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
        if self.quitting == Quitting::Quit || self.unsent(cx) == (0, 0) {
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
        let flushed = self.drafts.as_ref().map(Drafts::flushed);
        let finished = self.client().map(|client| client.commands().finished());
        cx.spawn(async move |this, cx| {
            // On one of the executor's threads rather than racing a timer, since a test's timers only move when the
            // test moves its clock.
            cx.background_spawn(async move {
                let by = Instant::now() + KEEPING_WAIT;
                let _ = flushed.map(|flushed| flushed.recv_deadline(by));
                let _ = finished.map(|finished| finished.recv_deadline(by));
            })
            .await;
            let _ = this.update(cx, |t, cx| {
                t.quitting = Quitting::No;
                let unsent = t.unsent(cx);
                then(t, unsent, cx);
            });
        })
        .detach();
    }

    /// Asks in the system's alert, as archiving anyway does, with keeping working first, so Return loses nothing.
    fn ask_to_quit(&mut self, chats: usize, comments: usize, cx: &mut Context<Self>) {
        let detail = view::unsent_note(chats, comments, "quitting").unwrap_or_default();
        let answer = self.window.update(cx, |_, window, cx| {
            window.prompt(PromptLevel::Warning, "Quit Roundhouse?", Some(&detail), &["Keep working", "Quit"], cx)
        });
        let Ok(answer) = answer else { return };
        // So quitting again while it asks doesn't ask twice.
        self.quitting = Quitting::Waiting;
        cx.spawn(async move |this, cx| {
            let quit = answer.await == Ok(1);
            let _ = this.update(cx, |t, cx| if quit { t.quit_now(cx) } else { t.quitting = Quitting::No });
        })
        .detach();
    }

    fn quit_now(&mut self, cx: &mut Context<Self>) {
        self.quitting = Quitting::Quit;
        let _ = self.window.update(cx, |_, window, cx| window_place::last_look(window, cx));
        cx.quit();
    }

    /// What keeps drafts on the hub, for a test to wait until nothing is waiting to be kept.
    #[cfg(feature = "testing")]
    pub fn drafts(&self) -> Option<Drafts> {
        self.drafts.clone()
    }

    /// Client-core's copy of the hub's records, for a test to see what's arrived before any view has.
    #[cfg(feature = "testing")]
    pub fn records(&self) -> Option<client_core::RecordsFeed> {
        self.feed.clone()
    }
}

/// Has quitting wait for the commands started on `client`, rather than on the connection before it, and for `drafts`.
fn wait_for(client: &Client, drafts: Drafts, cx: &mut App) {
    if !cx.has_global::<Started>() {
        cx.on_app_quit(|cx| {
            let started = cx.try_global::<Started>().map(|started| (started.0.finished(), started.1.flushed()));
            async move {
                if let Some((finished, flushed)) = started {
                    let _ = flushed.recv_async().await;
                    let _ = finished.recv_async().await;
                }
            }
        })
        .detach();
    }
    cx.set_global(Started(client.commands(), drafts));
}
