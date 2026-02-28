//! Common helpers shared by WMA variants.
//!

/// Return log2(number of output samples per frame).
///
/// This is `ff_wma_get_frame_len_bits()`.
pub fn ff_wma_get_frame_len_bits(sample_rate: i32, version: i32, decode_flags: u32) -> i32 {
    let mut frame_len_bits: i32;

    if sample_rate <= 16000 {
        frame_len_bits = 9;
    } else if sample_rate <= 22050 || (sample_rate <= 32000 && version == 1) {
        frame_len_bits = 10;
    } else if sample_rate <= 48000 || version < 3 {
        frame_len_bits = 11;
    } else if sample_rate <= 96000 {
        frame_len_bits = 12;
    } else {
        frame_len_bits = 13;
    }

    if version == 3 {
        let tmp = (decode_flags & 0x6) as i32;
        if tmp == 0x2 {
            frame_len_bits += 1;
        } else if tmp == 0x4 {
            frame_len_bits -= 1;
        } else if tmp == 0x6 {
            frame_len_bits -= 2;
        }
    }

    frame_len_bits
}
