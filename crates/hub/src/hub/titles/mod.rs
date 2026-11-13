//! What workspaces and chats are called: titled from the first prompt, then by a generated title, or renamed by hand,
//! and a workspace titled again when someone asks. A title is generated from what was typed and the images that came
//! with it, since a prompt such as "how do we fix this?" can be about a screenshot.

#[cfg(test)]
use protocol::attachment::Attachment;
#[cfg(test)]
use protocol::model::{PromptLine, SessionId};

#[cfg(test)]
mod tests;

mod naming;
#[cfg(test)]
use naming::{MOST_IMAGE_BYTES, MOST_IMAGES, MOST_PROMPT_CHARS, WORK_SO_FAR, chosen_images, work_so_far};
