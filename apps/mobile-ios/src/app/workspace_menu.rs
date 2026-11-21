//! What a long press on a workspace's row offers, as a right-click on the desktop's sidebar does: marking its chats
//! read or unread, renaming it, a new title, and archiving it, which asks first, as a handheld's menu does of a row that
//! removes something, since the phone can't bring a workspace back.

use std::rc::Rc;

use client_core::ClientError;
use client_core::marking::{self, Mark};
use gpui_kit::component::input::InputState;
use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, SharedString, Styled as _,
    Window,
};
use look::{ButtonKind, ControlSize, FieldKind, Space};
use protocol::model::{Session, Title, TitleError, Workspace, WorkspaceId};

use super::MobileApp;
use crate::safe_area;

/// What's being asked before it's done, in an action sheet over the screen.
pub(super) enum Asking {
    /// Archiving this workspace, from its menu.
    Archive { id: WorkspaceId, named: SharedString },
    /// Archiving it all the same, when the hub said there's work it would lose.
    ArchiveAnyway { id: WorkspaceId, reason: SharedString },
}

/// A workspace being renamed, in a sheet with its name in a field.
pub(super) struct Renaming {
    id: WorkspaceId,
    field: Entity<InputState>,
}

impl MobileApp {
    /// The menu a long press on `workspace`'s row opens, from what its chats say now, as every app's workspace menu
    /// does, with Archive asking first.
    pub(super) fn workspace_menu(
        &self,
        workspace: &Workspace,
        chats: &[Session],
        cx: &mut Context<Self>,
    ) -> impl Fn(look::Menu, &mut Window, &mut App) -> look::Menu + 'static {
        let this = cx.weak_entity();
        let (to_mark, to_rename, to_retitle, to_ask) = (this.clone(), this.clone(), this.clone(), this);
        let marking = marking::workspace_marking(chats, Session::unread).map(|(mark, chats)| {
            let does: client_ui::Choose = Rc::new(move |_, cx| {
                let _ = to_mark.update(cx, |app, cx| app.mark_chats(mark, chats.clone(), cx));
            });
            (mark, does)
        });
        let (renamed, retitled, archived) = (workspace.id.clone(), workspace.id.clone(), workspace.id.clone());
        let named = SharedString::from(workspace.label().to_string());
        let (retitling, archiving) = self.followed.as_ref().map_or((false, false), |followed| {
            (followed.retitling.contains(&workspace.id), followed.archiving.contains(&workspace.id))
        });
        client_ui::WorkspaceMenu {
            marking,
            rename: Rc::new(move |window, cx| {
                let _ = to_rename.update(cx, |app, cx| app.start_renaming(&renamed, window, cx));
            }),
            retitle: Rc::new(move |_, cx| {
                let _ = to_retitle.update(cx, |app, cx| app.regenerate_title(retitled.clone(), cx));
            }),
            archive: Rc::new(move |_, cx| {
                let asking = Asking::Archive { id: archived.clone(), named: named.clone() };
                let _ = to_ask.update(cx, |app, cx| app.ask(Some(asking), cx));
            }),
            retitling,
            archiving,
        }
        .of(workspace)
    }

    fn mark_chats(&mut self, mark: Mark, chats: Vec<Session>, cx: &mut Context<Self>) {
        let Some(client) = self.client(cx) else { return };
        let marking = client.start(move |client| mark.chats(client, &chats));
        cx.spawn(async move |this, cx| {
            if let Err(e) = marking.recv().await {
                let said = mark.couldnt_mark_the_workspace(&e);
                let _ = this.update(cx, |app, cx| app.say(said, cx));
            }
        })
        .detach();
    }

    /// Asks the hub to title the workspace again from what's been asked in it, which waits on a model. Asking again
    /// before the answer does nothing.
    fn regenerate_title(&mut self, id: WorkspaceId, cx: &mut Context<Self>) {
        let Some(client) = self.client(cx) else { return };
        let Some(followed) = &mut self.followed else { return };
        if !followed.retitling.insert(id.clone()) {
            return;
        }
        let target = id.clone();
        let asking = client.start(move |client| client.workspace_regenerate_title(&target));
        cx.spawn(async move |this, cx| {
            let retitled = asking.recv().await;
            let _ = this.update(cx, |app, cx| {
                if let Some(followed) = &mut app.followed {
                    followed.retitling.remove(&id);
                }
                if let Err(e) = retitled {
                    app.say(client_core::refusals::couldnt("regenerate the title", &e), cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Puts `asking` over the screen, or takes away what was.
    pub(super) fn ask(&mut self, asking: Option<Asking>, cx: &mut Context<Self>) {
        if let Some(followed) = &mut self.followed {
            followed.asking = asking;
        }
        cx.notify();
    }

    /// Archives, and asks again before losing work the hub says it would. With no connection, the sheet stays, to be
    /// tried again once the hub's back.
    fn archive(&mut self, id: WorkspaceId, force: bool, cx: &mut Context<Self>) {
        let Some(client) = self.client(cx) else {
            return self.say(client_core::refusals::couldnt("archive", &ClientError::Disconnected), cx);
        };
        self.ask(None, cx);
        let Some(followed) = &mut self.followed else { return };
        followed.archiving.insert(id.clone());
        let target = id.clone();
        let archiving = client.start(move |client| client.workspace_archive(&target, force));
        cx.spawn(async move |this, cx| {
            let archived = archiving.recv().await;
            let _ = this.update(cx, |app, cx| {
                if let Some(followed) = &mut app.followed {
                    followed.archiving.remove(&id);
                }
                if let Err(e) = archived {
                    match e.would_lose().filter(|_| !force) {
                        Some(reason) => {
                            app.ask(Some(Asking::ArchiveAnyway { id, reason: reason.to_owned().into() }), cx);
                        }
                        None => app.say(client_core::refusals::couldnt("archive", &e), cx),
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// What's asked, over everything, along the screen's bottom.
    pub(super) fn asking_sheet(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let asking = self.followed.as_ref()?.asking.as_ref()?;
        let bottom = safe_area::insets(window, cx).safe_area.bottom;
        let cancel = cx.listener(|app, _, _, cx| app.ask(None, cx));
        let sheet = match asking {
            Asking::Archive { id, named } => {
                let id = id.clone();
                look::action_sheet(format!("Archive {named}?")).removes(
                    "archive",
                    "Archive",
                    cx.listener(move |app, _, _, cx| app.archive(id.clone(), false, cx)),
                )
            }
            Asking::ArchiveAnyway { id, reason } => {
                let id = id.clone();
                look::action_sheet("Archive anyway?").says(reason.clone()).removes(
                    "archive-anyway",
                    "Archive anyway",
                    cx.listener(move |app, _, _, cx| app.archive(id.clone(), true, cx)),
                )
            }
        };
        Some(sheet.clear(bottom).on_cancel(cancel).into_any_element())
    }

    /// Opens a sheet with the workspace's name in a field, selected, so typing replaces it.
    fn start_renaming(&mut self, id: &WorkspaceId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(records) = self.link().and_then(|link| link.read(cx).snapshot()) else { return };
        let Some(current) = records.workspace(id).map(|w| w.label().to_string()) else { return };
        let field = cx.new(|cx| InputState::new(window, cx).default_value(current));
        field.update(cx, |field, cx| {
            field.select_all(window, cx);
            field.focus(window, cx);
        });
        if let Some(followed) = &mut self.followed {
            followed.renaming = Some(Renaming { id: id.clone(), field });
        }
        cx.notify();
    }

    /// Renames the workspace to what's typed, for the hub to tidy, unless that's nothing, when the sheet stays.
    fn rename(&mut self, cx: &mut Context<Self>) {
        let Some(followed) = &mut self.followed else { return };
        let Some(renaming) = &followed.renaming else { return };
        let title = renaming.field.read(cx).value().to_string();
        if Title::typed(&title) == Err(TitleError::Empty) {
            return;
        }
        let id = renaming.id.clone();
        // With no connection, the sheet stays, with what's typed, to be tried again once the hub's back.
        let Some(client) = self.client(cx) else {
            return self.say(client_core::refusals::couldnt("rename the workspace", &ClientError::Disconnected), cx);
        };
        if let Some(followed) = &mut self.followed {
            followed.renaming = None;
        }
        cx.notify();
        let renamed = client.start(move |client| client.workspace_rename(&id, &title));
        cx.spawn(async move |this, cx| {
            if let Err(e) = renamed.recv().await {
                let _ =
                    this.update(cx, |app, cx| app.say(client_core::refusals::couldnt("rename the workspace", &e), cx));
            }
        })
        .detach();
    }

    /// The sheet a workspace is renamed in, over everything, clear of the keyboard.
    pub(super) fn renaming_sheet(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let renaming = self.followed.as_ref()?.renaming.as_ref()?;
        let bottom = safe_area::insets(window, cx).effective().bottom;
        let rename = look::button("rename-confirm", ButtonKind::Main, ControlSize::Large, "Rename")
            .on_click(cx.listener(|app, _, _, cx| app.rename(cx)));
        let holds = v_flex()
            .p(Space::Medium)
            .gap(Space::Medium)
            .child(look::field("rename-field", &renaming.field, FieldKind::Boxed, ControlSize::Large))
            .child(rename);
        let sheet = look::sheet()
            .id("renaming")
            .title("Rename workspace")
            .clear(bottom)
            .on_close(cx.listener(|app, _, _, cx| {
                if let Some(followed) = &mut app.followed {
                    followed.renaming = None;
                }
                cx.notify();
            }))
            .child(holds);
        Some(sheet.into_any_element())
    }
}
