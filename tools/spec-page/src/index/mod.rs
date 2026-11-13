//! What the page shows, read from the spec: each area's sections, with what each says, rule by rule, beside the
//! screens that show it, and whether each is confirmed. The page is drawn from it, and it's written beside the page
//! as JSON, so another viewer can be drawn from it too.
//!
//! A screen is an image on a paragraph of its own. Its caption is the paragraph right above it that starts in bold,
//! or above the run of images it's in, so a run shares one. A caption ending *Unconfirmed* is the owner's to
//! confirm, as a rule's is, and one without a caption shows as having none. A screen in a rule's list item, under
//! the rule, shows that rule. One on its own shows its caption.

#[cfg(test)]
mod tests;

mod document;
#[cfg(test)]
use document::listed;
pub(crate) use document::{Area, Block, Confirmation, Index, NOT_BUILT, Screen, Section, UNCONFIRMED, read};
#[cfg(test)]
pub(crate) use document::{Size, area};
