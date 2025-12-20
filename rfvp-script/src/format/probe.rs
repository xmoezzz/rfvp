//! Heuristics to locate bytecode when the container layout is not yet fully known.
//!
//! These helpers are intentionally conservative. They do NOT attempt to "fully parse" the file.

/// Scan for a likely bytecode offset by looking for plausible opcode patterns:
/// - entry often begins with `init_stack` (opcode 1)
/// - followed by 2 small immediates (argc: u8, local_cnt: i8)
///
/// Returns the first offset that matches the pattern.
pub fn probe_bytecode_offset(bytes: &[u8]) -> Option<u32> {
    if bytes.len() < 4 {
        return None;
    }
    for i in 0..bytes.len().saturating_sub(4) {
        if bytes[i] == 1 {
            // argc: bytes[i+1] (any), local_cnt: bytes[i+2] (signed, any)
            // Next opcode should be in 0..=39.
            let next = bytes[i + 3];
            if next <= 39 {
                return Some(i as u32);
            }
        }
    }
    None
}

/// A minimal "entry point" guess.
/// If bytecode offset is known, entry is usually 0.
pub fn probe_entry_point(_bytecode: &[u8]) -> u32 {
    0
}
