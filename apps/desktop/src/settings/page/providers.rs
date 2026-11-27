//! Providers, where agents run, as Settings lists them: for each provider, who its agents are on GitHub. On a provider
//! Roundhouse sets up GitHub on, that's the hub's default account, as Git says. On one left to its machines, such as
//! this Mac, Roundhouse hands agents nothing, so they use the machine's own git and `gh`, and the page only informs:
//! on the hub's own machine, who `gh` and the SSH key there are, which the hub reads each time the page opens, and who
//! Roundhouse itself reads GitHub as, so a person sees when they differ. Which providers are which is the hub's
//! settings' to say.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, App, Context, IntoElement, ParentElement as _, RenderOnce, SharedString, Styled as _, Task, Window,
    div, px,
};
use protocol::client::github_accounts::HubMachineLogins;
use protocol::model::{GhLogin, GhLoginWorks, MachineLogins, ProviderName, SshLogin};

use super::account_row::{self, COLUMN_WIDTH, Columns, Detail, Picture, Row, Tone};
use super::lists::{heading, list as rows_of, said_in_the_list};
use super::{Asking, Body, OfHub, Page, PageId};
use crate::names::{MACHINE, THIS_MACHINE};
use crate::status_box::{self, StatusBox};
use client_core::records::Snapshot;

/// What a search finds the page by, besides its name.
const ABOUT: &str = if cfg!(target_os = "macos") {
    "Where agents run, and who they are on GitHub there: gh and the SSH key on this Mac."
} else {
    "Where agents run, and who they are on GitHub there: gh and the SSH key on this computer."
};

/// How wide a row's middle columns are: what the row is, `gh` or `SSH key`, and what else there is to say.
const COLUMNS: Columns = Columns { how: px(140.), detail: px(200.) };

pub(super) fn page(hub: &OfHub) -> Page {
    let (records, asking) = (hub.records.clone(), hub.asking.clone());
    let draw = Box::new(move |_: &super::Within| {
        ProvidersPage { records: records.clone(), asking: asking.clone() }.into_any_element()
    });
    Page { id: PageId::Providers, name: "Providers", icon: Lucide::Server, body: Body::Own { about: ABOUT, draw } }
}

/// The page, drawn from the hub's settings and what it said of its machine's logins, with the hub to ask while there's
/// one.
#[derive(IntoElement)]
struct ProvidersPage {
    records: Snapshot,
    asking: Option<Asking>,
}

impl RenderOnce for ProvidersPage {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let asking = self.asking;
        let read = window.use_keyed_state("settings-providers", cx, move |_, cx| MachineRead::start(asking, cx));
        let machine = read.read(cx).said.clone();
        let sections = providers(&self.records, &machine).into_iter().map(|section| section.draw(cx));
        v_flex().w_full().max_w(COLUMN_WIDTH).gap(look::Space::XXLarge).pb(look::Space::Large).children(sections)
    }
}

/// What the hub said of who `gh` and the SSH key on its machine are, asked once as the page opens, since either can
/// change at any time, and kept while it shows.
struct MachineRead {
    said: Said,
    /// The wait for the hub to say, while it hasn't. It goes with the page.
    _reading: Option<Task<()>>,
}

#[derive(Clone)]
enum Said {
    Asking,
    Read(MachineLogins),
    /// The hub couldn't say, or there's no hub to ask.
    Unsaid,
}

impl MachineRead {
    fn start(asking: Option<Asking>, cx: &mut Context<Self>) -> Self {
        let Some(asking) = asking else { return Self { said: Said::Unsaid, _reading: None } };
        let reading = asking.hub.ask(HubMachineLogins);
        let task = cx.spawn(async move |this, cx| {
            let answer = reading.recv().await;
            let _ = this.update(cx, |this, cx| {
                this.said = answer.map_or(Said::Unsaid, Said::Read);
                cx.notify();
            });
        });
        Self { said: Said::Asking, _reading: Some(task) }
    }
}

/// Every provider the hub's settings name: those left to their machines first, this Mac first among them, then those
/// Roundhouse sets up GitHub on, in the order the hub has them. None until the hub's settings arrive.
fn providers(records: &Snapshot, machine: &Said) -> Vec<Section> {
    let Some(github) = records.hub_settings().and_then(|settings| settings.github.as_ref()) else { return Vec::new() };
    let hub = hub_reads_as(records);
    let mut left: Vec<&ProviderName> = github.left_to_machine.iter().collect();
    left.sort_by_key(|provider| !provider.is_local());
    let left = left.into_iter().map(|provider| {
        let shows = if provider.is_local() {
            Shows::ThisMachine { machine: machine.clone(), hub: hub.clone() }
        } else {
            Shows::Elsewhere
        };
        Section { name: name_of(provider), says: left_says(provider).into(), shows }
    });
    let set_up = github.set_up.iter().map(|provider| Section {
        name: name_of(provider),
        says: MANAGED_SAYS.into(),
        shows: Shows::Managed { hub: hub.clone() },
    });
    left.chain(set_up).collect()
}

