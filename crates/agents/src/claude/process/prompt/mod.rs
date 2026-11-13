//! A prompt as the CLI takes it: one `user` line, with each attached file in it as far as the CLI can take it.
//!
//! Every file is in the worktree already, and the agent is told where in a tag that names it, so it can open the file
//! with its tools, in this turn or a later one. An image also goes in as a base64 image block, just before its tag, up
//! to what one prompt takes, and the CLI shrinks one bigger than its API takes. A short text file goes in whole,
//! inside its tag.
//! Anything else, such as a PDF, an archive or a long log, the agent opens from where it is. Files come before what
//! was typed, which is where Anthropic's guidance puts images for the best answers. `docs/claude-cli.md` has what was
//! checked.

#[cfg(test)]
use protocol::attachment::{AttachmentFile, MediaType};
#[cfg(test)]
use protocol::prompt::Prompt;
#[cfg(test)]
use serde_json::{Value, json};

#[cfg(test)]
mod tests;

mod message;
#[cfg(test)]
use message::INLINE_IMAGE_BYTES;
pub(super) use message::user_message;
pub(in crate::claude) use message::{image_block, user_line_saying};
