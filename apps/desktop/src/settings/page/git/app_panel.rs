//! The panel for making an app of your own, in steps: choosing where to make it, under the person's own account or an
//! organization's, and its name; making it on GitHub, through GitHub's manifest flow, with the app's listener waiting
//! for GitHub to send the person back with a code, which the hub makes into the app's key; and installing it where
//! agents should reach. What the hub has made, and where it's installed, are the hub's records, which the panel reads
//! each frame. The panel keeps only the step it's at, with what that step waits on, which goes with it.

use gpui_kit::{Context, Subscription, Task, Window};
use protocol::client::github_apps::GithubManifestCode;
use protocol::model::GithubAppId;

use super::super::field::Field;
use super::app_words;
use super::kept::{Kept, Shows};
use crate::manifest_flow::{Bound, Ended, Organization, Under};
use crate::timers::Timers;

pub(super) struct AppPanel {
    pub(super) step: AppStep,
    /// The wait for the hub, or for GitHub to send the person back, which goes with the panel, and the listener with it.
    pub(super) waiting: Option<Task<()>>,
    /// Has the hub look for where the app's installed as the window comes to the front again, while it's installing.
    pub(super) on_focus: Option<Subscription>,
}

pub(super) enum AppStep {
    Choosing(Choosing),
    OnGithub {
        chosen: Chosen,
        stands: OnGithub,
    },
    /// Installing the app the hub made, as its record and its installations' accounts say.
    Installing {
        app: GithubAppId,
        /// The hub is looking for where it's installed.
        looking: bool,
        /// Why it couldn't, the last time it was asked.
        failed: Option<String>,
    },
}

/// Choosing where to make it, and its name.
pub(super) struct Choosing {
    /// Under an organization, by the name in `organization`, rather than the person's own account.
    pub(super) under_organization: bool,
    pub(super) organization: Field,
    pub(super) name: Field,
    /// Why the last try went no further, if it didn't.
    pub(super) refused: Option<String>,
}

/// What was chosen, which making it on GitHub goes by.
#[derive(Clone)]
pub(super) struct Chosen {
    pub(super) under: Under,
    pub(super) name: String,
}

/// How making it on GitHub stands.
pub(super) enum OnGithub {
    /// The hub is writing its manifest.
    Opening,
    /// The browser has GitHub's form, and the listener waits for GitHub to send the person back, until `until_ms`.
    Waiting { page_url: String, until_ms: i64 },
    /// The listener's time limit passed first.
    StoppedWaiting,
    /// The hub is taking the app's key from GitHub with `code`, or couldn't, as `failed` says.
    Making { code: GithubManifestCode, failed: Option<NotMade> },
}

/// Why the hub couldn't make the app from GitHub's code.
pub(super) struct NotMade {
    pub(super) why: String,
    /// Giving it the same code again might, since GitHub may still take it.
    pub(super) again: bool,
}

impl Kept {
    /// Opens the panel at its first step, with `name` as the app's name, and the cursor in its field.
    pub(super) fn open_app(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let choosing = choosing(&Chosen { under: Under::Account, name: name.to_string() }, None, window, cx);
        self.open_app_at(AppStep::Choosing(choosing), cx);
    }

    /// Opens the panel at `step`, in place of any other.
    pub(super) fn open_app_at(&mut self, step: AppStep, cx: &mut Context<Self>) {
        self.open(Shows::App(AppPanel { step, waiting: None, on_focus: None }), cx);
    }

    pub(super) fn app_panel(&self) -> Option<&AppPanel> {
        let Some(Shows::App(app)) = self.panel.as_ref().map(|panel| &panel.shows) else { return None };
        Some(app)
    }

    pub(super) fn app_panel_mut(&mut self) -> Option<&mut AppPanel> {
        let Some(Shows::App(app)) = self.panel.as_mut().map(|panel| &mut panel.shows) else { return None };
        Some(app)
    }

