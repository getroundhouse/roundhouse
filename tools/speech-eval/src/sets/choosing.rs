//! Which of a set's clips a run hears, and which unrelated words each is heard among.

use std::collections::BTreeSet;

use super::{SoundAlike, librispeech, tech_sentences};
use crate::cache::Cache;
use crate::clip::{Clip, Set};

/// How many words a clip is heard among, said and not, as Le et al.'s shortest lists have.
pub(crate) const AMONG: usize = 100;

/// `count` of `set`'s clips, or all of them, spread evenly through it: the same ones on every run, for as long as
/// the set's revision and `count` are the same.
pub(crate) fn clips(
    set: Set,
    count: Option<usize>,
    sound_alikes: &[SoundAlike],
    cache: &Cache,
) -> anyhow::Result<Vec<Clip>> {
    let mut clips = match set {
        Set::TechSentences => tech_sentences::test(cache)?,
        Set::Librispeech => librispeech::test_clean(cache)?,
        Set::SoundAlikes | Set::Capitals => tech_sentences::sound_alikes(set, sound_alikes, cache)?,
    };
    clips.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(spread(clips, count))
}

/// `count` of `all`, evenly spaced, or all of them if there aren't more.
pub(crate) fn spread<T>(all: Vec<T>, count: Option<usize>) -> Vec<T> {
    let Some(count) = count.filter(|&count| count < all.len()) else { return all };
    let total = all.len();
    let picked: BTreeSet<usize> = (0..count).map(|k| k * total / count).collect();
    all.into_iter().enumerate().filter(|(i, _)| picked.contains(i)).map(|(_, item)| item).collect()
}

/// The first `count` of `pool` in an order that's the clip's own, made from its name, so each clip has its own
/// unrelated words and has them on every run.
pub(crate) fn unrelated<'a>(id: &str, pool: impl Iterator<Item = &'a str>, count: usize) -> Vec<String> {
    let mut ranked: Vec<(u64, &str)> = pool.map(|word| (fnv(&[id, word]), word)).collect();
    ranked.sort_unstable();
    ranked.into_iter().take(count).map(|(_, word)| word.to_string()).collect()
}

/// FNV-1a's 64-bit hash of `parts`, each ended by a zero byte, which doesn't change between Rust's versions as its
/// own hasher can.
fn fnv(parts: &[&str]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in parts.iter().flat_map(|part| part.bytes().chain([0])) {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
    hash
}
