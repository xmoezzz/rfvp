//! Restore "SJIS tunnel" substitute characters.
//!
//! Simplified Chinese has almost no JIS X 0208 code points: most everyday
//! simplified characters fail to encode as Shift-JIS. Chinese releases of FVP
//! games therefore write each of those characters as a *stand-in* kanji that
//! does have a code point, and the original PC engine maps it back after
//! decoding. Without that second step the text decodes into valid but wrong
//! characters — every glyph is a real character, yet the sentence is nonsense.
//!
//! The table ships with the game, in one of two shapes:
//!
//! * a run of consecutive UTF-16LE `(key, value)` pairs inside a `.dll`/`.exe`,
//!   or
//! * a `jis_map.bin` holding the same pair layout.
//!
//! Discovery is by shape, not by hard-coded offset: the longest run of pairs
//! whose halves both sit at or above U+2000 is the candidate. A game's own DLL
//! is preferred over an unrelated one, and the run has to be long enough that a
//! chance match is implausible.

use alloc::collections::BTreeMap;
use alloc::string::String;

/// Shortest run treated as a substitute table. Real tables carry around a
/// thousand pairs; an unrelated DLL can hold a handful of accidental ones, so
/// the bar is high.
const MIN_PAIRS: usize = 64;

/// A run this long is accepted even from a file not named after the script.
const MIN_PAIRS_UNRELATED: usize = 256;

/// The table, as a character-to-character map.
#[derive(Clone, Debug, Default)]
pub struct TextRestore {
    map: BTreeMap<char, char>,
}

impl TextRestore {
    /// Build from `(key, value)` code units, dropping anything that is not a
    /// plausible character pair.
    pub fn from_pairs(pairs: &[(u16, u16)]) -> Self {
        let mut map = BTreeMap::new();
        for &(key, value) in pairs {
            // Both halves have to be real characters, and a pair that maps a
            // character to itself carries no information.
            if key < 0x2000 || value < 0x2000 || key == value {
                continue;
            }
            let (Some(from), Some(to)) = (char::from_u32(key as u32), char::from_u32(value as u32))
            else {
                continue;
            };
            map.insert(from, to);
        }
        Self { map }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Replace every stand-in character; everything else is left alone.
    pub fn apply(&self, text: &str) -> String {
        if self.map.is_empty() || !text.chars().any(|c| self.map.contains_key(&c)) {
            return String::from(text);
        }
        text.chars()
            .map(|c| self.map.get(&c).copied().unwrap_or(c))
            .collect()
    }
}

/// The longest run of consecutive UTF-16LE `(key, value)` pairs in `bytes`.
///
/// Pairs are `u32`-sized and the tables sit `u32`-aligned in `.rdata`, so the
/// scan only starts at four-byte boundaries. Scanning every two bytes finds a
/// *shifted* run that is one code unit off — it interleaves the keys and values,
/// is one pair longer than the real table, and would win the length contest
/// while mapping every character to the wrong one.
///
/// Both halves must be at or above U+2000: below that is ASCII, control codes
/// and Latin, never a substitute table.
pub fn longest_pair_run(bytes: &[u8]) -> alloc::vec::Vec<(u16, u16)> {
    let mut best: alloc::vec::Vec<(u16, u16)> = alloc::vec::Vec::new();
    let mut run: alloc::vec::Vec<(u16, u16)> = alloc::vec::Vec::new();
    let mut i = 0usize;
    while i + 4 <= bytes.len() {
        let key = u16::from_le_bytes([bytes[i], bytes[i + 1]]);
        let value = u16::from_le_bytes([bytes[i + 2], bytes[i + 3]]);
        if key >= 0x2000 && value >= 0x2000 {
            run.push((key, value));
            i += 4;
        } else {
            if run.len() > best.len() {
                best = core::mem::take(&mut run);
            }
            run.clear();
            // Stay on four-byte boundaries after a break.
            i = (i + 4) & !3;
        }
    }
    if run.len() > best.len() {
        best = run;
    }
    best
}

/// Table discovered in a game directory.
pub struct Discovered {
    pub table: TextRestore,
    /// Where it came from, for the log.
    pub source: String,
}

#[cfg(not(feature = "no_std"))]
pub use filesystem::discover;

#[cfg(not(feature = "no_std"))]
mod filesystem {
    use super::{Discovered, MIN_PAIRS, MIN_PAIRS_UNRELATED, TextRestore, longest_pair_run};
    use alloc::format;
    use alloc::string::{String, ToString};
    use std::path::{Path, PathBuf};

