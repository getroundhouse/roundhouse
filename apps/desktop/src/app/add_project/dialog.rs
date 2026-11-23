//! The dialog Add project opens: a search over the repositories the hub's GitHub accounts can see, a list of them to
//! pick from, and a field for an address on another host.

use client_core::Client;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{IconName, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, TestSupportExt as _, Window, div, px,
};
use look::{DialogWidth, Space, Type, Typeset as _};
use protocol::client::github_repositories::GithubRepositorySeen;
use protocol::client::records::RepoToAdd;
use protocol::model::{GithubRepositoryId, RepoUrl};

use super::rows::row;

/// How tall the list is at most, so the address field stays in sight under a long one.
const LIST_UP_TO: f32 = 280.;

/// What the list shows.
enum Listed {
    Reading,
    Read(Vec<GithubRepositorySeen>),
    Failed(String),
}

struct AddProject {
    client: Client,
    search: Entity<InputState>,
    address: Entity<InputState>,
    listed: Listed,
    chosen: Option<GithubRepositoryId>,
    adding: bool,
    error: Option<String>,
    /// The read under way, which a newer search drops.
    reading: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

/// Opens the dialog over `window`, adding to the hub `client` reaches.
pub(crate) fn open(client: Client, window: &mut Window, cx: &mut App) {
    let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search your repositories on GitHub"));
    let address = cx.new(|cx| InputState::new(window, cx).placeholder("https://gitlab.com/owner/repository.git"));
    let (searched, typed) = (search.clone(), address.clone());
    let dialog = cx.new(|cx| AddProject::new(client, searched, typed, cx));
    look::open_dialog(window, cx, move |_, cx| AddProject::dialog(&dialog, cx));
    search.update(cx, |search, cx| search.focus(window, cx));
}

impl AddProject {
    fn new(client: Client, search: Entity<InputState>, address: Entity<InputState>, cx: &mut Context<Self>) -> Self {
        let searching = cx.subscribe(&search, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.read(cx);
            }
        });
        // Typing an address adds it rather than what's chosen in the list.
        let typing = cx.subscribe(&address, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.chosen = None;
                this.error = None;
                cx.notify();
            }
        });
        let mut adding = Self {
            client,
            search,
            address,
            listed: Listed::Reading,
            chosen: None,
            adding: false,
            error: None,
            reading: None,
            _subscriptions: vec![searching, typing],
        };
        adding.read(cx);
        adding
    }

    /// Reads the list again for what's searched now, in place of any read under way.
    fn read(&mut self, cx: &mut Context<Self>) {
        let (client, search) = (self.client.clone(), self.search.read(cx).value().to_string());
        let reading = cx.background_spawn(async move { client.github_repository_list(&search, None) });
        self.reading = Some(cx.spawn(async move |this, cx| {
            let read = reading.await;
            let _ = this.update(cx, |this, cx| {
                this.listed = match read {
                    Ok(page) => Listed::Read(page.repositories),
                    Err(e) => Listed::Failed(client_core::refusals::couldnt("list your repositories", &e)),
                };
                cx.notify();
            });
        }));
    }

    fn choose(&mut self, id: GithubRepositoryId, window: &mut Window, cx: &mut Context<Self>) {
        self.chosen = Some(id);
        self.error = None;
        self.address.update(cx, |address, cx| address.set_value("", window, cx));
        cx.notify();
    }

    /// What adding adds now: the address typed, or else the repository chosen.
    fn to_add(&self, cx: &App) -> Option<RepoToAdd> {
        let typed = self.address.read(cx).value().trim().to_owned();
        if !typed.is_empty() {
            return RepoUrl::try_from(typed).ok().map(|url| RepoToAdd::Address { url });
        }
        self.chosen.clone().map(|id| RepoToAdd::Github { id })
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(repository) = self.to_add(cx).filter(|_| !self.adding) else { return };
        self.adding = true;
        self.error = None;
        cx.notify();
        let adding = self.client.start(move |client| client.repo_add_linked(repository));
        let handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            let added = adding.recv().await;
            let _ = handle.update(cx, |_, window, cx| match added {
                Ok(_) => look::close_dialog(window, cx),
                Err(e) => {
                    let _ = this.update(cx, |this, cx| {
                        this.adding = false;
                        this.error = Some(client_core::refusals::couldnt("add that", &e));
                        cx.notify();
                    });
                }
            });
        })
        .detach();
    }

    /// The dialog as it stands now, built each time it's drawn.
    fn dialog(this: &Entity<Self>, cx: &App) -> look::Dialog {
        let adding = this.read(cx);
        let can_add = adding.to_add(cx).is_some() && !adding.adding;
        let shown = v_flex()
            .id("add-project-dialog")
            .w_full()
            .gap(Space::Medium)
            .child(
                look::field("add-project-search", &adding.search, look::FieldKind::Boxed, look::ControlSize::Large)
                    .icon(IconName::Search),
            )
            .child(list(this, adding, cx))
            .child(
                div()
                    .typeset(Type::Detail)
                    .text_color(look::colors(cx).text_muted)
                    .child("Or an address on another host"),
            )
            .child(
                look::field("add-project-address", &adding.address, look::FieldKind::Boxed, look::ControlSize::Large)
                    .icon(IconName::Globe),
            )
            .when_some(adding.error.clone(), |shown, error| {
                shown.child(div().typeset(Type::Body).text_color(look::colors(cx).error).child(error))
            })
            .test_support();
        let confirming = this.clone();
        look::dialog("Add project")
            .lead("Pick a repository your GitHub accounts can see. Its workspaces clone it.")
            .width(DialogWidth::Wide)
            .child(shown)
            .closes(look::button("cancel-add-project", look::ButtonKind::Edged, look::ControlSize::Large, "Cancel"))
            .confirms(
                look::button("add-project-confirm", look::ButtonKind::Main, look::ControlSize::Large, "Add")
                    .off(!can_add && !adding.adding)
                    .waiting(adding.adding),
            )
            .on_confirm(move |window, cx| {
                confirming.update(cx, |this, cx| this.add(window, cx));
                false
            })
    }
}

/// The list of what the accounts can see, or why there's none to show.
fn list(this: &Entity<AddProject>, adding: &AddProject, cx: &App) -> AnyElement {
    let muted = |words: String| {
        div()
            .py(Space::Large)
            .typeset(Type::Body)
            .text_color(look::colors(cx).text_muted)
            .child(words)
            .into_any_element()
    };
    let repositories = match &adding.listed {
        Listed::Reading => return muted("Reading your repositories…".to_owned()),
        Listed::Failed(why) => return muted(why.clone()),
        Listed::Read(repositories) if repositories.is_empty() => return muted("No repository matches.".to_owned()),
        Listed::Read(repositories) => repositories,
    };
    let rows = repositories.iter().map(|seen| {
        let (choosing, id) = (this.clone(), seen.id.clone());
        row(seen, adding.chosen.as_ref() == Some(&seen.id), cx).on_click(move |_, window, cx| {
            choosing.update(cx, |this, cx| this.choose(id.clone(), window, cx));
        })
    });
    v_flex().id("add-project-list").max_h(px(LIST_UP_TO)).overflow_y_scroll().children(rows).into_any_element()
}
