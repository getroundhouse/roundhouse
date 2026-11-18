//! The language a recording is heard in, as the person chose it: one of the 25 languages the model hears, or Auto,
//! which leaves the engine to tell which.

/// The language a recording is heard in. Auto unless one is chosen, since a wrong one hears far worse than none: the
/// model told to hear English hears Russian with four times the errors, and Greek with half its words lost.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Language {
    #[default]
    Auto,
    Bulgarian,
    Croatian,
    Czech,
    Danish,
    Dutch,
    English,
    Estonian,
    Finnish,
    French,
    German,
    Greek,
    Hungarian,
    Italian,
    Latvian,
    Lithuanian,
    Maltese,
    Polish,
    Portuguese,
    Romanian,
    Russian,
    Slovak,
    Slovenian,
    Spanish,
    Swedish,
    Ukrainian,
}

impl Language {
    /// Every language, Auto first and then by name, as a list to choose one from has them.
    pub const ALL: [Self; 26] = [
        Self::Auto,
        Self::Bulgarian,
        Self::Croatian,
        Self::Czech,
        Self::Danish,
        Self::Dutch,
        Self::English,
        Self::Estonian,
        Self::Finnish,
        Self::French,
        Self::German,
        Self::Greek,
        Self::Hungarian,
        Self::Italian,
        Self::Latvian,
        Self::Lithuanian,
        Self::Maltese,
        Self::Polish,
        Self::Portuguese,
        Self::Romanian,
        Self::Russian,
        Self::Slovak,
        Self::Slovenian,
        Self::Spanish,
        Self::Swedish,
        Self::Ukrainian,
    ];

    /// Its name in English.
    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Bulgarian => "Bulgarian",
            Self::Croatian => "Croatian",
            Self::Czech => "Czech",
            Self::Danish => "Danish",
            Self::Dutch => "Dutch",
            Self::English => "English",
            Self::Estonian => "Estonian",
            Self::Finnish => "Finnish",
            Self::French => "French",
            Self::German => "German",
            Self::Greek => "Greek",
            Self::Hungarian => "Hungarian",
            Self::Italian => "Italian",
            Self::Latvian => "Latvian",
            Self::Lithuanian => "Lithuanian",
            Self::Maltese => "Maltese",
            Self::Polish => "Polish",
            Self::Portuguese => "Portuguese",
            Self::Romanian => "Romanian",
            Self::Russian => "Russian",
            Self::Slovak => "Slovak",
            Self::Slovenian => "Slovenian",
            Self::Spanish => "Spanish",
            Self::Swedish => "Swedish",
            Self::Ukrainian => "Ukrainian",
        }
    }

    /// Its ISO 639-1 code, such as `en`, or `auto`, for keeping the choice.
    pub fn code(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Bulgarian => "bg",
            Self::Croatian => "hr",
            Self::Czech => "cs",
            Self::Danish => "da",
            Self::Dutch => "nl",
            Self::English => "en",
            Self::Estonian => "et",
            Self::Finnish => "fi",
            Self::French => "fr",
            Self::German => "de",
            Self::Greek => "el",
            Self::Hungarian => "hu",
            Self::Italian => "it",
            Self::Latvian => "lv",
            Self::Lithuanian => "lt",
            Self::Maltese => "mt",
            Self::Polish => "pl",
            Self::Portuguese => "pt",
            Self::Romanian => "ro",
            Self::Russian => "ru",
            Self::Slovak => "sk",
            Self::Slovenian => "sl",
            Self::Spanish => "es",
            Self::Swedish => "sv",
            Self::Ukrainian => "uk",
        }
    }

    /// The language whose [`Language::code`] is `code`, if there's one.
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|language| language.code() == code)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::Language;

    #[test]
    fn each_language_is_listed_once_by_name_and_read_back_from_its_own_code() {
        let names: Vec<&str> = Language::ALL[1..].iter().map(|language| language.name()).collect();
        assert!(names.is_sorted(), "{names:?}");
        assert_eq!(Language::ALL[0], Language::Auto);
        let codes: HashSet<&str> = Language::ALL.iter().map(|language| language.code()).collect();
        assert_eq!(codes.len(), Language::ALL.len());
        for language in Language::ALL {
            assert_eq!(Language::from_code(language.code()), Some(language));
        }
        assert_eq!(Language::from_code("tlh"), None);
    }
}
