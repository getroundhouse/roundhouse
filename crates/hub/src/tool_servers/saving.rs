//! Saving a server as `ToolServerSave` has it: adding it, or saving an edit of it, once it has listed its tools, or at
//! once where there's nothing to ask it or the person chose to. The hub asks the server with no lock held, so what would
//! be asked is worked out again under the lock the change is made under, and the change is made only where that's what
//! was asked.

use protocol::client::tool_server_checks::ToolServerToCheck;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::ToolServerId;

use super::checking::{Checking, ToAsk};
use super::secret::{ToolServerSecret, refused};
use super::{ToolServerChange, ToolServers};
use crate::login_files::Locked;

/// When a save is made, as its client chose. It's `SaveWhen` without `Unknown`, which the hub refuses as the request
/// comes, so what saves a server matches only what it knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum When {
    /// Once the server has listed its tools, where there's anything to ask it.
    Listed,
    /// At once, without asking it anything, so running agents list its tools again.
    Anyway,
}

/// What a save comes to before its server is asked anything.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Saving {
    Done(Kept),
    /// The server is to be asked this first.
    Ask(ToAsk),
}

/// What a save that wasn't refused comes to.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Kept {
    Saved(ToolServerChange),
    /// It was let go of before it was saved, so nothing changed.
    LetGo,
}

impl ToolServers {
    /// Does what `of` says at once, where `when` says, or where there's nothing to ask of its server, unless `still`
    /// says it has been let go of, or says what the server is to be asked first. Refused as `ToolServerSave` says.
    pub(crate) fn save_or_ask(
        &self,
        of: &ToolServerToCheck,
        when: When,
        still: impl FnOnce() -> bool,
    ) -> Result<Saving, ApiError> {
        let saved_anyway = match when {
            When::Listed => None,
            When::Anyway => edited(of),
        };
        self.changing_saved(saved_anyway, |files| {
            if when == When::Listed
                && let Checking::Ask(to_ask) = self.checking_in(files, of.clone())?
            {
                return Ok(Saving::Ask(to_ask));
            }
            self.keep_saved(files, of, still).map(Saving::Done)
        })
    }

    /// Does what `of` says, whose server was asked `asked` and listed its tools, unless `still` says it has been let go
    /// of. Refused as `ToolServerSave` says, with `Conflict` where the server would now be asked anything else, as when
    /// it was moved or given another key meanwhile. Where there's now nothing to ask, as when another client made the
    /// same change meanwhile, it's done.
    pub(crate) fn save_asked(
        &self,
        of: &ToolServerToCheck,
        asked: &ToAsk,
        still: impl FnOnce() -> bool,
    ) -> Result<Kept, ApiError> {
        self.changing(|files| {
            match self.checking_in(files, of.clone())? {
                Checking::Ask(to_ask) if to_ask != *asked => {
                    let message = "the MCP server was changed while it was asked for its tools, so look at it again";
                    return Err(ApiError::new(ErrorCode::Conflict, message));
                }
                Checking::Ask(_) | Checking::Nothing => {}
            }
            self.keep_saved(files, of, still)
        })
    }

    /// Adds or edits as `of` says, under the servers' lock, `files`, unless `still` says it has been let go of. A cancel
    /// after `still` is too late.
    fn keep_saved(
        &self,
        files: &Locked<'_, ToolServerSecret>,
        of: &ToolServerToCheck,
        still: impl FnOnce() -> bool,
    ) -> Result<Kept, ApiError> {
        if !still() {
            return Ok(Kept::LetGo);
        }
        let change = match of.clone() {
            ToolServerToCheck::Add(add) => self.add_to(files, add)?,
            ToolServerToCheck::Edit(edit) => self.edit_in(files, edit)?,
            ToolServerToCheck::Unknown => return Err(refused("this hub doesn't know what that would save")),
        };
        Ok(Kept::Saved(change))
    }
}

/// The server `of` edits, if it's an edit.
fn edited(of: &ToolServerToCheck) -> Option<&ToolServerId> {
    match of {
        ToolServerToCheck::Edit(edit) => Some(&edit.id),
        ToolServerToCheck::Add(_) | ToolServerToCheck::Unknown => None,
    }
}