fn name_of(provider: &ProviderName) -> SharedString {
    if provider.is_local() { THIS_MACHINE.into() } else { provider.to_string().into() }
}

/// Who the hub reads GitHub as: its default account's user, while it has one.
fn hub_reads_as(records: &Snapshot) -> Option<String> {
    let default = records.hub_settings()?.github.as_ref()?.account.as_ref()?;
    let account = records.github_account(default)?;
    Some(records.github_username(account).to_string())
}

/// What a provider's section shows.
struct Section {
    name: SharedString,
    says: SharedString,
    shows: Shows,
}

enum Shows {
    /// Roundhouse sets up GitHub there, so its agents are the hub's default, if it has one.
    Managed { hub: Option<String> },
    /// The hub's own machine: whoever `gh` and the SSH key there are, as `machine` says, beside who the hub reads
    /// GitHub as.
    ThisMachine { machine: Said, hub: Option<String> },
    /// Another provider's machines, whose logins the hub doesn't read.
    Elsewhere,
}

/// What a provider Roundhouse sets up GitHub on says of itself. *Unconfirmed.*
const MANAGED_SAYS: &str = "Roundhouse sets up GitHub here. Agents push, open pull requests and commit as the default \
                            account in Git, even to remotes written for SSH.";

/// What a provider left to its machines says of itself. *Unconfirmed:* the words for a provider other than this Mac.
fn left_says(provider: &ProviderName) -> String {
    if provider.is_local() {
        format!(
            "Roundhouse hands agents here nothing for GitHub, so they use this {MACHINE}'s own git and gh, as in your \
             terminal, and commit as your git config says."
        )
    } else {
        "Roundhouse hands agents here nothing for GitHub, so they use git and gh as these machines have them set up."
            .to_string()
    }
}

impl Section {
    fn draw(self, cx: &App) -> AnyElement {
        let id =
            |part: &str| SharedString::from(format!("providers-{}-{part}", self.name.to_lowercase().replace(' ', "-")));
        let heading = heading(self.name.clone(), None, self.says, None, cx);
        let column = match self.shows {
            Shows::Managed { hub } => {
                let row = match hub {
                    Some(hub) => Row {
                        id: id("account"),
                        picture: Picture::of(&hub),
                        name: hub,
                        dim: false,
                        default: false,
                        how: Some("Default in Git".into()),
                        detail: None,
                        menu: None,
                        card: None,
                        under: Vec::new(),
                    },
                    None => Row {
                        id: id("account"),
                        picture: Picture::Nobody,
                        name: "No account".to_string(),
                        dim: true,
                        default: false,
                        how: None,
                        detail: Some(Detail::Said("Connect one in Git".to_string(), Tone::Problem)),
                        menu: None,
                        card: None,
                        under: Vec::new(),
                    },
                };
                v_flex().child(heading).child(rows_of(cx).child(account_row::row(row, COLUMNS, cx)))
            }
            Shows::ThisMachine { machine, hub } => {
                let list = rows_of(cx);
                let list = match machine {
                    Said::Read(MachineLogins { gh, ssh }) => {
                        let gh_name = match &gh {
                            Some(GhLogin::LoggedIn { username, .. }) => Some(username.to_string()),
                            _ => None,
                        };
                        let warning =
                            gh_warning(gh.as_ref()).map(|said| div().py(look::Space::Large).child(said.draw(cx)));
                        list.children(warning)
                            .child(account_row::row(gh_row(id("gh"), gh.as_ref()), COLUMNS, cx))
                            .child(account_row::row(ssh_row(id("ssh"), &ssh, gh_name.as_deref()), COLUMNS, cx))
                    }
                    Said::Asking => list.child(said_in_the_list("providers-machine-asking", asking().into(), cx)),
                    Said::Unsaid => list.child(said_in_the_list("providers-machine-unsaid", unsaid().into(), cx)),
                };
                let hub = said_in_the_list("providers-hub-reads-as", hub_line(hub.as_deref()).into(), cx);
                v_flex().child(heading).child(list.child(hub))
            }
            Shows::Elsewhere => v_flex().child(heading),
        };
        column.into_any_element()
    }
}