    /// Look for a substitute table next to the script.
    ///
    /// Order: `jis_map.bin`, then a DLL/EXE named after the script, then any
    /// DLL/EXE whose run is long enough to be unambiguous.
    pub fn discover(dir: &Path, script_stem: Option<&str>) -> Option<Discovered> {
        let jis_map = dir.join("jis_map.bin");
        if let Some(found) = try_file(&jis_map, MIN_PAIRS) {
            return Some(found);
        }

        let mut candidates: alloc::vec::Vec<PathBuf> = alloc::vec::Vec::new();
        if let Some(stem) = script_stem {
            for ext in ["dll", "exe"] {
                candidates.push(dir.join(format!("{stem}.{ext}")));
            }
        }

        // A file named after the script is the likely owner of the table.
        for path in &candidates {
            if let Some(found) = try_file(path, MIN_PAIRS) {
                return Some(found);
            }
        }

        // Otherwise only accept an unmistakable run, so an unrelated DLL does
        // not silently rewrite the game's text.
        let entries = std::fs::read_dir(dir).ok()?;
        let mut best: Option<Discovered> = None;
        for entry in entries.flatten() {
            let path = entry.path();
            let is_binary = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("dll") || e.eq_ignore_ascii_case("exe"));
            if !is_binary {
                continue;
            }
            if let Some(found) = try_file(&path, MIN_PAIRS_UNRELATED) {
                if best.as_ref().is_none_or(|b| found.table.len() > b.table.len()) {
                    best = Some(found);
                }
            }
        }
        best
    }

    fn try_file(path: &Path, min_pairs: usize) -> Option<Discovered> {
        let bytes = std::fs::read(path).ok()?;
        let run = longest_pair_run(&bytes);
        if run.len() < min_pairs {
            return None;
        }
        let table = TextRestore::from_pairs(&run);
        // A run of numeric or symbol pairs is not a character table.
        if table.len() < min_pairs {
            return None;
        }
        Some(Discovered {
            table,
            source: path.display().to_string(),
        })
    }
}

#[cfg(not(feature = "no_std"))]
mod installed {
    use super::TextRestore;
    use alloc::string::String;
    use std::sync::OnceLock;

    static TABLE: OnceLock<TextRestore> = OnceLock::new();

    /// Install the table found for this game. Later calls are ignored.
    pub fn install(table: TextRestore) {
        let _ = TABLE.set(table);
    }

    /// Apply the installed table, if any.
    pub fn restore(text: &str) -> String {
        match TABLE.get() {
            Some(table) => table.apply(text),
            None => String::from(text),
        }
    }
}

#[cfg(not(feature = "no_std"))]
pub use installed::{install, restore};

#[cfg(feature = "no_std")]
pub fn restore(text: &str) -> alloc::string::String {
    alloc::string::String::from(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs_bytes(pairs: &[(u16, u16)]) -> alloc::vec::Vec<u8> {
        let mut out = alloc::vec::Vec::new();
        for &(k, v) in pairs {
            out.extend_from_slice(&k.to_le_bytes());
            out.extend_from_slice(&v.to_le_bytes());
        }
        out
    }

    #[test]
    fn finds_a_run_of_pairs_among_other_bytes() {
        let pairs: alloc::vec::Vec<(u16, u16)> =
            (0..100u16).map(|i| (0x5f00 + i, 0x8f00 + i)).collect();
        let mut bytes = alloc::vec![0u8; 8]; // table does not start at 0
        bytes.extend_from_slice(&pairs_bytes(&pairs));
        bytes.extend_from_slice(&[0u8; 16]);

        let run = longest_pair_run(&bytes);
        assert_eq!(run.len(), 100);
        assert_eq!(run[0], pairs[0]);
    }

    #[test]
    fn a_short_run_is_not_a_table() {
        let pairs: alloc::vec::Vec<(u16, u16)> = (0..10u16).map(|i| (0x5f00 + i, 0x8f00 + i)).collect();
        let table = TextRestore::from_pairs(&pairs);
        assert!(table.len() < MIN_PAIRS);
    }

    #[test]
    fn restores_stand_in_characters_and_leaves_the_rest_alone() {
        // 弗→么, 戍→你, 諍→说, 徼→这
        // 丙→么, 戊→你, 己→这, 庚→说 (arbitrary pairs; only the mapping matters).
        let table = TextRestore::from_pairs(&[
            (0x4e19, 0x4e48),
            (0x620a, 0x4f60),
            (0x5df1, 0x8fd9),
            (0x5e9a, 0x8bf4),
        ]);
        assert_eq!(
            table.apply("甲乙丙丁。戊好，己樣庚。"),
            "甲乙么丁。你好，这樣说。"
        );
        assert_eq!(table.apply("AudioLoad2"), "AudioLoad2");
    }

    /// Discovery against a real game directory. Ignored by default:
    /// `RFVP_TEXT_MAP_DIR=/path/to/game cargo test -p rfvp --lib -- --ignored`.
    #[cfg(not(feature = "no_std"))]
    #[test]
    #[ignore = "needs RFVP_TEXT_MAP_DIR pointing at a game directory"]
    fn discovers_the_table_that_ships_with_the_game() {
        let dir = std::path::PathBuf::from(
            std::env::var("RFVP_TEXT_MAP_DIR").expect("set RFVP_TEXT_MAP_DIR"),
        );
        let found = discover(&dir, None).expect("no substitute table found");
        assert!(
            found.table.len() > 1000,
            "expected a large table, got {} pairs from {}",
            found.table.len(),
            found.source
        );
        // ASCII never participates, so a table must leave it alone.
        assert_eq!(found.table.apply("AudioLoad2"), "AudioLoad2");
    }

    #[test]
    fn pairs_below_u2000_are_dropped() {
        let table = TextRestore::from_pairs(&[(0x0041, 0x8fd9), (0x8fd9, 0x0041)]);
        assert!(table.is_empty());
    }
}
