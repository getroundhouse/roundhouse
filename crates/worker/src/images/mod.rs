//! Reading an image or a video in a worktree, so the hub can keep one that the agent's text points to, as Markdown
//! points to an image. Text an agent writes is untrusted, so only a file inside the worktree can be read, once every
//! link on the way is followed, and only an image the transcript draws or a video it plays, so nothing else in the
//! worktree or on the machine can be read this way.

#[cfg(test)]
mod tests;

mod reading;

pub(crate) use reading::read;
