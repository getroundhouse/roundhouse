//! The workspace's setup script: its status line and its log.

use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, App, AppContext, Context, IntoElement, ParentElement, SharedString, StatefulInteractiveElement, Styled,
    div, px,
};
use look::Lined as _;
use look::{InRegion as _, Region, Typeset as _};
use protocol::model::SetupStatus;

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
        .px(look::Space::Medium)
        .py(look::Space::Small)
        .line_over(cx)
        .typeset(look::Type::Code)
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
            if let Err(e) = starting.recv().await {
                let _ =
                    this.update(cx, |_, cx| cx.emit(PaneEvent::Error(client_core::refusals::couldnt("run setup", &e))));
            }
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
        let (muted, warning) = (colors.text_muted, colors.warning);
        let log_label = if self.setup_log.is_some() { "Hide log" } else { "View log" };
        Some(
            h_flex()
                .flex_none()
                .w_full()
                .px(look::Space::Medium)
                .py(look::Space::XSmall)
                .gap(look::Space::Small)
                .line_over(cx)
                .typeset(look::Type::Detail)
                .text_color(if running { muted } else { warning })
                .child(div().flex_1().min_w_0().child(message))
                .child(
                    look::button("setup-log", look::ButtonKind::Borderless, look::ControlSize::Small, log_label)
                        .on_click(cx.listener(|t, _, _, cx| t.toggle_setup_log(cx))),
                )
                // The last button out past its own room, so its words end as far in as the message starts.
                .child(if running {
                    look::button("stop-setup", look::ButtonKind::Borderless, look::ControlSize::Small, "Stop")
                        .stands_out(&[look::Side::Right])
                        .on_click(cx.listener(|t, _, _, cx| t.stop_agent(cx)))
                        .into_any_element()
                } else {
                    look::button("rerun-setup", look::ButtonKind::Borderless, look::ControlSize::Small, "Run again")
                        .stands_out(&[look::Side::Right])
                        .on_click(cx.listener(|t, _, _, cx| t.rerun_setup(cx)))
                        .into_any_element()
                })
                .into_any_element(),
        )
    }
}
