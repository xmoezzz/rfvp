//! VLC (Huffman) table builder and decoder.
//!

use crate::error::{DecoderError, Result};
use crate::wma::bitstream::GetBitContext;

pub const VLC_INIT_USE_STATIC: i32 = 1;
pub const VLC_INIT_STATIC_OVERLONG: i32 = 2 | VLC_INIT_USE_STATIC;
pub const VLC_INIT_INPUT_LE: i32 = 4;
pub const VLC_INIT_OUTPUT_LE: i32 = 8;

pub type VlcBaseType = i16;

#[derive(Clone, Copy, Default)]
pub struct VlcElem {
    pub sym: VlcBaseType,
    pub len: VlcBaseType,
}

#[derive(Default)]
pub struct Vlc {
    pub bits: i32,
    pub table: Vec<VlcElem>,
    pub table_size: i32,
    pub table_allocated: i32,
}

#[derive(Clone, Copy)]
struct VlcCode {
    bits: u8,
    symbol: VlcBaseType,
    /// Codeword with the first bit-to-be-read in the MSB.
    code: u32,
}

fn bitswap_32(x: u32) -> u32 {
    x.reverse_bits()
}

fn alloc_table(vlc: &mut Vlc, size: i32, use_static: bool) -> Result<i32> {
    let index = vlc.table_size;
    vlc.table_size += size;
    if vlc.table_size > vlc.table_allocated {
        if use_static {
            return Err(DecoderError::InvalidData("static VLC table too small".into()));
        }
        vlc.table_allocated += 1 << vlc.bits;
        let new_len = vlc.table_allocated as usize;
        if new_len > vlc.table.len() {
            vlc.table.resize(new_len, VlcElem { sym: 0, len: 0 });
        }
    }
    Ok(index)
}

fn build_table(vlc: &mut Vlc, table_nb_bits: i32, nb_codes: usize, codes: &mut [VlcCode], flags: i32) -> Result<i32> {
    if table_nb_bits > 30 {
        return Err(DecoderError::InvalidData("table_nb_bits > 30".into()));
    }
    let table_size = 1 << table_nb_bits;
    let table_index = alloc_table(vlc, table_size, (flags & VLC_INIT_USE_STATIC) != 0)?;

    let base = table_index as usize;

    for i in 0..nb_codes {
        let mut n = codes[i].bits as i32;
        let mut code = codes[i].code;
        let symbol = codes[i].symbol;

        if n <= table_nb_bits {
            let mut j = (code >> (32 - table_nb_bits)) as i32;
            let nb = 1 << (table_nb_bits - n);
            let mut inc = 1;
            if (flags & VLC_INIT_OUTPUT_LE) != 0 {
                j = (bitswap_32(code) >> (32 - table_nb_bits)) as i32;
                inc = 1 << n;
            }
            for _k in 0..nb {
                let idx = base + j as usize;
                let bits = vlc.table[idx].len;
                let oldsym = vlc.table[idx].sym;
                if (bits != 0 || oldsym != 0) && (bits != n as i16 || oldsym != symbol) {
                    return Err(DecoderError::InvalidData("incorrect VLC codes".into()));
                }
                vlc.table[idx].len = n as i16;
                vlc.table[idx].sym = symbol;
                j += inc;
            }
        } else {
            // Subtable.
            n -= table_nb_bits;
            let code_prefix = code >> (32 - table_nb_bits);
            let mut subtable_bits = n;
            codes[i].bits = n as u8;
            codes[i].code = code << table_nb_bits;

            let mut k = i + 1;
            while k < nb_codes {
                let nn = codes[k].bits as i32 - table_nb_bits;
                if nn <= 0 {
                    break;
                }
                let cc = codes[k].code;
                if (cc >> (32 - table_nb_bits)) != code_prefix {
                    break;
                }
                codes[k].bits = nn as u8;
                codes[k].code = cc << table_nb_bits;
                if nn > subtable_bits {
                    subtable_bits = nn;
                }
                k += 1;
            }
            if subtable_bits > table_nb_bits {
                subtable_bits = table_nb_bits;
            }

            let j = if (flags & VLC_INIT_OUTPUT_LE) != 0 {
                (bitswap_32(code_prefix) >> (32 - table_nb_bits)) as i32
            } else {
                code_prefix as i32
            };

            let idx = base + j as usize;
            vlc.table[idx].len = -(subtable_bits as i16);

            let sub_index = build_table(vlc, subtable_bits, k - i, &mut codes[i..k], flags)?;

            // Rebase after possible resize.
            let base2 = table_index as usize;
            let idx2 = base2 + j as usize;
            vlc.table[idx2].sym = sub_index as i16;

            // Skip processed range.
            // Equivalent to `i = k - 1` in C loop.
            // We cannot easily modify `i` in Rust for-loop, so handle via while in caller.
        }
    }

    // Mark empty entries.
    let base3 = table_index as usize;
    for i in 0..table_size {
        let idx = base3 + i as usize;
        if vlc.table[idx].len == 0 {
            vlc.table[idx].sym = -1;
        }
    }

    Ok(table_index)
}

