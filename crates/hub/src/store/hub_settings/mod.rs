//! The hub's own settings, which are one record, and who the hub is.

#[cfg(test)]
mod tests;

mod settings_row;
pub(super) use settings_row::hub_settings;
