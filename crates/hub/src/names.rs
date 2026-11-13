//! Workspace names: the ones the hub picks from, how it picks one, and what makes a name, chosen or picked, safe to
//! use.

use std::fmt;

use protocol::model::BranchName;

/// The names the hub gives workspaces, in alphabetical order so they're easy to keep, and picked at random, as
/// [`Picked`] says. Each is a railway station somewhere in the world, as a roundhouse is a railway's building, with
/// an article about a station by that name on English Wikipedia. A station joins the list when its name is:
///
/// - One word, as English writes it in plain lowercase letters, such as `ryogoku` for Ryōgoku. One that needs an
///   accent, such as Chamartín, or two words joined, such as Flinders Street, is left out.
/// - Easy to say and to spell, since it's typed as a branch and a directory.
/// - Not a word that reads as meaning something on a branch, such as Union or Central.
/// - Not known for a disaster, such as Moorgate or Atocha.
/// - Still a station's name, so not Orsay, which is a museum now, or Britomart, which was renamed.
/// - Not a letter or two from another on the list, as Heuston is from `euston`.
pub(crate) const NAMES: &[&str] = &[
    "abando",
    "akabane",
    "akihabara",
    "amstel",
    "anhalter",
    "austerlitz",
    "barbican",
    "bercy",
    "blaak",
    "blackfriars",
    "bonaventure",
    "brignole",
    "buenavista",
    "byculla",
    "cadorna",
    "carnforth",
    "casement",
    "charbagh",
    "churchgate",
    "cockfosters",
    "colbert",
    "connolly",
    "cornavin",
    "corrour",
    "crewe",
    "dadar",
    "dammtor",
    "delicias",
    "deutz",
    "didcot",
    "diridon",
    "ebisu",
    "egmore",
    "embarcadero",
    "entrecampos",
    "euston",
    "farringdon",
    "finse",
    "gambir",
    "ghum",
    "gornergrat",
    "gotanda",
    "gubeng",
    "hakata",
    "howrah",
    "ikebukuro",
    "jolimont",
    "kamalapur",
    "kanda",
    "kazansky",
    "keleti",
    "kursky",
    "lambrate",
    "manggarai",
    "maradana",
    "matabiau",
    "meguro",
    "meidling",
    "mergellina",
    "millbrae",
    "myrdal",
    "nakano",
    "namba",
    "nippori",
    "nyugati",
    "odenplan",
    "oerlikon",
    "ogilvie",
    "osaki",
    "ostiense",
    "ostkreuz",
    "otsuka",
    "paddington",
    "pasila",
    "pearse",
    "perrache",
    "piccadilly",
    "pimlico",
    "plunkett",
    "praterstern",
    "rannoch",
    "recoletos",
    "redfern",
    "retiro",
    "ribblehead",
    "rifredi",
    "rogoredo",
    "rossio",
    "ryogoku",
    "sannomiya",
    "sants",
    "sealdah",
    "secaucus",
    "shibuya",
    "shimbashi",
    "shinagawa",
    "shinjuku",
    "strathfield",
    "sugamo",
    "suseo",
    "tamachi",
    "tanggula",
    "tennoji",
    "termini",
    "tiburtina",
    "tugu",
    "ueno",
    "umeda",
    "upminster",
    "vauxhall",
    "vitebsky",
    "waterloo",
    "waverley",
    "wynyard",
    "yongsan",
    "yoyogi",
];

/// A name the hub picked: a station from [`NAMES`], with a number after it, from 2 up, when the station alone is
/// taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Picked {
    station: &'static str,
    number: Option<u32>,
}

impl Picked {
    /// The station `random` falls on, of those no workspace in `taken` has, so names picked one after another are
    /// seldom alike. Once every one is taken, the station it falls on, of them all, with the lowest number nobody has.
    pub(crate) fn new(taken: &[String], random: usize) -> Self {
        let free: Vec<&'static str> = NAMES.iter().copied().filter(|station| !is_taken(taken, station)).collect();
        if free.is_empty() {
            return Self { station: NAMES[random % NAMES.len()], number: None }.next(taken);
        }
        Self { station: free[random % free.len()], number: None }
    }

    /// The same station, with the lowest number after this one's that nobody in `taken` has. It only goes up, since a
    /// name the worker refused, for a branch or a folder the hub doesn't know of, leaves the store once the workspace
    /// takes the next, and going back down would try it again.
    pub(crate) fn next(self, taken: &[String]) -> Self {
        let mut number = self.number.map_or(2, |number| number + 1);
        loop {
            let next = Self { number: Some(number), ..self };
            if !is_taken(taken, &next.to_string()) {
                return next;
            }
            number += 1;
        }
    }
}

impl fmt::Display for Picked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.number {
            None => f.write_str(self.station),
            Some(number) => write!(f, "{}-{number}", self.station),
        }
    }
}

fn is_taken(taken: &[String], name: &str) -> bool {
    taken.iter().any(|t| t == name)
}

