//! The workspace's setup script: its status line and its log.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, App, AppContext, Context, IntoElement, ParentElement, SharedString, StatefulInteractiveElement, Styled,
    div, px,
};
use look::{Hand as _, InRegion as _, Region};
use protocol::model::{Record, SetupStatus};

use super::{PaneEvent, WorkspacePane};

const SETUP_LOG_REFRESH: std::time::Duration = std::time::Duration::from_secs(1);

pub(super) fn setup_log_view(text: String, cx: &App) -> impl IntoElement {
    let colors = look::colors(cx);
    div()
        .region(Region::SetupLog)
        .flex_none()
        .w_full()
        .max_h(px(220.))
        .overflow_y_scroll()
        .px_3()
        .py_2()
        .border_t_1()
        .border_color(colors.edge)
        .font_family(cx.theme().mono_font_family.clone())
        .text_size(cx.theme().mono_font_size)
        .text_color(colors.text_muted)
        .child(SharedString::from(if text.is_empty() { "Nothing yet.".to_string() } else { text }))
}

impl WorkspacePane {
    /// Shows or hides the setup log. It stays current until setup finishes.
    pub fn toggle_setup_log(&mut self, cx: &mut Context<Self>) {
        if self.setup_log.take().is_some() {
            self.setup_log_refresh = None;
            cx.notify();
            return;
        }
        self.setup_log = Some(String::new());
        self.load_setup_log(cx);
        self.setup_log_refresh = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(SETUP_LOG_REFRESH).await;
                let running = this.update(cx, |t, cx| {
                    let running = t.workspace().setup == SetupStatus::Running;
                    if running {
                        t.load_setup_log(cx);
                    }
                    running
                });
                if !matches!(running, Ok(true)) {
                    return;
                }
            }
        }));
        cx.notify();
    }

    pub(super) fn load_setup_log(&mut self, cx: &mut Context<Self>) {
        let (client, id) = (self.client.clone(), self.workspace().id.clone());
        cx.spawn(async move |this, cx| {
            let log = cx.background_spawn(async move { client.workspace_setup_log(&id) }).await;
            let _ = this.update(cx, |t, cx| {
                if t.setup_log.is_some() {
                    t.setup_log = Some(log.unwrap_or_else(|e| format!("Couldn't read the log: {e}")));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub fn rerun_setup(&mut self, cx: &mut Context<Self>) {
        let id = self.workspace().id.clone();
        let starting = self.client.start(move |client| client.workspace_rerun_setup(&id));
        cx.spawn(async move |this, cx| {
            let started = starting.recv().await;
            let _ = this.update(cx, |t, cx| match started {
                Ok(workspace) => t.records.take_reply(Record::Workspace(workspace)),
                Err(e) => cx.emit(PaneEvent::Error(format!("couldn't run setup: {e}"))),
            });
        })
        .detach();
    }

    /// A line above the composer while setup matters: when it's running, or if it failed.
    pub(super) fn setup_line(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (message, running) = match self.workspace().setup {
            SetupStatus::Running => ("Setting up this workspace. Messages you send now wait until it's ready.", true),
            SetupStatus::Failed { code: Some(_) } => ("Setup failed, so the agent may find things missing.", false),
            SetupStatus::Failed { code: None } => ("Setup didn't finish, so the agent may find things missing.", false),
            _ => return None,
        };
        let colors = look::colors(cx);
        let (border, muted, warning) = (colors.edge, colors.text_muted, colors.warning);
        let log_label = if self.setup_log.is_some() { "Hide log" } else { "View log" };
        Some(
            h_flex()
                .flex_none()
                .w_full()
                .px_3()
                .py_1()
                .gap_2()
                .border_t_1()
                .border_color(border)
                .text_xs()
                .text_color(if running { muted } else { warning })
                .child(div().flex_1().min_w_0().child(message))
                .child(
                    Button::new("setup-log")
                        .hand()
                        .ghost()
                        .xsmall()
                        .label(log_label)
                        .on_click(cx.listener(|t, _, _, cx| t.toggle_setup_log(cx))),
                )
                .child(if running {
                    Button::new("stop-setup")
                        .hand()
                        .ghost()
                        .xsmall()
                        .label("Stop")
                        .on_click(cx.listener(|t, _, _, cx| t.stop_agent(cx)))
                        .into_any_element()
                } else {
                    Button::new("rerun-setup")
                        .hand()
                        .ghost()
                        .xsmall()
                        .label("Run again")
                        .on_click(cx.listener(|t, _, _, cx| t.rerun_setup(cx)))
                        .into_any_element()
                })
                .into_any_element(),
        )
    }
}
