//! The images the agent's text points to in the worktree, such as `![The new layout](docs/shot.png)`. The worker reads
//! each as the finished message passes through it, and sends it to the hub beside the event, so the hub keeps the image
//! as it was when the agent wrote about it. When the agent changes a file and points to it again, the later text gets
//! the file as it is then, and every client draws what each text showed, whatever happens to the worktree after.

mod addresses;

#[cfg(test)]
mod tests;

mod keeping;

#[cfg(test)]
use keeping::keep;
pub(crate) use keeping::keep_in;
