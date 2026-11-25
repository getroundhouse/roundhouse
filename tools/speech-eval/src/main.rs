//! `speech-eval` scores dictation's model on real recordings, by hand on a Mac, since the model is 2.4 GB and runs only
//! there. Each clip is heard as the app hears a recording, three times: with no hint words, with only the terms said,
//! and with them among unrelated words, as a field gets them. What's written, final and live, is scored for the terms
//! spelled right, the hinted words written that weren't said, and the errors in everything else. `compare` sets a run
//! against the baseline checked in beside this crate, so a change to the hints, their boost or the model shows what it
//! gained and lost, and `hear` hears files given by hand. `kept` hears the dictations
//! someone chose to keep, as they were heard or with other settings, and lists those that come out differently. `docs/reference/commands.md` says how to run it.

#![warn(clippy::wildcard_enum_match_arm)]

mod args;
mod audio;
mod cache;
mod clip;
mod compare;
mod hear_all;
mod hear_files;
mod hearing;
mod kept;
mod lines;
mod model;
mod results;
mod running;
mod score;
mod sets;

#[cfg(test)]
mod tests;

use std::process::ExitCode;

use clap::Parser;

use crate::args::{Cli, Command, KeptCommand, ModelArgs};
use crate::cache::Cache;
use crate::clip::Set;
use crate::results::{Baseline, Run};

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Command::Run { out, sets, clips, all, boost, model } => {
            let sets = if sets.is_empty() { Set::ALL.to_vec() } else { sets };
            let counts: Vec<(Set, Option<usize>)> = sets
                .into_iter()
                .map(|set| (set, if all { None } else { clips.or(running::clips_heard(set)) }))
                .collect();
            let (files, cache) = model_and_cache(model)?;
            running::run(&out, &counts, boost, &files, &cache)
        }
        Command::Hear { files, hints, language, engine_only, model } => {
            let (model, _cache) = model_and_cache(model)?;
            hear_files::hear(&files, hints, language.as_deref(), engine_only, &model)
        }
        Command::Compare { run, baseline } => {
            print!("{}", compare::compare(&Run::read(&run)?, &Baseline::read(&baseline)?));
            Ok(())
        }
        Command::Kept { command: KeptCommand::Hear { recordings, out, boost, no_hints, model } } => {
            let (files, _cache) = model_and_cache(model)?;
            let as_heard = kept::Settings { boost, no_hints };
            kept::hear(&recordings, &out, as_heard, &files)
        }
        Command::Kept { command: KeptCommand::Compare { run, other } } => {
            print!("{}", kept::compare(&run, other.as_deref())?);
            Ok(())
        }
        Command::Kept { command: KeptCommand::Sent { recordings } } => {
            print!("{}", kept::sent(&recordings)?);
            Ok(())
        }
        Command::Accept { run, baseline } => Run::read(&run)?.baseline().write(&baseline),
    }
}

fn model_and_cache(args: ModelArgs) -> anyhow::Result<(std::path::PathBuf, Cache)> {
    let cache = Cache::open(&args.cache.map_or_else(cache::default_dir, Ok)?)?;
    Ok((model::files(args.model, &cache)?, cache))
}
