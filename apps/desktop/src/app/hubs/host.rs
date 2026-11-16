//! Each hub has its own view, including drafts and outstanding callbacks.

use std::collections::HashMap;

use client_core::RemoteHub;
use gpui_kit::component::WindowExt as _;
use gpui_kit::{App, AppContext as _, Context, Entity, Global, IntoElement, Render, WeakEntity, Window};
use protocol::pairing::Fingerprint;

use crate::app::{AppView, HubSource, quitting};
use crate::checkout::Checkout;

pub(crate) struct Hubs {
    local: HubSource,
    checkout: Option<Checkout>,
    pub(super) selected: Option<Fingerprint>,
    pub(super) views: HashMap<Option<Fingerprint>, Entity<AppView>>,
    intent: u64,
}

struct OpenHubs(WeakEntity<Hubs>);
impl Global for OpenHubs {}

impl Hubs {
    pub(crate) fn new(
        local: HubSource,
        checkout: Option<Checkout>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        quitting::new_window(cx);
        let weak = cx.weak_entity();
        cx.set_global(OpenHubs(weak));
        let view = cx.new(|cx| AppView::new(local.clone(), checkout.clone(), window, cx));
        let settings = crate::settings::get(cx);
        if let Some(saved) =
            settings.hubs.into_iter().find(|hub| Some(&hub.fingerprint) == settings.selected_hub.as_ref())
        {
            let weak = view.downgrade();
            let handle = window.window_handle();
            cx.defer(move |cx| {
                let initial = cx
                    .try_global::<OpenHubs>()
                    .and_then(|host| host.0.upgrade())
                    .is_some_and(|host| host.read(cx).intent == 0);
                if initial {
                    let _ = handle.update(cx, |_, window, cx| crate::app::saved_hubs::open(saved, weak, window, cx));
                }
            });
        }
        Self { local, checkout, selected: None, views: HashMap::from([(None, view)]), intent: 0 }
    }

    pub(crate) fn active(&self) -> Entity<AppView> {
        self.views[&self.selected].clone()
    }

    fn select(&mut self, remote: Option<RemoteHub>, window: &mut Window, cx: &mut Context<Self>) {
        let key = remote.as_ref().map(|hub| hub.fingerprint.clone());
        // Pairing renews authorization, not the hub's identity. Keep the view and its unsent drafts.
        if let Some(view) = self.views.get(&key) {
            view.update(cx, |view, cx| {
                let renewed =
                    view.hub.remote.as_ref().map(|hub| &hub.device_id) != remote.as_ref().map(|hub| &hub.device_id);
                view.hub.remote.clone_from(&remote);
                if renewed {
                    view.connect(cx);
                }
            });
        }
        if key == self.selected {
            return;
        }
        window.close_all_dialogs(cx);
        self.active().update(cx, |view, cx| {
            view.visible = false;
            if let Some(dialog) = &view.new_workspace {
                dialog.update(cx, crate::new_workspace::NewWorkspaceDialog::closed);
            }
            view.read_on_screen(false, cx);
        });
        let source = HubSource { remote, ..self.local.clone() };
        let view = self
            .views
            .entry(key.clone())
            .or_insert_with(|| cx.new(|cx| AppView::new(source, self.checkout.clone(), window, cx)))
            .clone();
        self.selected.clone_from(&key);
        crate::settings::change(cx, |settings| settings.selected_hub = key);
        view.update(cx, |view, cx| {
            view.visible = true;
            view.read_on_screen(window.is_window_active(), cx);
            view.present_pending(window, cx);
            view.focus_what_shows(window, cx);
            view.menus_say = None;
            view.keep_menus_current(cx);
            quitting::ask_first(window, cx);
        });
        cx.notify();
    }
}

impl Render for Hubs {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.active()
    }
}

pub(crate) fn select(remote: Option<RemoteHub>, window: &mut Window, cx: &mut App) {
    if let Some(host) = cx.try_global::<OpenHubs>().and_then(|host| host.0.upgrade()) {
        host.update(cx, |host, cx| {
            host.intent += 1;
            host.select(remote, window, cx);
        });
    }
}

/// An asynchronous credential read or pairing may select only until the next navigation choice.
pub(crate) fn begin_selection(cx: &mut App) -> Option<u64> {
    let host = cx.try_global::<OpenHubs>()?.0.upgrade()?;
    Some(host.update(cx, |host, _| {
        host.intent += 1;
        host.intent
    }))
}

pub(crate) fn finish_selection(intent: Option<u64>, remote: RemoteHub, window: &mut Window, cx: &mut App) {
    if let Some(host) = cx.try_global::<OpenHubs>().and_then(|host| host.0.upgrade()) {
        host.update(cx, |host, cx| {
            if Some(host.intent) == intent {
                host.select(Some(remote), window, cx);
            }
        });
    }
}

pub(in crate::app) fn other_views(current: gpui_kit::EntityId, cx: &App) -> Vec<Entity<AppView>> {
    cx.try_global::<OpenHubs>().and_then(|host| host.0.upgrade()).map_or_else(Vec::new, |host| {
        host.read(cx).views.values().filter(|view| view.entity_id() != current).cloned().collect()
    })
}