/// Goes into a path and a branch name, so it stays conservative.
#[expect(clippy::case_sensitive_file_extension_comparisons, reason = "git refuses a ref ending in exactly `.lock`")]
pub(crate) fn is_valid(name: &str) -> bool {
    let mut chars = name.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_alphanumeric());
    first_ok
        && name.len() <= 64
        && !name.contains("..")
        && !name.ends_with('.')
        && !name.ends_with(".lock")
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// The branch a workspace named `name` starts on. Its branches go in a folder of its name, so the branches its agents
/// make sit under it, and one prefix finds them all. Git can't have a branch `euston` beside a folder `euston/`, so the
/// first has a name of its own in the folder.
pub(crate) fn start_branch(name: &str) -> BranchName {
    BranchName::from(format!("{name}/start"))
}

#[cfg(test)]
mod tests {
    use proptest::collection::vec;
    use proptest::prelude::*;
    use proptest::test_runner::{Config, RngSeed};

    use super::*;

    fn taken(names: &[&str]) -> Vec<String> {
        names.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn a_name_picked_is_a_station_nobody_has_and_any_can_be() {
        let all_but_one = taken(&NAMES[..NAMES.len() - 1]);

        let picked = Vec::from_fn(NAMES.len(), |random| Picked::new(&[], random).to_string());

        assert_eq!(picked, NAMES);
        assert!((0..10).all(|random| Picked::new(&all_but_one, random).to_string() == NAMES[NAMES.len() - 1]));
    }

    #[test]
    fn once_every_station_is_taken_one_gets_the_lowest_number_nobody_has_with_it() {
        let mut every = taken(NAMES);

        assert_eq!(Picked::new(&every, 3).to_string(), format!("{}-2", NAMES[3]));
        every.push(format!("{}-2", NAMES[3]));
        assert_eq!(Picked::new(&every, 3).to_string(), format!("{}-3", NAMES[3]));
    }

    proptest! {
        #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

        /// Taken names drawn from the stations, or every one of them, and from a few numbers of each, so a pick from
        /// the stations free and one from the numbers both come up.
        #[test]
        fn a_name_picked_and_the_one_after_it_are_the_lowest_nobody_has(
            every_station: bool,
            stations in vec(0..NAMES.len(), 0..NAMES.len()),
            numbered in vec((0..NAMES.len(), 2..6_u32), 0..40),
            random: usize,
        ) {
            let mut taken: Vec<String> = if every_station {
                taken(NAMES)
            } else {
                stations.iter().map(|at| NAMES[*at].to_string()).collect()
            };
            taken.extend(numbered.iter().map(|(at, number)| format!("{}-{number}", NAMES[*at])));
            let is_taken = |station: &str, number: u32| taken.contains(&format!("{station}-{number}"));

            let picked = Picked::new(&taken, random);
            let next = picked.next(&taken);

            prop_assert!(!taken.contains(&picked.to_string()), "{picked}");
            prop_assert_eq!(picked.number.is_none(), NAMES.iter().any(|station| !taken.iter().any(|t| t == station)));
            let from = picked.number.unwrap_or(1);
            prop_assert!((2..from).all(|lower| is_taken(picked.station, lower)), "{picked} isn't the lowest");
            let to = next.number.unwrap_or_default();
            prop_assert_eq!(next.station, picked.station);
            prop_assert!(to > from && !taken.contains(&next.to_string()), "{next}");
            prop_assert!((from + 1..to).all(|skipped| is_taken(picked.station, skipped)), "{next} skipped one");
        }
    }

    /// The name the workspace has is in the store as it's refused, and the one before isn't once it's renamed, so going
    /// up is what keeps a workspace from trying a number twice.
    #[test]
    fn the_next_name_is_the_same_station_with_the_next_number_nobody_has() {
        let first = Picked::new(&[], 0);
        let station = NAMES[0];

        let second = first.next(&taken(&[station]));
        let third = second.next(&taken(&[&format!("{station}-3")]));

        assert_eq!(first.to_string(), station);
        assert_eq!(second.to_string(), format!("{station}-2"));
        assert_eq!(third.to_string(), format!("{station}-4"), "past one taken");
        assert_eq!(third.next(&[]).to_string(), format!("{station}-5"), "up, though a lower one is free");
    }

    #[test]
    fn every_default_name_is_valid() {
        assert!(NAMES.iter().all(|n| is_valid(n)));
    }

    /// The list is kept by hand, and a name listed twice would be picked twice as often as the rest.
    #[test]
    fn default_names_are_in_order_and_each_is_listed_once() {
        let out_of_order: Vec<_> = NAMES.windows(2).filter(|pair| pair[0] >= pair[1]).collect();
        assert!(out_of_order.is_empty(), "{out_of_order:?}");
    }

    /// So no two differ only in case, which a folder on a Mac or Windows can't tell apart.
    #[test]
    fn every_default_name_is_plain_lowercase_letters() {
        let not_plain: Vec<_> = NAMES.iter().filter(|name| !name.bytes().all(|b| b.is_ascii_lowercase())).collect();
        assert!(not_plain.is_empty(), "{not_plain:?}");
    }

    #[test]
    fn rejects_names_that_could_escape_a_directory_or_break_git() {
        for bad in ["", "../up", "a/b", "-flag", ".hidden", "a..b", "trailing.", "x.lock", "has space", &"x".repeat(65)]
        {
            assert!(!is_valid(bad), "{bad}");
        }
        for good in ["euston", "fix-login", "v2.1_test"] {
            assert!(is_valid(good), "{good}");
        }
    }
}
