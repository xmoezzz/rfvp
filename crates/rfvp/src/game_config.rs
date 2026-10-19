//! `config.toml` in the game directory: what a console cannot ask the player.
//!
//! A console has no launcher to pick the script or its encoding, so a translated game (a GBK
//! script beside the Japanese one, say) says so here:
//!
//! ```toml
//! [game]
//! hcb = "MemoriaGBK.hcb"   # the script, relative to the game directory
//! nls = "gbk"              # its encoding: sjis (the default), gbk or utf8
//! ```
//!
//! Every key is optional; without the file the port boots as before (the first `.hcb` found,
//! ShiftJIS or the port's own `nls.txt`). Only the small subset of TOML these keys need is read:
//! `[section]` headers, `key = value` lines (a value quoted or bare) and `#` comments. Unknown
//! sections and keys are skipped, so a newer file still boots an older build.

use alloc::format;
use alloc::string::{String, ToString};

use crate::script::parser::Nls;

/// The file's name, in the game directory.
pub const GAME_CONFIG_FILE: &str = "config.toml";

/// What `config.toml` sets; `None` keeps the port's default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GameConfig {
    /// The script to run, relative to the game directory.
    pub hcb: Option<String>,
    /// The script's text encoding.
    pub nls: Option<Nls>,
}

impl GameConfig {
    /// Parses the file; an error names the line, for `last_error.txt`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut config = GameConfig::default();
        let mut section = String::new();
        for (index, raw) in text.lines().enumerate() {
            let line_no = index + 1;
            let line = strip_comment(raw).trim();
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix('[') {
                let Some(name) = rest.strip_suffix(']') else {
                    return Err(format!("line {line_no}: unclosed section header"));
                };
                section = name.trim().to_string();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(format!("line {line_no}: expected `key = value`"));
            };
            let key = key.trim();
            let value = unquote(value.trim())
                .ok_or_else(|| format!("line {line_no}: unterminated string"))?;
            if section != "game" {
                continue;
            }
            match key {
                "hcb" => {
                    if value.is_empty() {
                        return Err(format!("line {line_no}: `hcb` is empty"));
                    }
                    config.hcb = Some(value.to_string());
                }
                "nls" => {
                    config.nls = Some(
                        value
                            .parse()
                            .map_err(|err| format!("line {line_no}: {err}"))?,
                    );
                }
                _ => {}
            }
        }
        Ok(config)
    }
}

/// The line up to a `#` that is not inside a string.
fn strip_comment(line: &str) -> &str {
    let mut quote = None;
    for (i, c) in line.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), _) if c == q => quote = None,
            (None, '#') => return &line[..i],
            _ => {}
        }
    }
    line
}

/// A quoted value without its quotes, or a bare one as it is.
fn unquote(value: &str) -> Option<&str> {
    match value.chars().next() {
        Some(q @ ('"' | '\'')) => value[1..].strip_suffix(q),
        _ => Some(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_game_section() {
        let config = GameConfig::parse(
            "# a translation\n[game]\nhcb = \"MemoriaGBK.hcb\" # the script\nnls = 'GBK'\n",
        )
        .unwrap();
        assert_eq!(config.hcb.as_deref(), Some("MemoriaGBK.hcb"));
        assert_eq!(config.nls, Some(Nls::GBK));
    }

    #[test]
    fn bare_values_and_unknown_keys() {
        let config =
            GameConfig::parse("[game]\nhcb = a#b.hcb\nfuture = 1\n[other]\nnls = utf8\n").unwrap();
        assert_eq!(config.hcb.as_deref(), Some("a"));
        assert_eq!(config.nls, None);
        assert_eq!(GameConfig::parse("").unwrap(), GameConfig::default());
    }

    #[test]
    fn errors_name_the_line() {
        assert!(GameConfig::parse("[game]\nnls = big5\n").unwrap_err().starts_with("line 2:"));
        assert!(GameConfig::parse("[game\n").unwrap_err().starts_with("line 1:"));
        assert!(GameConfig::parse("[game]\nhcb = \"x\n").unwrap_err().starts_with("line 2:"));
    }
}
