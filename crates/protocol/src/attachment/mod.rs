//! Files a person sends with a prompt, such as a screenshot, a log or a dataset, and images a tool sends the agent.
//!
//! A client adds each file to its session with `AttachmentAdd`, then names them in the prompt that carries them. The
//! prompt's event only says what they are, so reading a log stays as light as it was. Before the prompt reaches the
//! agent, the worker writes each file into the worktree, where the agent can open it, and the agent is also given in
//! the prompt itself whatever it can take there. The hub keeps only what the transcript draws, an image, and a client
//! fetches it with `AttachmentRead`.
//!
//! An image a tool sends back, such as a screenshot the agent reads, is an attachment of the session too. The worker
//! sends its bytes beside the event, as a [`NewAttachment`], and the event's result names it, so it's kept and read the
//! same way as a prompt's, and the log stays as light. So is an image in the worktree that the agent's text points to,
//! which the worker reads as the text comes, and the text's block names by the address the text gives it.

#[cfg(test)]
mod tests;

mod file;
pub use file::{
    Attachment, AttachmentBytes, AttachmentFile, AttachmentProblem, MAX_FILE_BYTES, MAX_PER_PROMPT, MediaType,
    NewAttachment, describe_size, file_name,
};
