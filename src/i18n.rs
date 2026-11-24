//! Interface language. Texts live in `locales/app.yml` (Spanish and English side by side) and
//! are looked up with `rust_i18n::t!`, always with an explicit `locale` so the language is a
//! parameter like the current time: nothing reads a global, and tests stay deterministic.
//!
//! Only `env_override` and `system` touch the environment; `pick` decides, and is pure.

/// A language the interface speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    Es,
    /// Also the fallback for any language other than Spanish.
    #[default]
    En,
}

impl Lang {
    /// The locale code used in `locales/app.yml` (and in `t!(…, locale = …)`).
    pub fn code(self) -> &'static str {
        match self {
            Lang::Es => "es",
            Lang::En => "en",
        }
    }

    /// Spanish for any `es` tag (`es`, `es-CL`, `es_419`, `es-ES.UTF-8`), English otherwise.
    pub fn from_tag(tag: &str) -> Lang {
        let tag = tag.trim().to_ascii_lowercase();
        let primary = tag.split(['-', '_', '.', '@']).next().unwrap_or_default();
        if primary == "es" { Lang::Es } else { Lang::En }
    }

    /// Windows `LANGID`: its low 10 bits are the primary language (`LANG_SPANISH` = 0x0a).
    pub fn from_langid(langid: u16) -> Lang {
        if langid & 0x3ff == 0x0a {
            Lang::Es
        } else {
            Lang::En
        }
    }

    /// `es`/`en` only (from `CUANTOQUEDAPP_LANG`); anything else is `None`.
    pub fn parse_override(value: &str) -> Option<Lang> {
        match value.trim().to_ascii_lowercase().as_str() {
            "es" => Some(Lang::Es),
            "en" => Some(Lang::En),
            _ => None,
        }
    }
}

/// What the user picked in the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    /// Follow the system language.
    #[default]
    Auto,
    Spanish,
    English,
}

impl Language {
    /// The language to speak, given what the system says.
    pub fn resolve(self, system: Lang) -> Lang {
        match self {
            Language::Auto => system,
            Language::Spanish => Lang::Es,
            Language::English => Lang::En,
        }
    }
}

/// The language to speak: `CUANTOQUEDAPP_LANG` when set, then the user's setting, then the system.
pub fn pick(forced: Option<Lang>, language: Language, system: Lang) -> Lang {
    forced.unwrap_or_else(|| language.resolve(system))
}

/// `CUANTOQUEDAPP_LANG=es|en` forces the language (anything else is ignored).
pub fn env_override() -> Option<Lang> {
    std::env::var("CUANTOQUEDAPP_LANG")
        .ok()
        .and_then(|value| Lang::parse_override(&value))
}

/// The user's preferred language: macOS `NSLocale.preferredLanguages`, Windows
/// `GetUserDefaultUILanguage`, and `LC_ALL`/`LC_MESSAGES`/`LANG` elsewhere. Anything that is
/// not Spanish is English.
///
/// On macOS `LANG` is not used: an app opened from Finder or at login does not get it.
#[cfg(target_os = "macos")]
pub fn system() -> Lang {
    use objc2_foundation::NSLocale;
    NSLocale::preferredLanguages()
        .firstObject()
        .map(|tag| Lang::from_tag(&tag.to_string()))
        .unwrap_or_default()
}

#[cfg(windows)]
pub fn system() -> Lang {
    // SAFETY: no arguments, no pointers; it only returns the user's UI language id.
    let langid = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
    Lang::from_langid(langid)
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn system() -> Lang {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.is_empty())
        .map(|value| Lang::from_tag(&value))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, path::Path};

    use super::*;

    #[test]
    fn spanish_tags_of_any_region_are_spanish() {
        for tag in ["es", "es-CL", "es-419", "ES_es", "es_CL.UTF-8", " es-MX "] {
            assert_eq!(Lang::from_tag(tag), Lang::Es, "{tag}");
        }
    }

    #[test]
    fn every_other_tag_is_english() {
        for tag in [
            "en",
            "en-US",
            "pt-BR",
            "fr",
            "",
            "C",
            "POSIX",
            "esperanto",
            "et-EE",
        ] {
            assert_eq!(Lang::from_tag(tag), Lang::En, "{tag}");
        }
    }

    #[test]
    fn windows_language_ids_use_the_primary_language() {
        assert_eq!(Lang::from_langid(0x0c0a), Lang::Es); // es-ES
        assert_eq!(Lang::from_langid(0x340a), Lang::Es); // es-CL
        assert_eq!(Lang::from_langid(0x0409), Lang::En); // en-US
        assert_eq!(Lang::from_langid(0x0416), Lang::En); // pt-BR
    }

    #[test]
    fn the_override_accepts_only_es_and_en() {
        assert_eq!(Lang::parse_override("ES"), Some(Lang::Es));
        assert_eq!(Lang::parse_override("en"), Some(Lang::En));
        assert_eq!(Lang::parse_override("es-CL"), None);
        assert_eq!(Lang::parse_override(""), None);
    }

    #[test]
    fn the_variable_wins_then_the_setting_then_the_system() {
        assert_eq!(pick(None, Language::Auto, Lang::Es), Lang::Es);
        assert_eq!(pick(None, Language::Auto, Lang::En), Lang::En);
        assert_eq!(pick(None, Language::English, Lang::Es), Lang::En);
        assert_eq!(pick(None, Language::Spanish, Lang::En), Lang::Es);
        assert_eq!(pick(Some(Lang::Es), Language::English, Lang::En), Lang::Es);
        assert_eq!(pick(Some(Lang::En), Language::Spanish, Lang::Es), Lang::En);
    }

    /// Every key given to `t!` in `src/`, read from the source files.
    fn keys_used() -> BTreeSet<String> {
        let mut keys = BTreeSet::new();
        let mut folders = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
        while let Some(folder) = folders.pop() {
            for entry in std::fs::read_dir(&folder).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    folders.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    for (start, _) in text.match_indices("t!(") {
                        // `format!(` also ends in `t!(`: only a bare `t!` counts.
                        let before = text[..start].chars().next_back();
                        if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                            continue;
                        }
                        // The key may be on the next line once formatted.
                        let rest = text[start + 3..].trim_start();
                        if let Some(key) = rest.strip_prefix('"').and_then(|r| r.split('"').next())
                        {
                            // Only real keys: this scanner's own `"t!("` is not one.
                            if !key.is_empty()
                                && key
                                    .chars()
                                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
                            {
                                keys.insert(key.to_string());
                            }
                        }
                    }
                }
            }
        }
        keys
    }

    /// Top-level keys of `locales/app.yml` (lines without indentation, before the colon).
    fn keys_defined() -> BTreeSet<String> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("locales/app.yml");
        std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .filter(|line| !line.starts_with([' ', '#', '_']) && line.ends_with(':'))
            .map(|line| line.trim_end_matches(':').to_string())
            .collect()
    }

    #[test]
    fn every_key_in_use_has_both_languages() {
        let used = keys_used();
        assert!(used.len() > 50, "too few keys found: {}", used.len());
        for key in &used {
            for lang in [Lang::Es, Lang::En] {
                assert!(
                    crate::_rust_i18n_try_translate(lang.code(), key).is_some(),
                    "missing {} text for `{key}`",
                    lang.code()
                );
            }
        }
    }

    #[test]
    fn every_defined_key_is_used() {
        let unused: Vec<_> = keys_defined().difference(&keys_used()).cloned().collect();
        assert!(unused.is_empty(), "unused keys: {unused:?}");
    }
}
