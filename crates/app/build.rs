//! Turns the language files into Rust, so a missing key stops the build.
//!
//! `assets/lang/en.json` holds every word the window shows the DM. This
//! script reads it and writes one function for each key. A screen that
//! asks for a key the file does not hold asks for a function that does
//! not exist, and the build says so.
//!
//! Every other file in that folder is a translation. The script bakes it
//! in beside the English one, so the program carries its languages and a
//! new one is a file and a pull request. A key the translation does not
//! hold stands as `None`, and the program falls back to English.

// Rust guideline compliant 2026-02-21

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The folder that holds the language files.
const LANG: &str = "assets/lang";

/// The language that ships with the program and answers for every key.
const ENGLISH: &str = "en";

/// The key that names a language in its own words, for the Settings list.
const NAME_KEY: &str = "language.name";

fn main() {
    println!("cargo:rerun-if-changed={LANG}");
    let folder = Path::new(LANG);
    let english = read(&folder.join(format!("{ENGLISH}.json")));
    let keys: Vec<&String> = english.keys().collect();

    let mut out = String::new();
    out.push_str("// Written by build.rs from assets/lang. Do not edit.\n\n");
    write_english(&mut out, &english);
    write_calls(&mut out, &english);
    write_languages(&mut out, folder, &keys);

    let path = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR")).join("text.rs");
    std::fs::write(&path, out).expect("the build folder takes a file");
}

/// Reads one language file as a map of key to string.
///
/// # Panics
///
/// Panics when the file is missing or does not hold a flat JSON object of
/// strings. Either one is a mistake in the repository, and the build says
/// which file and why.
fn read(path: &Path) -> BTreeMap<String, String> {
    let json = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{}: cannot read: {error}", path.display()));
    serde_json::from_str(&json)
        .unwrap_or_else(|error| panic!("{}: not a flat map of strings: {error}", path.display()))
}

/// The English strings and their keys, in one order for every table.
fn write_english(out: &mut String, english: &BTreeMap<String, String>) {
    let _ = writeln!(out, "/// How many strings the catalog holds.");
    let _ = writeln!(out, "pub const COUNT: usize = {};\n", english.len());
    let _ = writeln!(out, "/// Every key, for the log of what fell back.");
    let _ = writeln!(out, "pub static KEYS: [&str; COUNT] = [");
    for key in english.keys() {
        let _ = writeln!(out, "    {},", quote(key));
    }
    let _ = writeln!(out, "];\n");
    let _ = writeln!(out, "/// The English strings, which answer for every key.");
    let _ = writeln!(out, "pub static ENGLISH: [&str; COUNT] = [");
    for text in english.values() {
        let _ = writeln!(out, "    {},", quote(text));
    }
    let _ = writeln!(out, "];\n");
}

/// One function for each key, which is what makes a typo a build error.
fn write_calls(out: &mut String, english: &BTreeMap<String, String>) {
    for (place, (key, text)) in english.iter().enumerate() {
        // The name of a language reaches the Settings list through the
        // list itself, so no screen calls for it.
        if key == NAME_KEY {
            continue;
        }
        let name = call_name(key);
        let places = placeholders(text);
        let _ = writeln!(out, "/// `{key}`, English: {text:?}");
        if places.is_empty() {
            let _ = writeln!(
                out,
                "pub fn {name}() -> &'static str {{\n    at({place})\n}}\n"
            );
            continue;
        }
        let args: Vec<String> = places
            .iter()
            .map(|place| format!("{place}: impl std::fmt::Display"))
            .collect();
        let pairs: Vec<String> = places
            .iter()
            .map(|place| format!("({}, &{place} as &dyn std::fmt::Display)", quote(place)))
            .collect();
        let _ = writeln!(
            out,
            "pub fn {name}({}) -> String {{\n    fill(at({place}), &[{}])\n}}\n",
            args.join(", "),
            pairs.join(", ")
        );
    }
}

/// The table of each translation, and the list the Settings select shows.
fn write_languages(out: &mut String, folder: &Path, keys: &[&String]) {
    let _ = writeln!(out, "/// One language the program carries.");
    let _ = writeln!(out, "#[derive(Debug, Clone, Copy)]");
    let _ = writeln!(out, "pub struct Language {{");
    let _ = writeln!(out, "    /// The name of its file, such as `ru`.");
    let _ = writeln!(out, "    pub code: &'static str,");
    let _ = writeln!(out, "    /// What that language calls itself.");
    let _ = writeln!(out, "    pub name: &'static str,");
    let _ = writeln!(
        out,
        "    /// Its strings, in key order. `None` falls back to English."
    );
    let _ = writeln!(
        out,
        "    pub strings: &'static [Option<&'static str>; COUNT],"
    );
    let _ = writeln!(out, "}}\n");

    let mut tables = String::new();
    let mut list = String::new();
    for path in files(folder) {
        let code = path
            .file_stem()
            .expect("a file that was found has a name")
            .to_string_lossy()
            .into_owned();
        let strings = read(&path);
        for key in strings.keys() {
            assert!(
                keys.contains(&key),
                "{}: the key {key} is in no English file",
                path.display()
            );
        }
        let table = format!("{}_STRINGS", code.to_uppercase().replace('-', "_"));
        let _ = writeln!(tables, "static {table}: [Option<&str>; COUNT] = [");
        for key in keys {
            match strings.get(*key) {
                Some(text) => {
                    let _ = writeln!(tables, "    Some({}),", quote(text));
                }
                None => {
                    let _ = writeln!(tables, "    None,");
                }
            }
        }
        let _ = writeln!(tables, "];\n");
        let name = strings
            .get(NAME_KEY)
            .cloned()
            .unwrap_or_else(|| code.clone());
        let _ = writeln!(
            list,
            "    Language {{ code: {}, name: {}, strings: &{table} }},",
            quote(&code),
            quote(&name)
        );
    }
    out.push_str(&tables);
    let _ = writeln!(out, "/// Every language, English first.");
    let _ = writeln!(out, "pub static LANGUAGES: &[Language] = &[");
    out.push_str(&list);
    let _ = writeln!(out, "];");
}

/// The language files, English first and the rest by their names.
fn files(folder: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(folder)
        .unwrap_or_else(|error| panic!("{}: cannot read: {error}", folder.display()))
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|kind| kind == "json"))
        .collect();
    found.sort();
    found.sort_by_key(|path| path.file_stem().is_none_or(|stem| stem != ENGLISH));
    found
}

/// The name of the function that gives one key its string.
fn call_name(key: &str) -> String {
    key.replace(['.', '-'], "_")
}

/// The names in `{braces}`, in the order they first appear.
fn placeholders(text: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        let name = &rest[open + 1..open + close];
        if !name.is_empty()
            && name.chars().all(|c| c.is_ascii_lowercase() || c == '_')
            && !found.iter().any(|had| had == name)
        {
            found.push(name.to_owned());
        }
        rest = &rest[open + close + 1..];
    }
    found
}

/// A Rust string literal that holds `text`.
fn quote(text: &str) -> String {
    format!("{text:?}")
}