    /// Makes it under an organization, with the cursor in the field for its name, or under the person's own account.
    pub(super) fn choose_organization(&mut self, organization: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(AppStep::Choosing(choosing)) = self.app_panel_mut().map(|app| &mut app.step) else { return };
        choosing.under_organization = organization;
        if organization {
            choosing.organization.input.update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    /// Whether the first step has what it needs to go on, with a hub to ask.
    pub(super) fn can_make_on_github(&self, cx: &gpui_kit::App) -> bool {
        let Some(AppStep::Choosing(choosing)) = self.app_panel().map(|app| &app.step) else { return false };
        self.can_ask() && choosing.name.filled(cx) && (!choosing.under_organization || choosing.organization.filled(cx))
    }

    /// Has the hub write the app's manifest, for GitHub to send the person back to a listener of the app's own, and
    /// opens the browser on the page that posts it to GitHub. What stops it is said in the first step.
    pub(super) fn make_on_github(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_make_on_github(cx) {
            return;
        }
        let Some(AppStep::Choosing(choosing)) = self.app_panel_mut().map(|app| &mut app.step) else { return };
        let chosen = match chosen(choosing, cx) {
            Ok(chosen) => chosen,
            Err(why) => {
                choosing.refused = Some(why);
                cx.notify();
                return;
            }
        };
        self.open_github(chosen, window, cx);
    }

    /// Opens GitHub's form for making the app as `chosen` says, through a listener of its own, with a new `state`.
    pub(super) fn open_github(&mut self, chosen: Chosen, window: &mut Window, cx: &mut Context<Self>) {
        let Some(asking) = self.asking.clone() else { return };
        let bound = match Bound::new() {
            Ok(bound) => bound,
            Err(e) => return self.choose_again(&chosen, Some(app_words::couldnt_listen(&e)), window, cx),
        };
        let (name, redirect) = (chosen.name.clone(), bound.redirect_url());
        let writing = asking.client.start(move |client| client.github_app_manifest_write(&name, &redirect));
        let task = cx.spawn_in(window, async move |this, cx| {
            let answer = writing.recv().await;
            let _ = cx.update(|window, cx| this.update(cx, |this, cx| this.written(answer, bound, window, cx)));
        });
        self.focus_the_panel(window, cx);
        let Some(app) = self.app_panel_mut() else { return };
        (app.step, app.waiting) = (AppStep::OnGithub { chosen, stands: OnGithub::Opening }, Some(task));
        cx.notify();
    }

    /// Serves the manifest the hub wrote, and opens the browser on the page that posts it, or goes back to the first
    /// step, saying why it couldn't.
    fn written(
        &mut self,
        answer: client_core::Result<protocol::client::github_apps::GithubAppManifest>,
        bound: Bound,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(AppStep::OnGithub { chosen, .. }) = self.app_panel().map(|app| &app.step) else { return };
        let chosen = chosen.clone();
        let limit = Timers::of(cx).github_sends_back;
        let listening = match answer {
            Ok(written) => bound.serve(&written.manifest, &chosen.under, limit),
            Err(e) => return self.choose_again(&chosen, Some(app_words::not_written(&e)), window, cx),
        };
        let listening = match listening {
            Ok(listening) => listening,
            Err(e) => return self.choose_again(&chosen, Some(app_words::couldnt_listen(&e)), window, cx),
        };
        cx.open_url(listening.page_url());
        let (page_url, ended) = (listening.page_url().to_string(), listening.ended());
        let task = cx.spawn_in(window, async move |this, cx| {
            let ended = ended.recv_async().await;
            // Held until GitHub has sent the person back, the limit has passed, or the panel's gone.
            drop(listening);
            let _ = cx.update(|window, cx| this.update(cx, |this, cx| this.listened(ended.ok(), window, cx)));
        });
        let until_ms = look::now_ms(cx) + i64::try_from(limit.as_millis()).unwrap_or(i64::MAX);
        let Some(app) = self.app_panel_mut() else { return };
        app.step = AppStep::OnGithub { chosen, stands: OnGithub::Waiting { page_url, until_ms } };
        app.waiting = Some(task);
        cx.notify();
    }

    /// Takes in how the listener ended: with the code, which the hub is given, or as its limit passed.
    fn listened(&mut self, ended: Option<Ended>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(AppStep::OnGithub { stands, .. }) = self.app_panel_mut().map(|app| &mut app.step) else { return };
        match ended {
            Some(Ended::Heard(code)) => {
                *stands = OnGithub::Making { code, failed: None };
                self.make(window, cx);
            }
            // Nothing heard is the listener let go of, which only the panel going does, with this wait.
            Some(Ended::TimedOut) | None => {
                *stands = OnGithub::StoppedWaiting;
                cx.notify();
            }
        }
    }

    /// Opens GitHub's page for making the app again, as the browser was first opened on it.
    pub(super) fn open_github_again(&self, cx: &mut Context<Self>) {
        let Some(AppStep::OnGithub { stands: OnGithub::Waiting { page_url, .. }, .. }) =
            self.app_panel().map(|app| &app.step)
        else {
            return;
        };
        cx.open_url(page_url);
    }

    /// Opens GitHub's form again, through a listener of its own, once the last stopped waiting.
    pub(super) fn open_github_from_the_start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(AppStep::OnGithub { chosen, .. }) = self.app_panel().map(|app| &app.step) else { return };
        let chosen = chosen.clone();
        self.open_github(chosen, window, cx);
    }

    /// Goes back to the first step, with what was chosen, once GitHub no longer takes the code it sent back.
    pub(super) fn start_again(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(AppStep::OnGithub { chosen, .. }) = self.app_panel().map(|app| &app.step) else { return };
        let chosen = chosen.clone();
        self.choose_again(&chosen, None, window, cx);
    }

    /// Goes back to the first step with what was `chosen`, saying `why` it went no further, if it says.
    pub(super) fn choose_again(
        &mut self,
        chosen: &Chosen,
        why: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let choosing = choosing(chosen, why, window, cx);
        let Some(app) = self.app_panel_mut() else { return };
        (app.step, app.waiting, app.on_focus) = (AppStep::Choosing(choosing), None, None);
        cx.notify();
    }
}

/// The first step, with what was `chosen` in its fields, the cursor in the name's, and why it went no further.
fn choosing(chosen: &Chosen, refused: Option<String>, window: &mut Window, cx: &mut Context<Kept>) -> Choosing {
    let (organization, under_organization) = match &chosen.under {
        Under::Account => (String::new(), false),
        Under::Organization(organization) => (organization.as_str().to_string(), true),
    };
    let organization = Field::new(window, cx, |input| input.default_value(organization), Kept::make_on_github);
    let name = chosen.name.clone();
    let name = Field::new(window, cx, |input| input.default_value(name), Kept::make_on_github);
    Choosing { under_organization, organization, name, refused }
}

/// What `choosing` says to make, or why it can't be made so.
fn chosen(choosing: &Choosing, cx: &gpui_kit::App) -> Result<Chosen, String> {
    let name = choosing.name.value(cx).trim().to_string();
    if !choosing.under_organization {
        return Ok(Chosen { under: Under::Account, name });
    }
    let organization = Organization::parse(&choosing.organization.value(cx));
    Ok(Chosen { under: Under::Organization(organization.map_err(|not| app_words::not_an_organization(&not))?), name })
}
