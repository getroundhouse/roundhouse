//! Making a workspace from what's in the new-workspace dialog, as the hub keeps it: the workspace, its first chat, and
//! the prompt, carrying copies of the dialog's files, so a client sends none of them again. What's in the dialog stays
//! kept until the chat has the prompt, so a client that stops meanwhile loses nothing.

#[cfg(test)]
mod tests;

mod making;
