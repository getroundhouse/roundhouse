//! A log that learns how far it goes, starts again following other events when a hub reads them differently from the
//! one before, or reads the same events again where they are when a hub of another build may have rewritten them.

mod reading;

#[cfg(test)]
mod tests;
