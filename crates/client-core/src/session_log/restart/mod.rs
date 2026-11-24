//! A log that learns how far it goes, or reads its events again where they are when a hub of another build may have
//! rewritten them.

mod reading;

#[cfg(test)]
mod tests;
