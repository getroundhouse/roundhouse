//! How each reply said out loud ended for the person hearing it. A record stays as long as its chat does.

#[cfg(test)]
mod tests;

mod reply_rows;
pub(super) use reply_rows::replies_heard_of;
