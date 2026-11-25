//! What `speech-eval` is asked to do.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::clip::Set;

/// The baseline, beside the eval.
pub(crate) const BASELINE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/baseline.json");

#[derive(Parser)]
#[command(about = "Scores dictation's model on real recordings, by hand on a Mac")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Hears the sets' clips under each condition, into a run's folder, going on from where a run in it stopped.
    Run {
        /// The run's folder, outside the repository.
        #[arg(long)]
        out: PathBuf,
        /// The sets to hear; every set unless given.
        #[arg(long = "set", value_enum)]
        sets: Vec<Set>,
        /// How many clips of each set to hear, spread through it, rather than each set's own number: 300 of
        /// Tech-Sentences, 100 of `LibriSpeech`, and all of the sound-alikes.
        #[arg(long, conflicts_with = "all")]
        clips: Option<usize>,
        /// Hears every clip of each set, which takes an hour and a half.
        #[arg(long)]
        all: bool,
        /// The hint boost, rather than the app's.
        #[arg(long)]
        boost: Option<f32>,
        #[command(flatten)]
        model: ModelArgs,
    },
    /// Hears WAV or FLAC files, and prints what was written of each, live and final, as a JSON line.
    Hear {
        files: Vec<PathBuf>,
        /// A word to bias it toward, as a field would hint it.
        #[arg(long = "hint")]
        hints: Vec<String>,
        /// The language's code, such as `en`, rather than leaving the model to tell.
        #[arg(long)]
        language: Option<String>,
        /// Hears each file with the engine alone, live and then whole, rather than through dictation, which cuts a
        /// recording at its pauses, so a problem can be put down to one or the other. A file is then no longer than
        /// 15 s.
        #[arg(long)]
        engine_only: bool,
        #[command(flatten)]
        model: ModelArgs,
    },
    /// Sets a run against the baseline, in Markdown for a pull request.
    Compare {
        run: PathBuf,
        #[arg(long, default_value = BASELINE)]
        baseline: PathBuf,
    },
    /// Hears dictations someone chose to keep, and sets what's written of them against what was. Nothing from it goes in
    /// the repository.
    Kept {
        #[command(subcommand)]
        command: KeptCommand,
    },
    /// Makes a run the baseline.
    Accept {
        run: PathBuf,
        #[arg(long, default_value = BASELINE)]
        baseline: PathBuf,
    },
}

#[derive(Subcommand)]
pub(crate) enum KeptCommand {
    /// Hears each recording in a folder with the hint words, boost and language it was heard with, or with those
    /// given here, into a run's folder, going on from where a run in it stopped.
    Hear {
        /// The folder the app keeps recordings in, `recordings` in its data directory.
        recordings: PathBuf,
        /// The run's folder, outside the repository.
        #[arg(long)]
        out: PathBuf,
        /// The hint boost, rather than the one each was heard with.
        #[arg(long)]
        boost: Option<f32>,
        /// Hears each with no hint words.
        #[arg(long)]
        no_hints: bool,
        #[command(flatten)]
        model: ModelArgs,
    },
    /// Lists the recordings a run wrote differently from another run, or from what the app wrote as they were dictated.
    Compare { run: PathBuf, other: Option<PathBuf> },
    /// Sets what was sent against the recordings it came from, and lists those whose text was changed or left out.
    Sent {
        /// The folder the app keeps recordings in.
        recordings: PathBuf,
    },
}

/// Where the model and the datasets are.
#[derive(clap::Args)]
pub(crate) struct ModelArgs {
    /// A folder with the model's files, as the app keeps them, rather than downloading them into the cache.
    #[arg(long)]
    pub(crate) model: Option<PathBuf>,
    /// The cache, rather than `~/Library/Caches/roundhouse-speech-eval`.
    #[arg(long)]
    pub(crate) cache: Option<PathBuf>,
}
