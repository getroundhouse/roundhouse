//! Reading an image in a worktree, so the hub can keep one that the agent's text points to. Text an agent writes is
//! untrusted, so only a file inside the worktree can be read, once every link on the way is followed, and only an
//! image the transcript draws, so nothing else in the worktree or on the machine can be read this way.

#[cfg(test)]
mod tests;

mod reading;

pub(crate) use reading::read;
