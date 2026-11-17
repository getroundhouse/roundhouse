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
}

/// A version that isn't three numbers, which is all the release workflow publishes.
#[derive(Debug, thiserror::Error)]
#[error("{0} isn't a version the release workflow publishes, which is three numbers")]
pub struct BadVersion(String);

impl Feed {
    /// Each release in it, by its full package.
    pub fn releases(&self) -> impl Iterator<Item = Release<'_>> {
        self.packages
            .iter()
            .filter(|package| package.kind == Kind::Full)
            .map(|package| Release { version: &package.version, worker_versions: package.worker_versions })
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

/// Its numbers, in the order versions go in.
fn order_of(version: &str) -> Result<[u64; 3], BadVersion> {
    let bad = || BadVersion(version.to_string());
    let numbers: Vec<u64> = version.split('.').map(str::parse).collect::<Result<_, _>>().map_err(|_| bad())?;
    numbers.try_into().map_err(|_| bad())
}
