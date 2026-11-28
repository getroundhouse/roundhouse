use std::collections::{HashMap, HashSet};

use protocol::versions::Versions;
use serde::Deserialize;

use crate::Floors;

/// The feed's packages, each as far as what Velopack doesn't read goes, which serde reads as the feed has it.
#[derive(Debug, Deserialize)]
pub struct Feed {
    #[serde(rename = "Assets")]
    packages: Vec<Package>,
}

#[derive(Debug, Deserialize)]
struct Package {
    #[serde(rename = "Version")]
    version: String,
    #[serde(rename = "Type")]
    kind: Kind,
    /// On a full package, which versions of the worker protocol its release's hub speaks.
    #[serde(rename = "WorkerVersions")]
    worker_versions: Option<Versions>,
    /// On a full package, the oldest release its release updates from, if it says one.
    #[serde(rename = "UpdatesFrom")]
    updates_from: Option<String>,
    /// On a full package, the archive of its release's hub, on a channel that publishes one.
    #[serde(rename = "Hub")]
    hub: Option<HubArchive>,
}

/// The archive a hub installed as a service is installed and updated from, as the feed describes it, under its
/// signature. `tools/package/publish.py` writes it, with a SHA-1 too, which nothing here reads.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct HubArchive {
    /// Its name beside the feed.
    #[serde(rename = "FileName")]
    pub file_name: String,
    #[serde(rename = "Size")]
    pub size: u64,
    /// Its SHA-256, in hexadecimal.
    #[serde(rename = "SHA256")]
    pub sha256: String,
}

/// An archive's SHA-256 that isn't 64 hexadecimal digits.
#[derive(Debug, thiserror::Error)]
#[error("the archive's SHA-256 in the update feed, {0}, isn't 64 hexadecimal digits")]
pub struct BadChecksum(String);

impl HubArchive {
    /// Its SHA-256, as the bytes a download is checked against.
    pub fn sha256_bytes(&self) -> Result<[u8; 32], BadChecksum> {
        let bad = || BadChecksum(self.sha256.clone());
        let hex = self.sha256.as_bytes();
        // `from_str_radix` would take a sign too.
        if hex.len() != 64 || !hex.iter().all(u8::is_ascii_hexdigit) {
            return Err(bad());
        }
        let mut bytes = [0; 32];
        for (byte, pair) in bytes.iter_mut().zip(hex.as_chunks::<2>().0) {
            let pair = std::str::from_utf8(pair).map_err(|_| bad())?;
            *byte = u8::from_str_radix(pair, 16).map_err(|_| bad())?;
        }
        Ok(bytes)
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
enum Kind {
    Full,
    #[serde(other)]
    Other,
}

/// A release, as its full package says it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Release<'a> {
    pub version: &'a str,
    /// Which versions of the worker protocol its hub speaks. `None` from a release from before releases said.
    pub worker_versions: Option<Versions>,
    /// The oldest release it updates from, since its hub can't migrate the database of one before it. `None` when it
    /// updates from any.
    pub updates_from: Option<&'a str>,
    /// The archive of its hub. `None` on a channel that publishes none, and from a release from before releases did.
    pub hub: Option<&'a HubArchive>,
}

/// Where an update goes from: the release that's running, the release of the hub whose database the update's hub
/// migrates next, and the oldest version of the worker protocol among the workers the update has to keep, if it has to
/// keep any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Running<'a> {
    pub version: &'a str,
    pub hub: HubRelease<&'a str>,
    pub workers: Option<u32>,
}

/// Which release a hub is on, as far as an update knows. For an app, that's the hub it last heard from, which can be
/// older than the app while that hub waits to restart on the app's release, or the app's own once its hub has failed to
/// start. For a hub updating itself, it's its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HubRelease<V> {
    /// The app hasn't heard from its hub yet, as in the moment after it opens, so the hub's database may be older than
    /// any floor, and an update takes only a release that updates from any.
    NotHeard,
    On(V),
}

