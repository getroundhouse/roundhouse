//! What a push from the workspace would meet on GitHub, as the hub last checked its repository: a line above the
//! composer while agents can't push there as the default GitHub account, which says why and what to do, and blocks
//! nothing.

use gpui_kit::component::h_flex;
use gpui_kit::{AnyElement, Context, HighlightStyle, IntoElement as _, ParentElement as _, SharedString, Styled as _};
use gpui_kit::{StyledText, div};
use look::{InRegion as _, Region, Typeset as _};
use protocol::model::{GithubRepositoryName, GithubUsername, Reach};

use super::{PaneEvent, WorkspacePane};

/// The line's button, which opens Git in Settings.
pub const OPEN_GIT: &str = "push-line-git";

/// What the line says.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Said {
    /// What's wrong, in a warning's colour.
    pub(super) wrong: String,
    /// GitHub's words, if it gave any, and what to do, quieter, on from it.
    pub(super) then: String,
}

/// What a workspace says of its repository's `reach`, if anything. Nothing while a push would be taken, or the hub
/// couldn't tell, since a push that fails then says why itself, or while the default is each machine's own, which the
/// hub has no token of to check, or the repository isn't on GitHub. *Unconfirmed.*
pub(super) fn said(reach: &Reach) -> Option<Said> {
    let cant = |repository: &GithubRepositoryName, username: &GithubUsername| {
        format!("Agents can't push to {repository} as {username}, the default GitHub account.")
    };
    let said = match reach {
        Reach::Refused { repository, username, reason } => Said {
            wrong: cant(repository, username),
            then: format!(
                "GitHub says: {} Choose another default or connect an account that can push there, or ask {} to give \
                 {username} access or to approve Roundhouse.",
                sentence(reason),
                repository.owner(),
            ),
        },
        Reach::NotFound { repository, username } => Said {
            wrong: cant(repository, username),
            then: format!(
                "GitHub has no such repository that {username} can see. Check that it's where origin pushes, choose \
                 another default or connect an account that can see it, or ask {} to give {username} access.",
                repository.owner(),
            ),
        },
        Reach::TokenRefused { repository, username } => Said {
            wrong: cant(repository, username),
            then: "GitHub doesn't take its token, as when it has been revoked there. Connect it again or sign in with \
                   GitHub again, or choose another default."
                .to_string(),
        },
        // The block's reason says why and what to do, as git says it as it refuses the push.
        Reach::Blocked { repository, reason } => {
            Said { wrong: format!("Agents can't push to {repository}."), then: sentence(reason) }
        }
        Reach::Pushes { .. }
        | Reach::CouldntTell { .. }
        | Reach::MachineOwn { .. }
        | Reach::NotOnGithub
        | Reach::Unknown => return None,
    };
    Some(said)
}

/// `words` ending as a sentence does, so what follows them starts one of its own.
fn sentence(words: &str) -> String {
    let words = words.trim();
    if words.ends_with(['.', '!', '?', '…']) { words.to_string() } else { format!("{words}.") }
}

impl WorkspacePane {
    /// A line above the composer while agents can't push to the workspace's repository on GitHub, with a way to Git
    /// in Settings, where what it says to do is done.
    pub(super) fn push_line(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let Said { wrong, then } = said(self.shown.github_reach()?)?;
        let colors = look::colors(cx);
        // One paragraph, so what to do wraps on from what's wrong, which only its colour sets apart.
        let warned = (0..wrong.len(), HighlightStyle { color: Some(colors.warning), ..HighlightStyle::default() });
        let words = StyledText::new(SharedString::from(format!("{wrong} {then}"))).with_highlights([warned]);
        Some(
            h_flex()
                .region(Region::PushLine)
                .flex_none()
                .w_full()
                .px(look::Space::Medium)
                .py(look::Space::XSmall)
                .gap(look::Space::Small)
                .border_t_1()
                .border_color(colors.edge)
                .typeset(look::Type::Detail)
                .text_color(colors.text_muted)
                .child(div().flex_1().min_w_0().child(words))
                // Out past its own room, so its words end as far in as the line's start, as the setup line's do.
                .child(
                    look::button(OPEN_GIT, look::ButtonKind::Borderless, look::ControlSize::Small, "Open Git")
                        .stands_out(&[look::Side::Right])
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(PaneEvent::OpenGit))),
                )
                .into_any_element(),
        )
    }
}
