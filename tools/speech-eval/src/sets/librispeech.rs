//! [LibriSpeech](https://www.openslr.org/12)'s test-clean, under CC BY 4.0: passages of books read by many voices,
//! with [Le et al. 2021](https://arxiv.org/abs/2104.02194)'s lists of 100 words for each, under MIT: the rare words
//! said in it, among others that aren't, as published scores are measured. They aren't quite comparable: the model
//! writes numbers as digits and hyphens where the references spell them out, and nothing here makes the two agree.
//! Its archive is downloaded once and its recordings kept in the cache.

use std::io::Read as _;
use std::path::Path;

use anyhow::{Context as _, ensure};
use models::ModelFile;

use crate::cache::{Cache, Pinned};
use crate::clip::{Clip, Set, Source};

/// The archive's checksum, and the lists' revision of Meta's repository, pinned.
pub(crate) const LIBRISPEECH: &str = "test-clean.tar.gz 39fde525, fbai-speech 3fc7bb1e";

const ARCHIVE: Pinned = Pinned {
    url: "https://openslr.elda.org/resources/12/test-clean.tar.gz",
    file: ModelFile::new(
        "librispeech/test-clean.tar.gz",
        346_663_984,
        "39fde525e59672dc6d1551919b1478f724438a95aa55f874b576be21967e6c23",
    ),
};

const LISTS: Pinned = Pinned {
    url: "https://raw.githubusercontent.com/facebookresearch/fbai-speech/3fc7bb1ea20732fdb93613c66b14ea884debb443/is21_deep_bias/ref/test-clean.biasing_100.tsv",
    file: ModelFile::new(
        "librispeech/test-clean.biasing_100.tsv",
        3_549_833,
        "77f0be388c4f5821c71ee7cab65d1141b40b606e70053c18f0b11346a229d8e2",
    ),
};

/// Where the recordings are kept, each by its name, once they're out of the archive.
const RECORDINGS: &str = "librispeech/test-clean";

/// What's there once every recording is.
const ALL_OUT: &str = "librispeech/test-clean/.all-out";

/// Every clip in test-clean, with its list.
pub(super) fn test_clean(cache: &Cache) -> anyhow::Result<Vec<Clip>> {
    let lists = cache.file(&LISTS)?;
    take_out_recordings(cache)?;
    let text = std::fs::read_to_string(&lists).with_context(|| format!("couldn't read {}", lists.display()))?;
    text.lines()
        .enumerate()
        .map(|(i, line)| clip(line).with_context(|| format!("{}:{}", lists.display(), i + 1)))
        .collect()
}

/// A clip from a line of the lists: its name, what's said, the rare words said, and the list they're among.
fn clip(line: &str) -> anyhow::Result<Clip> {
    let columns: Vec<&str> = line.split('\t').collect();
    let [id, text, said, list] = columns[..] else { anyhow::bail!("not four columns") };
    let terms: Vec<String> = serde_json::from_str(said)?;
    let list: Vec<String> = serde_json::from_str(list)?;
    let unrelated = list.into_iter().filter(|word| !terms.contains(word)).collect();
    Ok(Clip {
        set: Set::Librispeech,
        id: id.to_string(),
        text: text.to_string(),
        terms,
        sound_alikes: Vec::new(),
        unrelated,
        audio: Source::Cached(format!("{RECORDINGS}/{id}.flac")),
    })
}

/// Takes every recording out of the archive, downloading it first, unless they're out already, and then lets go of
/// the archive, since only the recordings are heard.
fn take_out_recordings(cache: &Cache) -> anyhow::Result<()> {
    let dir = cache.dir();
    if dir.join(ALL_OUT).exists() {
        return Ok(());
    }
    let archive = cache.file(&ARCHIVE)?;
    eprintln!("taking LibriSpeech's recordings out of {}", archive.display());
    let file = std::fs::File::open(&archive).with_context(|| format!("couldn't open {}", archive.display()))?;
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
    let out = dir.join(RECORDINGS);
    std::fs::create_dir_all(&out).with_context(|| format!("couldn't make {}", out.display()))?;
    let mut count = 0;
    for entry in tar.entries().context("couldn't read LibriSpeech's archive")? {
        let mut entry = entry.context("couldn't read LibriSpeech's archive")?;
        let path = entry.path().context("a path in LibriSpeech's archive")?.into_owned();
        let Some(name) = flac_name(&path) else { continue };
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).with_context(|| format!("couldn't read {}", path.display()))?;
        let at = out.join(name);
        std::fs::write(&at, bytes).with_context(|| format!("couldn't write {}", at.display()))?;
        count += 1;
    }
    ensure!(count > 0, "LibriSpeech's archive has no recordings");
    std::fs::write(dir.join(ALL_OUT), count.to_string()).context("couldn't note the recordings are out")?;
    std::fs::remove_file(&archive).with_context(|| format!("couldn't remove {}", archive.display()))
}

/// A recording's name, if `path` in the archive is one.
fn flac_name(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    std::path::Path::new(name).extension().is_some_and(|ext| ext == "flac").then(|| name.to_string())
}
