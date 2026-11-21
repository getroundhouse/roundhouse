//! The keys that step a view between its stops, `Option-Up` and `Option-Down`, and with Shift, which move the end of
//! what's selected to them: the actions they send, and what takes them on a view with stops, which are a chat, a
//! rendered Markdown file and a proposal. Each view says where its own steps go.

use gpui_kit::{Context, Div, InteractiveElement as _, Window, actions};

use crate::selection::Way;

actions!(transcript, [StepBack, StepOn, SelectStepBack, SelectStepOn]);

/// A view with stops in what it shows: the prompts of a chat, the headings of a file and the parts of a proposal.
pub trait Steps: Sized + 'static {
    /// Steps it `way`, to the stop before where it's read or the one after, as `Option-Up` and `Option-Down` do.
    fn step(&mut self, way: Way, cx: &mut Context<Self>);

    /// Moves the end of what's selected `way` to the start of a stop: back to the one before it, or to the start of
    /// the one it's in, and on to the one after, as `Option-Shift-Up` and `Option-Shift-Down` do.
    fn select_step(&mut self, way: Way, window: &mut Window, cx: &mut Context<Self>);
}

/// `view`, taking the keys that step it, for the view `V` that draws it.
pub(crate) fn with_step_keys<V: Steps>(view: Div, cx: &mut Context<V>) -> Div {
    view.on_action(cx.listener(|view: &mut V, _: &StepBack, _, cx| view.step(Way::Back, cx)))
        .on_action(cx.listener(|view: &mut V, _: &StepOn, _, cx| view.step(Way::On, cx)))
        .on_action(cx.listener(|view: &mut V, _: &SelectStepBack, window, cx| view.select_step(Way::Back, window, cx)))
        .on_action(cx.listener(|view: &mut V, _: &SelectStepOn, window, cx| view.select_step(Way::On, window, cx)))
}
