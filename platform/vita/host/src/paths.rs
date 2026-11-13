//! Path rules shared by the file browser and the launcher's settings file.
//!
//! Device paths on the Vita look like `ux0:`, `ux0:data/game`, `ux0:/data/game`, `app0:x`. The
//! browser works with these absolute strings and hands the platform layer one of them as the
//! game root; everything the core then opens is relative to it. These helpers only do the text
//! part: no file is touched here, so they can be unit-tested on a host without a card.
//!
//! Kept in the library rather than in the launcher module so the rules can be tested at all: the
//! launcher is part of a `no_std` binary that has no test harness.

use alloc::format;
use alloc::string::{String, ToString};

/// A directory path in its canonical form: no trailing separator, and `ux0:/data` written as
/// `ux0:data` (the two name the same directory; the short form is what the platform's default
/// root already uses). `ux0:/` becomes `ux0:`, the device root.
pub fn normalize_dir(path: &str) -> String {
    let mut out = path.trim();
    while out.len() > 1 && (out.ends_with('/') || out.ends_with('\\')) {
        out = &out[..out.len() - 1];
    }
    if out.is_empty() {
        return String::from("/");
    }
    // One separator after the device designator, not two spellings of the same directory.
    if let Some((device, rest)) = out.split_once(':') {
        if !device.is_empty() && rest.starts_with('/') {
            let mut canonical = String::with_capacity(out.len());
            canonical.push_str(device);
            canonical.push(':');
            canonical.push_str(rest.trim_start_matches('/'));
            return canonical;
        }
    }
    out.to_string()
}

/// The parent of a directory, or `None` at a device root.
///
/// The device designator is not a separator: `ux0:data` is one level below `ux0:`, and the only
/// path without a parent is `ux0:` itself. Splitting on `/` alone would report `ux0:data` as
/// having no parent, which strands the browser inside the first directory it enters.
pub fn parent_dir(path: &str) -> Option<String> {
    let normalized = normalize_dir(path);
    let (device, rest) = match normalized.split_once(':') {
        Some((device, rest)) => (Some(device), rest),
        None => (None, normalized.as_str()),
    };
    match device {
        Some(device) => {
            if rest.is_empty() {
                return None; // the device root
            }
            return match rest.rsplit_once('/') {
                Some((head, _)) if !head.is_empty() => Some(format!("{device}:{head}")),
                _ => Some(format!("{device}:")),
            };
        }
        None => {
            let (head, tail) = normalized.rsplit_once('/')?;
            if tail.is_empty() {
                return None;
            }
            Some(normalize_dir(if head.is_empty() { "/" } else { head }))
        }
    }
}

/// Join a directory and an entry name.
pub fn join_path(dir: &str, name: &str) -> String {
    let dir = normalize_dir(dir);
    if dir.ends_with('/') || dir.ends_with(':') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    /// The two spellings of a device path name the same directory; the canonical one is used for
    /// comparisons and for the remembered-directory key.
    #[test]
    fn device_paths_have_one_canonical_form() {
        assert_eq!(normalize_dir("ux0:"), "ux0:");
        assert_eq!(normalize_dir("ux0:/"), "ux0:");
        assert_eq!(normalize_dir("ux0:data"), "ux0:data");
        assert_eq!(normalize_dir("ux0:/data"), "ux0:data");
        assert_eq!(normalize_dir("ux0:/data/game"), "ux0:data/game");
        assert_eq!(normalize_dir("ux0:data/rfvp/game/"), "ux0:data/rfvp/game");
    }

    #[test]
    fn device_root_has_no_parent() {
        assert_eq!(parent_dir("ux0:"), None);
        assert_eq!(parent_dir("ux0:/"), None);
    }

    #[test]
    fn parent_walks_up_to_the_device_root_and_stops() {
        assert_eq!(parent_dir("ux0:data/rfvp/game").as_deref(), Some("ux0:data/rfvp"));
        assert_eq!(parent_dir("ux0:data/rfvp").as_deref(), Some("ux0:data"));
        // The first directory under the device still has a parent: the device root itself.
        assert_eq!(parent_dir("ux0:data").as_deref(), Some("ux0:"));
        assert_eq!(parent_dir("ux0:data/rfvp/game/").as_deref(), Some("ux0:data/rfvp"));
        // A slash after the designator must not swallow the "data" level.
        assert_eq!(parent_dir("ux0:/data/game").as_deref(), Some("ux0:data"));
        assert_eq!(parent_dir("ux0:/data").as_deref(), Some("ux0:"));
    }

    #[test]
    fn join_keeps_one_separator() {
        assert_eq!(join_path("ux0:data/rfvp/game", "Sakura.hcb"), "ux0:data/rfvp/game/Sakura.hcb");
        assert_eq!(join_path("ux0:data/rfvp/game/", "Sakura.hcb"), "ux0:data/rfvp/game/Sakura.hcb");
        assert_eq!(join_path("ux0:", "data"), "ux0:data");
        // "ux0:/" is the device root, whose canonical form has no extra separator.
        assert_eq!(join_path("ux0:/", "data"), "ux0:data");
        assert_eq!(join_path("ux0:/data", "game"), "ux0:data/game");
    }

    #[test]
    fn chinese_and_space_names_round_trip() {
        let dir = join_path("ux0:data/视觉 小说", "星空 的记忆");
        assert_eq!(dir, "ux0:data/视觉 小说/星空 的记忆");
        // Walking back up must give the same directory text, not a truncated display name.
        assert_eq!(parent_dir(&dir).as_deref(), Some("ux0:data/视觉 小说"));
    }

    #[test]
    fn normalize_leaves_relative_paths_alone() {
        assert_eq!(normalize_dir("game"), "game".to_string());
        assert_eq!(normalize_dir(""), "/".to_string());
        assert_eq!(parent_dir("game"), None);
    }

    // The absolute-path rule itself lives in C (`rfvp_path_is_absolute` in rfvp_psv_c.c) and is
    // checked there: a Rust copy would only test itself.
}
