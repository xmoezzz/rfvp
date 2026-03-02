use super::bitreader::GetBits;

#[derive(Clone, Copy, Default, Debug)]
pub struct VlcElem {
    pub sym: i16,
    pub len: i16,
}

#[derive(Clone, Debug)]
pub struct Vlc {
    pub bits: i32,
    pub table: Vec<VlcElem>,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct RlVlcElem {
    pub level: i16,
    pub len8: i8,
    pub run: u8,
}

#[derive(Clone, Copy, Debug)]
struct VlcCode {
    bits: u8,
    symbol: i16,
    code: u32, // msb-aligned (first bit to read at msb)
}

fn alloc_table(vlc: &mut Vlc, size: usize) -> usize {
    let index = vlc.table.len();
    vlc.table.resize(index + size, VlcElem::default());
    index
}

fn build_table(vlc: &mut Vlc, table_nb_bits: i32, codes: &mut [VlcCode]) -> Result<usize, ()> {
    if table_nb_bits > 30 {
        return Err(());
    }
    let table_size = 1usize << (table_nb_bits as usize);
    let table_index = alloc_table(vlc, table_size);

    // first pass
    let mut i = 0usize;
    while i < codes.len() {
        let mut n = codes[i].bits as i32;
        let mut code = codes[i].code;
        let symbol = codes[i].symbol;

        if n <= table_nb_bits {
            let mut j = (code >> (32 - table_nb_bits)) as usize;
            let nb = 1usize << ((table_nb_bits - n) as usize);
            for _ in 0..nb {
                let entry = &mut vlc.table[table_index + j];
                let bits = entry.len;
                let oldsym = entry.sym;
                if (bits != 0 || oldsym != 0) && (bits as i32 != n || oldsym != symbol) {
                    return Err(());
                }
                entry.len = n as i16;
                entry.sym = symbol;
                j += 1;
            }
            i += 1;
        } else {
            // subtable
            n -= table_nb_bits;
            let code_prefix = code >> (32 - table_nb_bits);
            let mut subtable_bits = n;

            codes[i].bits = n as u8;
            codes[i].code = code << (table_nb_bits as u32);

            let mut k = i + 1;
            while k < codes.len() {
                let n2 = codes[k].bits as i32 - table_nb_bits;
                if n2 <= 0 {
                    break;
                }
                let code2 = codes[k].code;
                if (code2 >> (32 - table_nb_bits)) != code_prefix {
                    break;
                }
                codes[k].bits = n2 as u8;
                codes[k].code = code2 << (table_nb_bits as u32);
                if n2 > subtable_bits {
                    subtable_bits = n2;
                }
                k += 1;
            }

            if subtable_bits > table_nb_bits {
                subtable_bits = table_nb_bits;
            }

            let j = code_prefix as usize;
            let e = &mut vlc.table[table_index + j];
            e.len = -(subtable_bits as i16);

            // recurse
            let sub_index = build_table(vlc, subtable_bits, &mut codes[i..k])?;
            // reload entry
            vlc.table[table_index + j].sym = sub_index as i16;

            i = k;
        }
    }

    // finalize: mark illegal
    for t in &mut vlc.table[table_index..table_index + table_size] {
        if t.len == 0 {
            t.sym = -1;
        }
    }

    Ok(table_index)
}

impl Vlc {
    pub fn init_sparse(nb_bits: i32, bits: &[u8], codes: &[u16], symbols: Option<&[i16]>) -> Self {
        // Build a canonical list of (code,len,symbol) entries and sort by the
        // MSB-aligned code.
        //
        // IMPORTANT: VLC table construction relies on codes being grouped by
        // prefix. Any ordering hack (e.g. splitting by length or sorting by a
        // shifted key like `code >> 1`) will corrupt the table and cause
        // widespread "invalid data: ac" failures.
        let mut buf: Vec<VlcCode> = Vec::with_capacity(bits.len());

        #[inline]
        fn push_code(
            buf: &mut Vec<VlcCode>,
            i: usize,
            bits: &[u8],
            codes: &[u16],
            symbols: Option<&[i16]>,
        ) {
            let len = bits[i] as i32;
            if len <= 0 {
                return;
            }
            let mut code = codes[i] as u32;
            // validate
            if len > 32 {
                return;
            }
            if code >= (1u32 << len) {
                return;
            }
            code <<= 32 - len;
            let sym = symbols.map(|s| s[i]).unwrap_or(i as i16);
            buf.push(VlcCode { bits: len as u8, symbol: sym, code });
        }

        for i in 0..bits.len() {
            push_code(&mut buf, i, bits, codes, symbols);
        }

        // Sort by full MSB-aligned code (NOT shifted) to avoid collisions.
        buf.sort_by(|a, b| a.code.cmp(&b.code));

        let mut vlc = Vlc { bits: nb_bits, table: Vec::new() };
        let _ = build_table(&mut vlc, nb_bits, &mut buf).unwrap();
        vlc
    }
}

/// Equivalent to `get_vlc2()`.
#[inline]
pub fn get_vlc2(gb: &mut GetBits<'_>, table: &[VlcElem], bits: i32, max_depth: i32) -> i32 {
    let mut index = gb.show_bits(bits as usize) as usize;
    let mut code = table[index].sym as i32;
    let mut n = table[index].len as i32;

    if max_depth > 1 && n < 0 {
        gb.skip_bits(bits as usize);
        let mut nb_bits = -n;
        index = gb.show_bits(nb_bits as usize) as usize + (code as usize);
        code = table[index].sym as i32;
        n = table[index].len as i32;
        if max_depth > 2 && n < 0 {
            gb.skip_bits(nb_bits as usize);
            nb_bits = -n;
            index = gb.show_bits(nb_bits as usize) as usize + (code as usize);
            code = table[index].sym as i32;
            n = table[index].len as i32;
        }
    }

    if n > 0 {
        gb.skip_bits(n as usize);
    }
    code
}

/// Equivalent to `GET_RL_VLC` macro for run/level VLC tables.
#[inline]
pub fn get_rl_vlc(gb: &mut GetBits<'_>, table: &[RlVlcElem], bits: i32, max_depth: i32) -> (i16, u8) {
    let mut index = gb.show_bits(bits as usize) as usize;
    let mut level = table[index].level;
    let mut n = table[index].len8 as i32;

    if max_depth > 1 && n < 0 {
        gb.skip_bits(bits as usize);
        let mut nb_bits = -n;
        index = gb.show_bits(nb_bits as usize) as usize + (level as usize);
        level = table[index].level;
        n = table[index].len8 as i32;
        if max_depth > 2 && n < 0 {
            gb.skip_bits(nb_bits as usize);
            nb_bits = -n;
            index = gb.show_bits(nb_bits as usize) as usize + (level as usize);
            level = table[index].level;
            n = table[index].len8 as i32;
        }
    }

    let run = table[index].run;
    if n > 0 {
        gb.skip_bits(n as usize);
    }
    (level, run)
}
