use protocol::versions::Versions;
use serde::Deserialize;

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
    /// The archive of its hub. `None` on a channel that publishes none, and from a release from before releases did.
    pub hub: Option<&'a HubArchive>,
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
            hub: package.hub.as_ref(),
        })
    }

    /// Each release whose hub speaks workers of `oldest`, the oldest at work, so updating to it ends none of them, and
    /// an update crosses at most one break in the worker protocol, as `docs/principles/protocol.md` says.
    pub fn speaking(&self, oldest: u32) -> impl Iterator<Item = Release<'_>> {
        self.releases().filter(move |release| release.speaks(oldest))
    }

    /// The release a hub installed as a service, on version `running` with workers of `oldest` at work, would update
    /// to: the newest that's newer than it, speaks those workers, and has an archive of its hub. `None` when there's
    /// none, such as when it's the newest.
    pub fn hub_update(&self, running: &str, oldest: u32) -> Result<Option<Release<'_>>, BadVersion> {
        let running = order_of(running)?;
        let mut newest: Option<([u64; 3], Release<'_>)> = None;
        for release in self.speaking(oldest).filter(|release| release.hub.is_some()) {
            let order = order_of(release.version)?;
            if order > running && newest.is_none_or(|(at, _)| order > at) {
                newest = Some((order, release));
            }
        }
        Ok(newest.map(|(_, release)| release))
    }

    /// The newest release, which is the one an app with that release's workers at work steps from, if a hub that
    /// speaks `speaks` can't talk to those workers, so no release of that hub's build, or after it, can be the app's
    /// next step. A release from before releases said what they speak leaves nothing to check.
    pub fn stranded_by(&self, speaks: Versions) -> Result<Option<Release<'_>>, BadVersion> {
        let mut newest: Option<([u64; 3], Release<'_>)> = None;
        for release in self.releases() {
            let order = order_of(release.version)?;
            if newest.is_none_or(|(at, _)| order > at) {
                newest = Some((order, release));
            }
        }
        Ok(newest
            .map(|(_, release)| release)
            .filter(|release| release.worker_versions.is_some_and(|its| speaks.oldest > its.newest)))
    }
}

impl Release<'_> {
    /// Whether its hub speaks workers of `version`. One from before releases said may not.
    pub fn speaks(&self, version: u32) -> bool {
        self.worker_versions.is_some_and(|versions| versions.speaks(version))
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