fn vlc_common_init(vlc: &mut Vlc, nb_bits: i32, flags: i32) {
    vlc.bits = nb_bits;
    vlc.table_size = 0;
    if (flags & VLC_INIT_USE_STATIC) == 0 {
        vlc.table.clear();
        vlc.table_allocated = 0;
    }
}

fn vlc_common_end(vlc: &mut Vlc, nb_bits: i32, codes: &mut [VlcCode], flags: i32) -> Result<()> {
    // upstream's build_table expects codes grouped; for sparse init it sorts.
    // We use a while loop in order to emulate the C for-loop that updates `i`.
    let nb_codes = codes.len();

    // Build table.
    // Our build_table implementation above uses recursion but does not update outer loop index.
    // To preserve upstream semantics, we rebuild using a local recursive builder that uses slices.

    // Re-implement build_table logic with slice recursion, closer to C.
    fn build(vlc: &mut Vlc, table_nb_bits: i32, codes: &mut [VlcCode], flags: i32) -> Result<i32> {
        if table_nb_bits > 30 {
            return Err(DecoderError::InvalidData("table_nb_bits > 30".into()));
        }
        let table_size = 1 << table_nb_bits;
        let table_index = alloc_table(vlc, table_size, (flags & VLC_INIT_USE_STATIC) != 0)?;
        let mut i: usize = 0;
        while i < codes.len() {
            let mut n = codes[i].bits as i32;
            let mut code = codes[i].code;
            let symbol = codes[i].symbol;

            let base = table_index as usize;

            if n <= table_nb_bits {
                let mut j = (code >> (32 - table_nb_bits)) as i32;
                let nb = 1 << (table_nb_bits - n);
                let mut inc = 1;
                if (flags & VLC_INIT_OUTPUT_LE) != 0 {
                    j = (bitswap_32(code) >> (32 - table_nb_bits)) as i32;
                    inc = 1 << n;
                }
                for _ in 0..nb {
                    let idx = base + j as usize;
                    let bits = vlc.table[idx].len;
                    let oldsym = vlc.table[idx].sym;
                    if (bits != 0 || oldsym != 0) && (bits != n as i16 || oldsym != symbol) {
                        return Err(DecoderError::InvalidData("incorrect VLC codes".into()));
                    }
                    vlc.table[idx].len = n as i16;
                    vlc.table[idx].sym = symbol;
                    j += inc;
                }
                i += 1;
            } else {
                // Subtable.
                n -= table_nb_bits;
                let code_prefix = code >> (32 - table_nb_bits);
                let mut subtable_bits = n;

                codes[i].bits = n as u8;
                codes[i].code = code << table_nb_bits;

                let mut k = i + 1;
                while k < codes.len() {
                    let nn = codes[k].bits as i32 - table_nb_bits;
                    if nn <= 0 {
                        break;
                    }
                    let cc = codes[k].code;
                    if (cc >> (32 - table_nb_bits)) != code_prefix {
                        break;
                    }
                    codes[k].bits = nn as u8;
                    codes[k].code = cc << table_nb_bits;
                    if nn > subtable_bits {
                        subtable_bits = nn;
                    }
                    k += 1;
                }

                if subtable_bits > table_nb_bits {
                    subtable_bits = table_nb_bits;
                }

                let j = if (flags & VLC_INIT_OUTPUT_LE) != 0 {
                    (bitswap_32(code_prefix) >> (32 - table_nb_bits)) as i32
                } else {
                    code_prefix as i32
                };

                {
                    let idx = base + j as usize;
                    vlc.table[idx].len = -(subtable_bits as i16);
                }

                let sub_index = build(vlc, subtable_bits, &mut codes[i..k], flags)?;

                // Reload base after possible resize.
                let base2 = table_index as usize;
                let idx2 = base2 + j as usize;
                vlc.table[idx2].sym = sub_index as i16;

                i = k;
            }
        }

        // Mark empty.
        let base = table_index as usize;
        for t in 0..table_size {
            let idx = base + t as usize;
            if vlc.table[idx].len == 0 {
                vlc.table[idx].sym = -1;
            }
        }

        Ok(table_index)
    }

    build(vlc, nb_bits, codes, flags)?;

    if (flags & VLC_INIT_USE_STATIC) != 0 {
        // Nothing.
        let _ = nb_codes;
    }

    Ok(())
}

fn get_data_u32(table: &[u8], wrap: i32, i: usize, size: i32) -> u32 {
    let off = i * wrap as usize;
    match size {
        1 => table[off] as u32,
        2 => u16::from_ne_bytes([table[off], table[off + 1]]) as u32,
        4 => u32::from_ne_bytes([table[off], table[off + 1], table[off + 2], table[off + 3]]),
        _ => 0,
    }
}

fn get_data_u16(table: &[u8], wrap: i32, i: usize, size: i32) -> u16 {
    let off = i * wrap as usize;
    match size {
        1 => table[off] as u16,
        2 => u16::from_ne_bytes([table[off], table[off + 1]]),
        _ => 0,
    }
}

