//! What a kind of observation says of itself, which the store keeps every kind by, as `super::kinds` and
//! `super::github_kinds` have each say it.

use protocol::model::{Observation, ObservationKey, Observed, Rev};
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::keepers::Unkept;
use super::kept::Table;
use super::subjects::{GoesWith, Subject};
use super::tables::Tables;

/// A kind of observation. A new kind is added to [`Observation`] and [`ObservationKey`] in `protocol`, gets a type
/// in `super::kinds`, and a table in [`Tables`], whose `each` the compiler stops at.
pub(crate) trait Kind: Sized + 'static {
    /// What it's of, such as a workspace, which its table in the cache file keys it by, in its serde form.
    type Of: Subject;
    /// What it says, which its table in the cache file keeps in its serde form.
    type Value: Clone + PartialEq + Serialize + DeserializeOwned + Send + Sync;
    /// Its table in the cache file, and that table's version. Raise the version with any change to the serde form of
    /// [`Kind::Of`] or [`Kind::Value`], and a hub of the build that does empties this kind alone, rather than
    /// migrating it.
    const FORM: Form;
    /// Whether it's kept in memory alone, with no table in the cache file, since it doesn't outlast the hub, as which
    /// build a workspace's worker is doesn't, which the hub hears again of each worker it carries on with. Its
    /// [`Kind::FORM`] then only names it, in what the hub logs.
    const IN_MEMORY_ALONE: bool = false;
    /// What it is once nothing keeps it, as when the worker that watched it goes. A kind nothing keeps is as of its
    /// last read all the same.
    const UNKEPT: Unkept;

    /// It, as a client is sent it, or `None` for a kind the hub keeps and sends nowhere, which no list has, and whose
    /// change and removal tell nobody anything.
    fn observation(of: &Self::Of, said: Observed<Self::Value>, rev: Rev) -> Option<Observation>;

    /// Which it is, as its removal names it, or `None` for a kind sent nowhere, as [`Kind::observation`] says.
    fn key(of: &Self::Of) -> Option<ObservationKey>;

    /// Its table among the store's.
    fn table(tables: &mut Tables) -> &mut Table<Self>;

    /// Which of the hub's records the observation of `of` goes with, as what it's of says, but for a kind that goes
    /// with less than that, as a workspace's lines go with its worktree, which goes as it's archived.
    fn goes_with(of: &Self::Of) -> Option<GoesWith<'_>> {
        of.goes_with()
    }

    /// Whether `read` says what `held` says, so a client isn't told of it, and no revision moves: whether the two are
    /// equal, but for a kind that holds what clients aren't told of a change to, such as when each of an account's
    /// limits was heard, which memory and the file keep all the same.
    fn says_the_same(held: &Self::Value, read: &Self::Value) -> bool {
        held == read
    }
}

/// A kind's table in the cache file, and the version of its form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Form {
    pub(crate) table: &'static str,
    pub(crate) version: i64,
}
