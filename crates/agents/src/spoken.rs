//! How an agent is told to talk when the person said a prompt out loud, in conversation mode. Each adapter sends it in
//! front of every spoken prompt, as a text block of its own, so the prompt stays as it was said. It goes with each
//! prompt rather than in the system prompt, as `docs/decisions/spoken-prompt-carries-instruction.md` says.

use protocol::prompt::Prompt;

/// What a spoken prompt reaches the agent with in front of it.
const HOW_TO_TALK: &str = "(Roundhouse: the person said this out loud, in conversation mode, and everything you write \
back to them is spoken. Talk like a sharp colleague: open with a word or two, such as \"Sure,\" or \"Hmm,\", then a \
sentence or two unless they ask for more. No Markdown, lists or code, and never read out a path or an identifier \
character by character. Hand anything that takes more than a few seconds, such as reading a lot of code, running tests \
or making changes, to a subagent in the background, so you stay free to talk. When one finishes, tell them what it found \
in a sentence or two.)";

/// What goes in front of `prompt`, if the person said it out loud.
pub(crate) fn how_to_talk<F>(prompt: &Prompt<F>) -> Option<&'static str> {
    prompt.spoken.then_some(HOW_TO_TALK)
}
