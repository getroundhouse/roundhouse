//! How the new-workspace dialog looks: the repository over a line the dialog's width, then what's attached and what's
//! typed, then the model and effort, attaching and Create.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Textarea;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    Anchor, Context, InteractiveElement as _, IntoElement, Keystroke, ParentElement as _, Render, Styled as _, Window,
    div, px, transparent_black,
};
use look::{Hand as _, Off as _};

use super::{NewWorkspaceDialog, RADIUS};
use crate::attached;
use crate::model_picker::model_picker;

impl Render for NewWorkspaceDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // An edge of its own inside the dialog's, which shows only while files are dragged over it.
        let target = v_flex().id("new-workspace").w_full().rounded(RADIUS).border_1().border_color(transparent_black());
        attached::takes_drops(target, cx, |_, paths, cx| Self::attach_paths(paths, cx))
            .child(self.header(cx))
            .children(
                self.attached.tiles("new-workspace-attachments", cx, Self::remove_attached).map(|t| t.px_2().pt_2()),
            )
            .child(self.prompt_field(cx))
            .child(self.footer(cx))
    }
}

impl NewWorkspaceDialog {
    /// The repository it'll be made in, which opens a menu of the others.
    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let repos = &self.offer.repos;
        let name = repos.iter().find(|(id, _)| *id == self.repo).map(|(_, name)| name.clone()).unwrap_or_default();
        let (choices, current, this) = (repos.clone(), self.repo.clone(), cx.weak_entity());
        let menu = move |mut menu: PopupMenu, _: &mut Window, _: &mut Context<PopupMenu>| {
            for (id, name) in &choices {
                let (this, id) = (this.clone(), id.clone());
                let checked = id == current;
                menu = menu.item(PopupMenuItem::new(name.clone()).checked(checked).on_click(move |_, _, cx| {
                    let _ = this.update(cx, |t, cx| t.set_repo(id.clone(), cx));
                }));
            }
            menu
        };
        h_flex().w_full().h(px(48.)).px_2().border_b_1().border_color(cx.theme().border).child(
            Button::new("new-workspace-repo")
                .hand()
                .ghost()
                .dropdown_caret(true)
                .label(name)
                .dropdown_menu_with_anchor(Anchor::TopLeft, menu),
        )
    }

    /// What's typed, which takes a paste of images or files as attachments.
    fn prompt_field(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.weak_entity();
        let field = Textarea::new(&self.prompt)
            .appearance(false)
            .on_paste(move |item, _, cx| this.update(cx, |_, cx| Self::paste(item, cx)).unwrap_or(false));
        div().w_full().px_2().pt_2().child(field)
    }

    /// The model and effort on the left, and on the right attaching and Create, the dialog's one main action.
    fn footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.weak_entity();
        let picker = model_picker("new-workspace-model", self.model.as_deref(), self.effort, move |picked, _, cx| {
            let _ = this.update(cx, |t, cx| t.pick(picked, cx));
        });
        let attach = attached::attach_button("new-workspace-attach", cx, |_, paths, cx| Self::attach_paths(paths, cx));
        let enter = Keystroke::parse("enter").ok().map(|key| Kbd::new(key).appearance(false));
        let create = Button::new("new-workspace-create")
            .primary()
            .small()
            .label("Create")
            .children(enter)
            .off_while(!self.offer.can_create)
            .on_click(cx.listener(|t, _, _, cx| t.create(cx)));
        h_flex()
            .w_full()
            .px_3()
            .pb_3()
            .pt_1()
            .gap_1()
            // A row, so the picker keeps its own width and its caret stays beside what it says.
            .child(h_flex().flex_1().min_w_0().child(picker))
            .child(attach.small())
            .child(create)
    }
}
