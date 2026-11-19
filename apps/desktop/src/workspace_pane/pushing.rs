//! What a push from the workspace would meet on GitHub, as the hub last checked its repository: a line above the
//! composer while agents can't push there as the default GitHub account, which says why and what to do, and blocks
//! nothing.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::h_flex;
use gpui_kit::{AnyElement, Context, HighlightStyle, IntoElement as _, ParentElement as _, SharedString, Styled as _};
use gpui_kit::{StyledText, div};
use look::Lined as _;
use look::{InRegion as _, Region, Typeset as _};
use protocol::model::{GithubRepositoryName, GithubUsername, InstalledApp, Reach};

use super::{PaneEvent, WorkspacePane};
use crate::github_app_pages::install_page;

/// The line's button, which opens Git in Settings.
pub const OPEN_GIT: &str = "push-line-git";

/// The line's button, for a default that's an app of your own's installation, which opens GitHub's page for installing
/// the app, where it's given the repository, or installed again.
pub const INSTALL_APP: &str = "push-line-install";

/// What the line says.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Said {
    /// What's wrong, in a warning's colour.
    pub(super) wrong: String,
    /// GitHub's words, if it gave any, and what to do, quieter, on from it.
    pub(super) then: String,
    /// GitHub's page for installing the default's app, when what to do is done there.
    pub(super) install_at: Option<String>,
    /// Whether what it says to do is done in Git in Settings, which the line's button opens.
    pub(super) in_settings: bool,
}

/// What to do, after what's wrong, and GitHub's page for installing the default's app, when it's done there, as
/// [`Said`] has them.
type ToDo = (String, Option<String>);

/// What a workspace says of its repository's `reach`, if anything. Nothing while a push would be taken, or the hub
/// couldn't tell, since a push that fails then says why itself, or while the default is each machine's own, which the
/// hub has no token of to check, or the repository isn't on GitHub. *Unconfirmed.*
pub(super) fn said(reach: &Reach) -> Option<Said> {
    let (repository, username, (then, install_at)) = match reach {
        Reach::Refused { repository, username, reason, installation } => {
            (repository, username, refused(repository, username, reason, installation.as_ref()))
        }
        Reach::NotFound { repository, username, installation } => {
            (repository, username, not_found(repository, username, installation.as_ref()))
        }
        Reach::TokenRefused { repository, username, installation } => {
            (repository, username, token_refused(installation.as_ref()))
        }
        // The block's reason says why and what to do, as git says it as it refuses the push.
        Reach::Blocked { repository, reason } => {
            let wrong = format!("Agents can't push to {repository}.");
            return Some(Said { wrong, then: sentence(reason), install_at: None, in_settings: true });
        }
        // What to do is in the repository's git config, not in Settings, so there's no button.
        Reach::PushesAsMachineOwn { repository } => {
            let wrong =
                format!("Agents push to {repository} as their machine's own login, not the default GitHub account.");
            let then = format!(
                "Where origin pushes, as git config rewrites it, isn't https://github.com/, so pushes go round the \
                 default, as over SSH with the machine's key. To push as the default, have origin push to \
                 https://github.com/{repository}.git."
            );
            return Some(Said { wrong, then, install_at: None, in_settings: false });
        }
        Reach::Pushes { .. }
        | Reach::CouldntTell { .. }
        | Reach::MachineOwn { .. }
        | Reach::NotOnGithub
        | Reach::Unknown => return None,
    };
    let wrong = format!("Agents can't push to {repository} as {username}, the default GitHub account.");
    Some(Said { wrong, then, install_at, in_settings: true })
}

/// What to do once GitHub refused a push to `repository` as `username`, saying `reason`. A token's or a sign-in's
/// account is given access by the repository's owner. An app of your own's `installation` is given it where the app
/// is installed, when that's the repository's owner, and otherwise can't be, since its bot reaches only the
/// repositories of the owner it's installed on.
fn refused(
    repository: &GithubRepositoryName,
    username: &GithubUsername,
    reason: &str,
    installation: Option<&InstalledApp>,
) -> ToDo {
    let github_says = format!("GitHub says: {}", sentence(reason));
    let owner = repository.owner();
    let Some(InstalledApp { slug, on }) = installation else {
        let then = format!(
            "{github_says} Choose another default or connect an account that can push there, or ask {owner} to give \
             {username} access or to approve Roundhouse."
        );
        return (then, None);
    };
    if !installed_on_owner(repository, on) {
        let then = format!(
            "{github_says} Its app is installed only on {on}, so it can't push to {owner}'s repositories. Choose \
             another default or connect an account that can push there."
        );
        return (then, None);
    }
    let then = format!(
        "{github_says} Install the app on {owner} with access to {repository}, choose another default, or connect an \
         account that can push there."
    );
    (then, Some(install_page(slug)))
}