impl<V: AsRef<str>> HubRelease<V> {
    pub fn as_deref(&self) -> HubRelease<&str> {
        match self {
            Self::NotHeard => HubRelease::NotHeard,
            Self::On(version) => HubRelease::On(version.as_ref()),
        }
    }

    /// Whether an update from a hub on this release takes every release that an update from one on `was` takes, as
    /// far as their floors go. A hub not heard from takes the fewest, and one on a newer release takes as many or more.
    /// A version that isn't a release's, such as a build of a commit, is past no floor, as one not heard from is.
    pub fn past_every_floor_of<W: AsRef<str>>(&self, was: &HubRelease<W>) -> bool {
        // `None`, past no floor, comes before every release.
        self.release_order() >= was.release_order()
    }

    fn release_order(&self) -> Option<[u64; 3]> {
        match self.as_deref() {
            HubRelease::NotHeard => None,
            HubRelease::On(version) => order_of(version).ok(),
        }
    }
}

/// A version that isn't three numbers, which is all the release workflow publishes.
#[derive(Debug, thiserror::Error)]
#[error("{0} isn't a version the release workflow publishes, which is three numbers")]
pub struct BadVersion(String);

impl Feed {
    /// Each release in it, by its full package.
    pub fn releases(&self) -> impl Iterator<Item = Release<'_>> {
        self.packages.iter().filter(|package| package.kind == Kind::Full).map(|package| Release {
            version: &package.version,
            worker_versions: package.worker_versions,
            updates_from: package.updates_from.as_deref(),
            hub: package.hub.as_ref(),
        })
    }

    /// Each release an update from `running` can take, as [`Release::takes_over`] says.
    pub fn taking_over<'a>(&'a self, running: Running<'_>) -> impl Iterator<Item = Release<'a>> {
        self.releases().filter(move |release| release.takes_over(running))
    }

    /// The release a hub installed as a service, on version `running` with workers of `oldest` at work, would update
    /// to: the newest that's newer than it, takes over from it, and has an archive of its hub. `None` when there's
    /// none, such as when it's the newest.
    pub fn hub_update(&self, running: &str, oldest: u32) -> Result<Option<Release<'_>>, BadVersion> {
        // Its database is from its own release, which it migrated as it started.
        let from = Running { version: running, hub: HubRelease::On(running), workers: Some(oldest) };
        let running = order_of(running)?;
        let mut newest: Option<([u64; 3], Release<'_>)> = None;
        for release in self.taking_over(from).filter(|release| release.hub.is_some()) {
            let order = order_of(release.version)?;
            if order > running && newest.is_none_or(|(at, _)| order > at) {
                newest = Some((order, release));
            }
        }
        Ok(newest.map(|(_, release)| release))
    }

    /// Why a build with `floors` would leave an app on some release in this feed with none to step to, so that no
    /// release of that build, or after it, could be the app's next step, if it would. Every platform's apps keep their
    /// workers through an update, so they step through each break in the worker protocol. A release from before
    /// releases said what they speak leaves nothing to check of them.
    pub fn stranded_by(&self, floors: Floors) -> Result<Option<Stranded<'_>>, BadVersion> {
        let Some(newest) = self.newest()? else { return Ok(None) };
        let newest_speaks = newest.worker_versions.map(|its| its.newest);
        if newest_speaks.is_some_and(|its| floors.workers.oldest > its) {
            return Ok(Some(Stranded::Workers(newest)));
        }
        let Some(floor) = floors.release else { return Ok(None) };
        if order_of(floor)? > order_of(newest.version)? {
            return Ok(Some(Stranded::Release(newest)));
        }
        if !self.steps()?.contains(&order_of(floor)?) {
            return Ok(Some(Stranded::NotKept));
        }
        let mut last: Option<([u64; 3], Release<'_>)> = None;
        for release in self.releases() {
            let order = order_of(release.version)?;
            let before = release.worker_versions.is_some_and(|its| its.newest < floors.workers.oldest);
            if before && last.is_none_or(|(at, _)| order > at) {
                last = Some((order, release));
            }
        }
        let past = match last {
            Some((at, _)) => order_of(floor)? > at,
            None => true,
        };
        Ok(past.then_some(Stranded::PastTheWorkerStep(last.map(|(_, release)| release))))
    }

    /// The newest release.
    fn newest(&self) -> Result<Option<Release<'_>>, BadVersion> {
        let mut newest: Option<([u64; 3], Release<'_>)> = None;
        for release in self.releases() {
            let order = order_of(release.version)?;
            if newest.is_none_or(|(at, _)| order > at) {
                newest = Some((order, release));
            }
        }
        Ok(newest.map(|(_, release)| release))
    }

    /// The newest release of each floor, which the publisher keeps for an app to step to while it can't take a newer
    /// one, as `stepped_to` in `tools/package/publish.py` has them.
    fn steps(&self) -> Result<HashSet<[u64; 3]>, BadVersion> {
        let mut steps: HashMap<(u32, Option<[u64; 3]>), [u64; 3]> = HashMap::new();
        for release in self.releases() {
            let Some(speaks) = release.worker_versions else { continue };
            let floor = (speaks.oldest, release.updates_from.map(order_of).transpose()?);
            let order = order_of(release.version)?;
            steps.entry(floor).and_modify(|newest| *newest = order.max(*newest)).or_insert(order);
        }
        Ok(steps.into_values().collect())
    }
}

