mod host;

pub(crate) use host::{Hubs, begin_selection, finish_selection, local_view, other_views, select, view_of};

#[cfg(test)]
pub(in crate::app) mod tests;
