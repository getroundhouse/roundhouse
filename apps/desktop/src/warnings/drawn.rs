//! How a warning is drawn: what's wrong, after the warning's sign, in its colour, then why and what to do, quieter,
//! then its buttons, wherever it shows.

use std::rc::Rc;

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::{AnyElement, App, IntoElement as _, ParentElement as _, Pixels, Styled as _, Window, div, px};
use look::{IconSizing as _, Region, Typeset as _};

use super::{Answer, Warning};

/// How far the sign sits below the top of the line of body text it's beside, so it's centred on it, the line being
/// that much taller than the sign on each side. Not a space: it lines the sign up with the text.
const SIGN_DOWN: Pixels = px(2.);

/// What a warning's Open Settings does, which only the view that shows it can.
pub(crate) type OpenGit = Rc<dyn Fn(&mut Window, &mut App)>;

/// The warning's sign, in its colour, as big as a sign among body text.
pub(crate) fn sign(cx: &App) -> Icon {
    Icon::new(Lucide::TriangleAlert).flex_none().sized(look::IconSize::Regular).text_color(look::colors(cx).warning)
}

/// `warning`, its Open Settings doing what `open_git` does.
pub(crate) fn drawn(warning: &Warning, open_git: &OpenGit, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    // Beside what's wrong, at its first line, so a second line starts where the first does.
    let wrong = h_flex()
        .items_start()
        .gap(look::Space::Small)
        .child(div().flex_none().pt(SIGN_DOWN).child(sign(cx)))
        .child(div().flex_1().min_w_0().emphasis().text_color(colors.warning).child(warning.wrong.clone()));
    let then = div().typeset(look::Type::Detail).text_color(colors.text_muted).child(warning.then.clone());
    // Borderless, as a banner's are, the first out past its own room, so its words start where the warning's do.
    let answers = warning.answers.iter().enumerate().map(|(at, answer)| {
        let button = button(answer, open_git);
        if at == 0 { button.stands_out(&[look::Side::Left]) } else { button }
    });
    // One part of a card, so as close together as a part's lines. Wherever it's shown, its words wrap, as the header it
    // can be opened from doesn't let its own.
    v_flex()
        .whitespace_normal()
        .gap(look::Space::XSmall)
        .child(wrong)
        .child(then)
        .child(h_flex().gap(look::Space::XSmall).children(answers))
        .into_any_element()
}

/// `warnings` alone, in a card over what's around it, as the pull request's bar opens it.
pub(crate) fn card(warnings: &[Warning], open_git: &OpenGit, cx: &App) -> AnyElement {
    let card = look::card(look::CardKind::Over, look::CardWidth::Regular).region(Region::WarningsCard);
    card.children(warnings.iter().map(|warning| drawn(warning, open_git, cx))).into_any_element()
}

fn button(answer: &Answer, open_git: &OpenGit) -> look::Button {
    let (kind, size) = (look::ButtonKind::Borderless, look::ControlSize::Small);
    match answer {
        Answer::OnGithub { label, page } => {
            let page = page.clone();
            look::button("warning-on-github", kind, size, *label)
                .icon(Lucide::ExternalLink)
                .on_click(move |_, _, cx| cx.open_url(&page))
        }
        Answer::OpenGit => {
            let open_git = open_git.clone();
            look::button("warning-open-git", kind, size, "Open Settings")
                .on_click(move |_, window, cx| open_git(window, cx))
        }
    }
}
