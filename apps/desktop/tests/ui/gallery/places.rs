//! Where a sheet of the foundations draws each name it shows, such as a colour's swatch or the two blocks a step of
//! space leaves between them, so the design system's page can cut each out of the sheet's picture and show
//! it beside its row of the spec's table. They're kept beside the sheet's picture, as `docs/spec/screens/NAME.json`,
//! and checked and accepted with it.

#![cfg(test)]

use std::collections::BTreeMap;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{ElementId, Window};
use serde::Serialize;
use spec_screens::{DESKTOP, UPDATE};

use super::frame::to_pixels;

/// The ID a sheet gives what it draws for `name` in `kind`, such as `Medium` in `look::Space`, so its place can be
/// found.
pub(crate) fn id(kind: &str, name: &str) -> ElementId {
    ElementId::Name(format!("{kind}::{name}").into())
}

/// Where a sheet drew each name it shows, by the type that gives it, as the spec's table of them names the type, then
/// by the name, and which it didn't draw.
#[derive(Default)]
pub(crate) struct Places {
    found: BTreeMap<&'static str, BTreeMap<String, Place>>,
    missing: Vec<String>,
}

/// A box in a picture's pixels: its left and top edges, and how wide and tall it is.
#[derive(Serialize)]
struct Place {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl Places {
    /// Where the window last drew each name, by the ID [`id`] gives it: each of the names that go with the type that
    /// gives them, such as `look::Space` and `["XSmall", "Small"]`.
    pub(crate) fn of(window: &Window, named: &[(&'static str, &[String])]) -> Self {
        let scale = window.scale_factor();
        let to = |points: gpui_kit::Pixels| to_pixels(f32::from(points), scale);
        let mut places = Places::default();
        for &(kind, names) in named {
            for name in names {
                let Some(found) = window.try_find(id(kind, name)) else {
                    places.missing.push(format!("{kind}::{name}"));
                    continue;
                };
                let bounds = found.bounds();
                let (x, y) = (to(bounds.left()), to(bounds.top()));
                let place = Place { x, y, width: to(bounds.right()) - x, height: to(bounds.bottom()) - y };
                places.found.entry(kind).or_default().insert(name.clone(), place);
            }
        }
        places
    }

    /// What's wrong with them: a name the sheet didn't draw, or places that aren't as approved. With [`UPDATE`] set,
    /// they're accepted instead of compared, as the sheet's picture is.
    pub(crate) fn look_at(&self, sheet: &str) -> Vec<String> {
        let mut problems: Vec<String> =
            self.missing.iter().map(|name| format!("{sheet} doesn't draw {name} with its ID")).collect();
        let written = serde_json::to_string_pretty(&self.found).unwrap() + "\n";
        let path = DESKTOP.beside(&format!("{sheet}.json"));
        if std::env::var_os(UPDATE).is_some() {
            std::fs::write(&path, written).unwrap();
        } else {
            match std::fs::read_to_string(&path) {
                Ok(approved) if approved == written => {}
                Ok(_) => problems.push(format!("where {sheet} draws each name no longer matches {sheet}.json")),
                Err(_) => problems.push(format!("{sheet} has no {sheet}.json for where it draws each name")),
            }
        }
        problems
    }
}
