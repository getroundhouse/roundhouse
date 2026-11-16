mod host;

pub(in crate::app) use host::other_views;
pub(crate) use host::{Hubs, begin_selection, finish_selection, select};

#[cfg(test)]
pub(in crate::app) mod tests;