/// Equivalent to upstream `ff_vlc_init_sparse()`.
#[allow(clippy::too_many_arguments)]
pub fn ff_vlc_init_sparse(
    vlc: &mut Vlc,
    nb_bits: i32,
    nb_codes: usize,
    bits: &[u8],
    bits_wrap: i32,
    bits_size: i32,
    codes: &[u8],
    codes_wrap: i32,
    codes_size: i32,
    symbols: Option<&[u8]>,
    symbols_wrap: i32,
    symbols_size: i32,
    flags: i32,
) -> Result<()> {
    vlc_common_init(vlc, nb_bits, flags);

    let mut buf: Vec<VlcCode> = Vec::with_capacity(nb_codes);

    // Copy entries with len > nb_bits first.
    for pass in 0..2 {
        for i in 0..nb_codes {
            let len = get_data_u32(bits, bits_wrap, i, bits_size) as u32;
            let cond = if pass == 0 { len > nb_bits as u32 } else { len != 0 && len <= nb_bits as u32 };
            if !cond {
                continue;
            }
            if len > (3 * nb_bits) as u32 || len > 32 {
                return Err(DecoderError::InvalidData(format!("Too long VLC ({len})")));
            }
            let mut code = get_data_u32(codes, codes_wrap, i, codes_size);
            if code as u64 >= (1u64 << len) {
                return Err(DecoderError::InvalidData(format!("Invalid code {code:x} for {i}")));
            }
            if (flags & VLC_INIT_INPUT_LE) != 0 {
                code = bitswap_32(code);
            } else {
                code <<= 32 - len;
            }
            let sym: i16 = if let Some(symtab) = symbols {
                get_data_u16(symtab, symbols_wrap, i, symbols_size) as i16
            } else {
                i as i16
            };
            buf.push(VlcCode { bits: len as u8, symbol: sym, code });
        }
        if pass == 0 {
            buf.sort_by_key(|c| c.code >> 1);
        }
    }

    vlc_common_end(vlc, nb_bits, &mut buf, flags)
}

/// Equivalent to upstream `ff_vlc_init_from_lengths()`.
#[allow(clippy::too_many_arguments)]
pub fn ff_vlc_init_from_lengths(
    vlc: &mut Vlc,
    nb_bits: i32,
    nb_codes: usize,
    lens: &[i8],
    lens_wrap: i32,
    symbols: Option<&[u8]>,
    symbols_wrap: i32,
    symbols_size: i32,
    offset: i32,
    flags: i32,
) -> Result<()> {
    vlc_common_init(vlc, nb_bits, flags);

    let mut buf: Vec<VlcCode> = Vec::with_capacity(nb_codes);
    let mut code: u64 = 0;
    let len_max: i32 = 32.min(3 * nb_bits);

    for i in 0..nb_codes {
        let len = lens[(i * lens_wrap as usize)] as i32;
        if len > 0 {
            let sym_u = if let Some(symtab) = symbols {
                get_data_u16(symtab, symbols_wrap, i, symbols_size) as u32
            } else {
                i as u32
            };
            let sym = (sym_u as i32 + offset) as i16;
            buf.push(VlcCode {
                bits: len as u8,
                symbol: sym,
                code: code as u32,
            });
        } else if len < 0 {
            // Incomplete tree marker.
        } else {
            continue;
        }

        let mut abs_len = len;
        if abs_len < 0 {
            abs_len = -abs_len;
        }
        if abs_len > len_max || (code & ((1u64 << (32 - abs_len)) - 1)) != 0 {
            return Err(DecoderError::InvalidData(format!("Invalid VLC (length {abs_len})")));
        }
        code += 1u64 << (32 - abs_len);
        if code > (u32::MAX as u64) + 1 {
            return Err(DecoderError::InvalidData("Overdetermined VLC tree".into()));
        }
    }

    vlc_common_end(vlc, nb_bits, &mut buf, flags)
}

/// Equivalent to `get_vlc2()`.
#[inline]
pub fn get_vlc2(gb: &mut GetBitContext<'_>, table: &[VlcElem], bits: i32, max_depth: i32) -> Result<i32> {
    let mut code: i32;
    let mut index = gb.show_bits(bits as usize)? as usize;
    let mut n = table[index].len as i32;
    code = table[index].sym as i32;

    if max_depth > 1 && n < 0 {
        gb.skip_bits(bits as usize)?;
        let mut nb_bits = -n;
        index = (gb.show_bits(nb_bits as usize)? as usize) + code as usize;
        n = table[index].len as i32;
        code = table[index].sym as i32;
        if max_depth > 2 && n < 0 {
            gb.skip_bits(nb_bits as usize)?;
            nb_bits = -n;
            index = (gb.show_bits(nb_bits as usize)? as usize) + code as usize;
            n = table[index].len as i32;
            code = table[index].sym as i32;
        }
    }

    gb.skip_bits(n as usize)?;
    Ok(code)
}

