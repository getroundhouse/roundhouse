//! Default workspace names. Short, easy to say, and valid as branch names and directory names.

const NAMES: &[&str] = &[
    "acacia",
    "alder",
    "almond",
    "apple",
    "ash",
    "aspen",
    "bamboo",
    "banyan",
    "baobab",
    "beech",
    "birch",
    "boxwood",
    "buckeye",
    "butternut",
    "camphor",
    "cedar",
    "cherry",
    "chestnut",
    "cottonwood",
    "cypress",
    "dogwood",
    "ebony",
    "elder",
    "elm",
    "eucalyptus",
    "fig",
    "fir",
    "ginkgo",
    "hackberry",
    "hawthorn",
    "hazel",
    "hemlock",
    "hickory",
    "holly",
    "hornbeam",
    "jacaranda",
    "juniper",
    "kapok",
    "larch",
    "laurel",
    "linden",
    "locust",
    "magnolia",
    "mahogany",
    "mango",
    "maple",
    "medlar",
    "mesquite",
    "mimosa",
    "mulberry",
    "myrtle",
    "oak",
    "olive",
    "palm",
    "pear",
    "pecan",
    "persimmon",
    "pine",
    "plane",
    "plum",
    "poplar",
    "quince",
    "redbud",
    "redwood",
    "rowan",
    "sequoia",
    "spruce",
    "sumac",
    "sycamore",
    "tamarack",
    "tamarind",
    "teak",
    "tupelo",
    "walnut",
    "willow",
    "yew",
];

/// A name not in `taken`. `salt` varies the starting point, and a suffix keeps it going forever.
pub(crate) fn pick(taken: &[String], salt: u64) -> String {
    let start = salt as usize % NAMES.len();
    for offset in 0..NAMES.len() {
        let name = NAMES[(start + offset) % NAMES.len()];
        if !taken.iter().any(|t| t == name) {
            return name.to_string();
        }
    }
    let base = NAMES[start];
    (2..).map(|n| format!("{base}-{n}")).find(|name| !taken.contains(name)).expect("the range is unbounded")
}

/// Goes into a path and a branch name, so it stays conservative.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_names_already_taken() {
        let taken = vec![NAMES[3].to_string()];
        assert_eq!(pick(&taken, 3), NAMES[4]);
    }

    #[test]
    fn keeps_working_after_every_name_is_used() {
        let mut taken: Vec<String> = NAMES.iter().map(ToString::to_string).collect();
        assert_eq!(pick(&taken, 0), format!("{}-2", NAMES[0]));
        taken.push(format!("{}-2", NAMES[0]));
        assert_eq!(pick(&taken, 0), format!("{}-3", NAMES[0]));
    }

    #[test]
    fn every_default_name_is_valid() {
        assert!(NAMES.iter().all(|n| is_valid(n)));
    }

    #[test]
    fn rejects_names_that_could_escape_a_directory_or_break_git() {
        for bad in ["", "../up", "a/b", "-flag", ".hidden", "a..b", "trailing.", "x.lock", "has space", &"x".repeat(65)]
        {
            assert!(!is_valid(bad), "{bad}");
        }
        for good in ["alder", "fix-login", "v2.1_test"] {
            assert!(is_valid(good), "{good}");
        }
    }
}