/// What's said while the hub hasn't said who `gh` and the SSH key here are. *Unconfirmed.*
fn asking() -> String {
    format!("Asking who gh and the SSH key on this {MACHINE} are…")
}

/// What's said when the hub couldn't say who they are. *Unconfirmed.*
fn unsaid() -> String {
    format!("Roundhouse couldn't tell who gh and the SSH key on this {MACHINE} are.")
}

/// `gh`'s row: who it's logged in as, or that it's logged in to no one or isn't there, and what to do about that, or
/// that the hub couldn't tell, with `None`.
fn gh_row(id: SharedString, gh: Option<&GhLogin>) -> Row {
    let (picture, name, detail) = match gh {
        Some(GhLogin::LoggedIn { username, works }) => {
            let detail = (*works == GhLoginWorks::Refused)
                .then(|| Detail::Said("GitHub refused its token".to_string(), Tone::Problem));
            (Picture::of(username.as_str()), username.to_string(), detail)
        }
        Some(GhLogin::LoggedOut) => (
            Picture::Nobody,
            "Not logged in".to_string(),
            Some(Detail::Said("Run gh auth login".into(), Tone::Problem)),
        ),
        Some(GhLogin::NotInstalled) => {
            (Picture::Nobody, "No gh".to_string(), Some(Detail::Said("Install gh".into(), Tone::Problem)))
        }
        None => (Picture::Nobody, "Couldn't tell".to_string(), None),
        Some(GhLogin::Unknown) => (Picture::Nobody, "Unknown".to_string(), None),
    };
    let nobody = matches!(picture, Picture::Nobody);
    Row {
        id,
        picture,
        name,
        dim: nobody,
        default: false,
        how: Some("gh".into()),
        detail,
        menu: None,
        card: None,
        under: Vec::new(),
    }
}

/// The SSH key's row: whose GitHub says it is, and that it isn't `gh`'s account, when it isn't.
fn ssh_row(id: SharedString, ssh: &SshLogin, gh: Option<&str>) -> Row {
    let (picture, name, detail) = match ssh {
        SshLogin::Key { username } => {
            let differs = gh.is_some_and(|gh| !gh.eq_ignore_ascii_case(username.as_str()));
            let detail = differs.then(|| Detail::Said("Not gh's account".to_string(), Tone::Warning));
            (Picture::of(username.as_str()), username.to_string(), detail)
        }
        SshLogin::NoKey => (
            Picture::Nobody,
            "No SSH key for GitHub".to_string(),
            Some(Detail::Said("Pushes go as git is set up".to_string(), Tone::Plain)),
        ),
        SshLogin::CouldntTell => (Picture::Nobody, "Couldn't tell".to_string(), None),
        SshLogin::Unknown => (Picture::Nobody, "Unknown".to_string(), None),
    };
    let nobody = matches!(picture, Picture::Nobody);
    Row {
        id,
        picture,
        name,
        dim: nobody,
        default: false,
        how: Some("SSH key".into()),
        detail,
        menu: None,
        card: None,
        under: Vec::new(),
    }
}

/// What's said of agents here while `gh` there can't be used, and what to do about it.
fn gh_warning(gh: Option<&GhLogin>) -> Option<StatusBox> {
    let detail = match gh? {
        GhLogin::LoggedOut => "Run gh auth login in a terminal here. Their pushes still go as git is set up here.",
        GhLogin::NotInstalled => "Install gh, then run gh auth login. Their pushes still go as git is set up here.",
        GhLogin::LoggedIn { works: GhLoginWorks::Refused, .. } => {
            "GitHub refused gh's token. Run gh auth login again. Their pushes still go as git is set up here."
        }
        GhLogin::LoggedIn { .. } | GhLogin::Unknown => return None,
    };
    Some(StatusBox {
        id: "providers-gh-unusable",
        icon: Lucide::TriangleAlert,
        tone: status_box::Tone::Problem,
        lead: Some("Agents here can't use gh.".into()),
        detail: detail.into(),
        progress: None,
        action: None,
    })
}

/// Who the hub reads GitHub as, which may not be who agents here are. *Unconfirmed.*
fn hub_line(hub: Option<&str>) -> String {
    match hub {
        Some(hub) => format!("Roundhouse itself reads pull requests as {hub}, the default account in Git."),
        None => {
            "Roundhouse itself has no default GitHub account, so it can't read pull requests. Choose or connect one \
                 in Git."
                .into()
        }
    }
}