/// What to do once GitHub has no repository `repository` that `username` can see, as [`refused`] says.
fn not_found(
    repository: &GithubRepositoryName,
    username: &GithubUsername,
    installation: Option<&InstalledApp>,
) -> ToDo {
    let none = format!("GitHub has no such repository that {username} can see.");
    let owner = repository.owner();
    let Some(InstalledApp { slug, on }) = installation else {
        let then = format!(
            "{none} Check that it's where origin pushes, choose another default or connect an account that can see \
             it, or ask {owner} to give {username} access."
        );
        return (then, None);
    };
    if !installed_on_owner(repository, on) {
        let then = format!(
            "{none} Its app is installed only on {on}, so it can't see {owner}'s private repositories. Check that \
             it's where origin pushes, choose another default, or connect an account that can see it."
        );
        return (then, None);
    }
    let then = format!(
        "{none} Check that it's where origin pushes, install the app on {owner} with access to it, choose another \
         default, or connect an account that can see it."
    );
    (then, Some(install_page(slug)))
}

/// What to do once GitHub didn't take the default's token: connect it again, and for an app of your own's
/// `installation`, install the app again where it was.
fn token_refused(installation: Option<&InstalledApp>) -> ToDo {
    let Some(InstalledApp { slug, on }) = installation else {
        let then = "GitHub doesn't take its token, as when it has been revoked there. Connect it again or sign in with \
                    GitHub again, or choose another default.";
        return (then.to_string(), None);
    };
    let then = format!(
        "GitHub doesn't take its token, as when the app has been uninstalled from {on}. Install it there again and \
         connect it again, or choose another default."
    );
    (then, Some(install_page(slug)))
}

/// Whether `repository` is of the owner an app is installed `on`, as GitHub compares names, without case.
fn installed_on_owner(repository: &GithubRepositoryName, on: &GithubUsername) -> bool {
    repository.owner().eq_ignore_ascii_case(on.as_str())
}

/// `words` ending as a sentence does, so what follows them starts one of its own.
fn sentence(words: &str) -> String {
    let words = words.trim();
    if words.ends_with(['.', '!', '?', '…']) { words.to_string() } else { format!("{words}.") }
}

impl WorkspacePane {
    /// A line above the composer while agents can't push to the workspace's repository on GitHub as the default, with
    /// a way to Git in Settings, where what it says to do is done, if that's where, and to GitHub's page for installing
    /// the default's app, when it's done there.
    pub(super) fn push_line(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let Said { wrong, then, install_at, in_settings } = said(self.shown.github_reach()?)?;
        let colors = look::colors(cx);
        // One paragraph, so what to do wraps on from what's wrong, which only its colour sets apart.
        let warned = (0..wrong.len(), HighlightStyle { color: Some(colors.warning), ..HighlightStyle::default() });
        let words = StyledText::new(SharedString::from(format!("{wrong} {then}"))).with_highlights([warned]);
        let size = look::ControlSize::Small;
        let install = install_at.map(|page| {
            look::button(INSTALL_APP, look::ButtonKind::Borderless, size, "Install on GitHub")
                .icon(Lucide::ExternalLink)
                .on_click(move |_, _, cx| cx.open_url(&page))
        });
        Some(
            h_flex()
                .region(Region::PushLine)
                .flex_none()
                .w_full()
                .px(look::Space::Medium)
                .py(look::Space::XSmall)
                .gap(look::Space::Small)
                .line_over(cx)
                .typeset(look::Type::Detail)
                .text_color(colors.text_muted)
                .child(div().flex_1().min_w_0().child(words))
                .children(install)
                // Out past its own room, so its words end as far in as the line's start, as the setup line's do.
                .children(in_settings.then(|| {
                    look::button(OPEN_GIT, look::ButtonKind::Borderless, size, "Open Git")
                        .stands_out(&[look::Side::Right])
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(PaneEvent::OpenGit)))
                }))
                .into_any_element(),
        )
    }
}
