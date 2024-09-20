//! Implement the LZ77 variant used in the game.
//!
//! LZ77 is a lossless compression algorithm that compressed data by replacing repeated sequences of bytes with references to the previous occurrences of the same sequence.
//!
//! This variant of LZ77 precedes a block of data with an 8-bit bitmap specifying whether decompressor should read a literal byte or a reference.
//!
//! The references are encoded as 16-bit big-endian (sic!) integers. It encodes offset and length of the reference,
//!     but the amount of bits spent on each part is dependent on the format (const generics are used to specify this).
//!
//! Offset is specified as amount to seek back from the current position.
//!
//! The minimum offset is 1, so the actual offset is offset + 1.
//! The minimum length is 3, so the actual length is length + 3.
//!
//! Here's an example of a data stream encoded with 12 bits offset (and, hence, 4 bits length):
//!
//! ```text
//! // first byte is the bitmap. bitmap is read from LSB to MSB
//! 0b11000000 // 6 literals, 2 references
//! // 6 literal bytes encoding the string "HELLO "
//! 0x48 0x45 0x4c 0x4c 0x4f 0x20
//! // first reference
//! 0x3005 // offset <- 1+5=6, length <- 3+3=6
//! // ^ this seeks back 6 bytes and copies 6 bytes from there
//! // repeating the string "HELLO " previously decoded
//! // second reference
//! 0x800b // offset <- 1+11=12, length <- 3+8=11
//! // ^ this seeks back 12 bytes and copies 11 bytes from there
//! // repeating the string "HELLO HELLO" previously decoded
//! // note that you CAN reference the data that was itself decoded by a reference
//! ```
//!
//! ```
//! let compressed = [0b11000000, 0x48, 0x45, 0x4c, 0x4c, 0x4f, 0x20, 0x30, 0x05, 0x80, 0x0b];
//! let mut decompressed = Vec::new();
//! rfvp_core::format::lz77::decompress::<12>(&compressed, &mut decompressed);
//! assert_eq!(decompressed, b"HELLO HELLO HELLO HELLO");
//! ```
//!
//! Encoding is (to be) implemented using a sliding window and a greedy algorithm.
//! Theoretically the efficiency can be improved by using a bit of backtracking,
//!     but it seems this improves compression ratio only by several percent (not worth the time).

use std::io;

use bytes::Buf;

pub fn decompress<const OFFSET_BITS: u32>(input: &[u8], output: &mut Vec<u8>) {
    let mut input = io::Cursor::new(input);

    while input.has_remaining() {
        let map = input.get_u8();
        for i in 0..8 {
            if !input.has_remaining() {
                break;
            }

            if ((map >> i) & 1) == 0 {
                /* literal value */
                output.push(input.get_u8());
            } else {
                /* back seek */
                let backseek_spec = input.get_u16(); // big endian Oo

                /*  MSB  XXXXXXXX          YYYYYYYY    LSB
                    val  len               backOffset
                    size (16-OFFSET_BITS)  OFFSET_BITS
                */

                let back_offset_mask = (1 << OFFSET_BITS) - 1; // magic to get the last OFFSET_BITS bits

                let len = (backseek_spec >> OFFSET_BITS) + 3;
                let back_offset = (backseek_spec & back_offset_mask) + 1;

                for _ in 0..len {
                    let last = output.len() - back_offset as usize;
                    // TODO: make this fallible?
                    // TODO: this might be optimized by stopping the bounds checking after we have enough data to guarantee that it's in bounds
                    output.push(output[last]);
                }
            }
        }
    }
}
