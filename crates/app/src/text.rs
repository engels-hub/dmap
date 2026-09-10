//! Every word the window shows the DM, in the language the DM picked.
//!
//! `assets/lang/en.json` holds the English text. `build.rs` reads it and
//! writes one function for each key into this module, so a screen says
//! `text::panel_objects_title()` and a key that is not in the file does
//! not build. A string with a place for a number or a name takes it as an
//! argument, and the place moves with the language.
//!
//! Every other file in that folder is a translation, and the program
//! carries them all. A key a translation does not hold falls back to
//! English, so the window never shows a bare key. [`use_language`] says
//! in the log which language it took and which keys fell back.
//!
//! The catalog is one value for the whole program. Every screen reads it
//! and the DM changes it in one place, so passing it through each panel
//! would say nothing a global does not.

// Rust guideline compliant 2026-02-21

use std::sync::RwLock;

include!(concat!(env!("OUT_DIR"), "/text.rs"));

/// The strings of the language the DM picked, or none for English.
///
/// A string of a language that is baked in lives as long as the program,
/// so a screen holds `&'static str` and no frame copies a word.
static ACTIVE: RwLock<Option<&'static Language>> = RwLock::new(None);

/// The language of a first run, when the system asks for none we hold.
pub const DEFAULT: &str = "en";

/// The string at one place in the catalog.
///
/// The generated calls reach the catalog through this. A language that
/// lacks the key falls back to English, and so does a poisoned lock: the
/// DM keeps working in a language they can read either way.
fn at(place: usize) -> &'static str {
    let active = ACTIVE.read().ok().and_then(|active| *active);
    active
        .and_then(|language| language.strings[place])
        .unwrap_or(ENGLISH[place])
}

/// Puts the values in a string in the places the language gave them.
///
/// A place is a name in braces, such as `{count}`. A name the caller does
/// not know is left as it stands, so a broken translation shows its own
/// mistake instead of losing the rest of the line.
fn fill(template: &str, args: &[(&str, &dyn std::fmt::Display)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        // A brace with no partner is text. Nothing is written until the
        // pair is whole, so the tail below carries it as it stands.
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        out.push_str(&rest[..open]);
        let name = &rest[open + 1..open + close];
        match args.iter().find(|(known, _)| *known == name) {
            Some((_, value)) => {
                let _ = std::fmt::Write::write_fmt(&mut out, format_args!("{value}"));
            }
            None => out.push_str(&rest[open..=open + close]),
        }
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out
}

/// The language of this code, when the program carries one.
pub fn language(code: &str) -> Option<&'static Language> {
    LANGUAGES.iter().find(|language| language.code == code)
}

/// Takes the language of this code, and says so in the log.
///
/// The log names the language and every key that fell back to English, so
/// a translator sees at once what their file still lacks. A code the
/// program does not carry leaves the window in the language it had.
pub fn use_language(code: &str) {
    let Some(language) = language(code) else {
        eprintln!("language {code}: not one this program carries; keeping the last one");
        return;
    };
    let missing: Vec<&str> = language
        .strings
        .iter()
        .enumerate()
        .filter(|(_, text)| text.is_none())
        .map(|(place, _)| KEYS[place])
        .collect();
    if missing.is_empty() {
        eprintln!("language {code}: every key");
    } else {
        eprintln!(
            "language {code}: {} of {COUNT} keys fall back to English: {}",
            missing.len(),
            missing.join(", ")
        );
    }
    if let Ok(mut active) = ACTIVE.write() {
        *active = Some(language);
    }
}

/// The language the system asks for, when the program carries one.
///
/// A locale names a language and often a country, such as `ru_RU.UTF-8`
/// or `pt-BR`. The language alone decides, so a Brazilian locale takes a
/// Portuguese file. Returns `None` when nothing matches, and the caller
/// falls back to [`DEFAULT`].
pub fn system_language(locale: &str) -> Option<&'static str> {
    let head = locale.split(['.', '_', '-']).next()?.to_ascii_lowercase();
    if head.is_empty() {
        return None;
    }
    LANGUAGES
        .iter()
        .find(|language| language.code == head)
        .map(|language| language.code)
}

#[cfg(test)]
mod tests {
    use super::{ENGLISH, KEYS, LANGUAGES, fill, language, system_language};

    #[test]
    fn english_answers_for_every_key() {
        assert_eq!(KEYS.len(), ENGLISH.len());
        assert!(!KEYS.is_empty());
        assert!(ENGLISH.iter().all(|text| !text.is_empty()));
    }

    #[test]
    fn english_is_the_first_language_and_names_itself() {
        let first = LANGUAGES.first().expect("the program carries English");
        assert_eq!(first.code, "en");
        assert_eq!(first.name, "English");
    }

    #[test]
    fn a_language_the_program_lacks_is_not_found() {
        assert!(language("en").is_some());
        assert!(language("qq").is_none());
    }

    #[test]
    fn a_place_takes_its_value_wherever_the_language_puts_it() {
        assert_eq!(fill("{a} and {b}", &[("a", &1), ("b", &2)]), "1 and 2");
        assert_eq!(fill("{b} and {a}", &[("a", &1), ("b", &2)]), "2 and 1");
        assert_eq!(fill("{a} twice {a}", &[("a", &"x")]), "x twice x");
    }

    #[test]
    fn a_place_no_one_fills_stays_as_it_stands() {
        assert_eq!(fill("{who} knows", &[("what", &1)]), "{who} knows");
        // A brace with no partner is text, not a place.
        assert_eq!(fill("100 % {", &[]), "100 % {");
    }

    #[test]
    fn a_locale_gives_up_its_country_and_its_encoding() {
        assert_eq!(system_language("en_US.UTF-8"), Some("en"));
        assert_eq!(system_language("en"), Some("en"));
        assert_eq!(system_language("EN-gb"), Some("en"));
        assert_eq!(system_language("zz_ZZ"), None);
        assert_eq!(system_language(""), None);
    }
}
