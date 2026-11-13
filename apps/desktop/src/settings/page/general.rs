//! General, the first page: how the app behaves wherever it is, and what its hub does whether or not the app is open.

use std::rc::Rc;

use gpui_kit::assets::IconName as Lucide;
use protocol::model::{HubSettings, HubSettingsChange};

use super::{Body, Control, OfHub, Page, PageId, Setting};
use crate::settings::{change, get};

pub(super) fn page(hub: &OfHub) -> Page {
    let always_expand_thinking = Setting {
        name: "Always expand thinking",
        does: "Show what the agent thinks in full, rather than as a line to open.",
        control: Control::Switch {
            is_on: Box::new(|cx| Some(get(cx).always_expand_thinking)),
            set: Rc::new(|on, cx| change(cx, |settings| settings.always_expand_thinking = on)),
        },
    };
    let keep_awake = Setting {
        name: "Keep computer awake while agents are working",
        does: "The display still sleeps, and closing the lid still puts the computer to sleep.",
        control: hub_switch(
            hub,
            |settings| Some(settings.keep_awake),
            |on| HubSettingsChange { keep_awake: Some(on), ..HubSettingsChange::default() },
        ),
    };
    let allow_asked = Setting {
        name: "Skip Claude's safety checks",
        does: "With full access, Claude only stops to ask about commands it flags as possibly destructive, like rm -rf on a path it can't see. This lets those run without asking.",
        control: hub_switch(
            hub,
            |settings| settings.allow_asked_in_full_access,
            |on| HubSettingsChange { allow_asked_in_full_access: Some(on), ..HubSettingsChange::default() },
        ),
    };
    let settings = vec![always_expand_thinking, keep_awake, allow_asked];
    Page { id: PageId::General, name: "General", icon: Lucide::Settings2, body: Body::Settings(settings) }
}

/// A switch for one of the hub's own settings: on as `says` reads them, and flipped by asking the hub for the change
/// `to` makes. It shows off and can't be flipped while there's no hub to ask, or with a hub that doesn't say.
fn hub_switch(hub: &OfHub, says: fn(&HubSettings) -> Option<bool>, to: fn(bool) -> HubSettingsChange) -> Control {
    let client = hub.asking.as_ref().map(|asking| asking.client.clone());
    let is_on = client.as_ref().and(hub.records.hub_settings()).and_then(says);
    Control::Switch {
        is_on: Box::new(move |_| is_on),
        set: Rc::new(move |on, _| {
            let Some(client) = &client else { return };
            let change = to(on);
            // Nothing waits for the reply: the switch shows what the hub says, which it says to every client once it
            // has changed, and a hub that couldn't change it leaves the switch as it was.
            drop(client.start(move |client| client.hub_configure(change)));
        }),
    }
}
