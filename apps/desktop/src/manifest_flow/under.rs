//! Where on GitHub an app of your own is made: under the person's own account, or an organization's, whose form for
//! making an app is at an address of its own. A private app can be installed only where it's made.

use thiserror::Error;

/// The longest name GitHub gives an organization.
const LONGEST: usize = 39;

/// Where an app of your own is made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Under {
    /// The account the person is signed in to GitHub as, in the browser, whoever that is.
    Account,
    Organization(Organization),
}

impl Under {
    /// GitHub's form for making an app from a manifest, under it.
    pub(crate) fn form(&self) -> String {
        match self {
            Under::Account => "https://github.com/settings/apps/new".to_string(),
            Under::Organization(organization) => {
                format!("https://github.com/organizations/{}/settings/apps/new", organization.0)
            }
        }
    }
}

/// An organization's name, as GitHub takes one, so it can go in the address of its form as it is: letters, numbers and
/// hyphens, one at a time, neither first nor last, and at most [`LONGEST`] of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Organization(String);

/// What's typed isn't a name GitHub gives an organization.
#[derive(Debug, Error, PartialEq, Eq)]
#[error("{0:?} isn't the name of an organization on GitHub")]
pub(crate) struct NotAnOrganization(pub(crate) String);

impl Organization {
    /// `name`, with any spaces around it left out, if it's one GitHub gives an organization.
    pub(crate) fn parse(name: &str) -> Result<Organization, NotAnOrganization> {
        let name = name.trim();
        let allowed = name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
        let hyphens_apart = !name.starts_with('-') && !name.ends_with('-') && !name.contains("--");
        if name.is_empty() || name.len() > LONGEST || !allowed || !hyphens_apart {
            return Err(NotAnOrganization(name.to_string()));
        }
        Ok(Organization(name.to_string()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}