/// Why a build would leave an app with no release to step to, as [`Feed::stranded_by`] finds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stranded<'a> {
    /// The build's hub can't talk to the workers of the newest release, which an app with those at work is on.
    Workers(Release<'a>),
    /// The newest release is older than the oldest release the build updates from.
    Release(Release<'a>),
    /// The oldest release the build updates from isn't one the feed keeps for an app to step to, so a person whose hub
    /// refuses a database that old can't install it.
    NotKept,
    /// The oldest release the build updates from is past the last release whose hub doesn't speak the build's oldest
    /// workers, or there's none. An app from before releases said what they update from takes the build once its hub's
    /// workers are that new, which only a release after that last one runs, so a floor no newer carries it too.
    PastTheWorkerStep(Option<Release<'a>>),
}

impl Release<'_> {
    /// Whether an update from `running` can take it, which is whether it's past both of this release's floors. Its hub
    /// speaks the workers the update has to keep, so the update ends none of them and crosses at most one break in the
    /// worker protocol, as `docs/principles/versions.md` says. And `running`, and the hub it names, are at least the
    /// oldest release it updates from, so its hub can migrate the database, as "The database" in
    /// `docs/principles/state.md` says. A hub not heard from yet is past no floor. An app or a hub takes the newest
    /// release it can, so it steps through each floor.
    pub fn takes_over(&self, running: Running<'_>) -> bool {
        let hub = match running.hub {
            HubRelease::NotHeard => self.updates_from.is_none(),
            HubRelease::On(hub) => self.updates_from(hub),
        };
        running.workers.is_none_or(|oldest| self.speaks(oldest)) && self.updates_from(running.version) && hub
    }

    /// Whether its hub speaks workers of `version`. One from before releases said may not.
    pub fn speaks(&self, version: u32) -> bool {
        self.worker_versions.is_some_and(|versions| versions.speaks(version))
    }

    /// Whether it updates from release `version`, by version rather than as text. A version that isn't a release's,
    /// such as a build of a commit, isn't known to be past any floor, so only a release that says none updates from it.
    fn updates_from(&self, version: &str) -> bool {
        self.updates_from.is_none_or(
            |oldest| matches!((order_of(oldest), order_of(version)), (Ok(oldest), Ok(version)) if oldest <= version),
        )
    }
}

/// Whether `version` is a release's, three numbers, as the release workflow publishes, rather than a build of a commit.
pub fn is_release(version: &str) -> bool {
    order_of(version).is_ok()
}

/// Its numbers, in the order versions go in.
fn order_of(version: &str) -> Result<[u64; 3], BadVersion> {
    let bad = || BadVersion(version.to_string());
    let numbers: Vec<u64> = version.split('.').map(str::parse).collect::<Result<_, _>>().map_err(|_| bad())?;
    numbers.try_into().map_err(|_| bad())
}
