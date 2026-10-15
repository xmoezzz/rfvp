#!/usr/bin/env python3
"""Pack the compiled script segments into SCRIPT.AOT (see README.md).

    aot_link.py <aot dir> <rfvp.elf> <SCRIPT.AOT>

<aot dir> is hcb_aot.py's output with its seg*.c compiled to obj/seg*.c.o. The segments call the
runtime in the executable: those references are resolved here, against rfvp.elf, so SCRIPT.AOT
goes with the executable it was packed for. What refers to the segment itself (its table of
entry points) is left to the loader, which places a segment anywhere.

SCRIPT.AOT:
    header      "AOT2", u32 segments, u32 pages, u32 page table offset, u32 thread table
                offset, u32 thread count, u32 globals, u32 segments a recorded run used (the
                first ones)
    pages       consecutive segments, read and cached together (a CD read costs a seek, and
                the music's): u16 segment count, u16 offset of each segment in the page, then
                the segments, each at a multiple of 8: u16 code bytes, u16 relocations, the code
                (text, then its entry table at a multiple of 4), then the relocations: u16
                (type << 14 | offset / 4), u16 target offset; type 0 = HI16, 1 = LO16; then
                its short strings: u16 count, each u32 script address, u8 length, the bytes
    page table  per page: u32 offset, u16 first segment, u16 pad; then one more offset
    threads     per function a thread can start at: u32 script address, u16 segment,
                u16 frame slots
"""

import json
import os
import struct
import sys

R_MIPS_32, R_MIPS_26, R_MIPS_HI16, R_MIPS_LO16 = 2, 4, 5, 6


class Elf:
    def __init__(self, path):
        self.data = d = open(path, 'rb').read()
        if d[:4] != b'\x7fELF' or d[4] != 1 or d[5] != 1:
            raise SystemExit(f'{path}: not a little-endian ELF32 file')
        shoff = struct.unpack_from('<I', d, 32)[0]
        shentsize, shnum, shstrndx = struct.unpack_from('<HHH', d, 46)
        self.sections = []
        for i in range(shnum):
            name, typ, flags, addr, off, size, link, info, align, entsize = \
                struct.unpack_from('<IIIIIIIIII', d, shoff + i * shentsize)
            self.sections.append({'name': name, 'type': typ, 'addr': addr, 'off': off,
                                  'size': size, 'link': link, 'info': info})
        strtab = self.sections[shstrndx]
        for s in self.sections:
            s['name'] = self.cstr(strtab['off'] + s['name'])
        self.symbols = []
        for s in self.sections:
            if s['type'] == 2:  # SHT_SYMTAB
                names = self.sections[s['link']]['off']
                for k in range(s['size'] // 16):
                    name, value, size, info, other, shndx = \
                        struct.unpack_from('<IIIBBH', d, s['off'] + k * 16)
                    self.symbols.append({'name': self.cstr(names + name), 'value': value,
                                         'shndx': shndx, 'type': info & 15})

    def cstr(self, off):
        return self.data[off:self.data.index(b'\0', off)].decode()

    def section_bytes(self, s):
        return bytearray(self.data[s['off']:s['off'] + s['size']])

    def relocs(self, target_index):
        for s in self.sections:
            if s['type'] == 9 and s['info'] == target_index:  # SHT_REL
                for k in range(s['size'] // 8):
                    off, info = struct.unpack_from('<II', self.data, s['off'] + k * 8)
                    yield off, info & 0xff, info >> 8


def exe_symbols(path):
    elf = Elf(path)
    return {s['name']: s['value'] for s in elf.symbols if s['shndx'] != 0 and s['name']}


def pack_segment(elf, text_index, rodata_index, runtime):
    """The segment's bytes and its relocations for the loader."""
    text = elf.section_bytes(elf.sections[text_index])
    base = {text_index: 0}
    blob = bytearray(text)
    if rodata_index is not None:
        blob += b'\0' * (-len(blob) % 4)
        base[rodata_index] = len(blob)
        blob += elf.section_bytes(elf.sections[rodata_index])
    out_relocs = []
    for index in base:
        rels = list(elf.relocs(index))
        for k, (off, typ, sym) in enumerate(rels):
            at = base[index] + off
            s = elf.symbols[sym]
            word = struct.unpack_from('<I', blob, at)[0]
            if s['shndx'] in base:
                # Within the segment: the loader adds where it is placed.
                if s['type'] == 3:   # STT_SECTION
                    target_base = base[s['shndx']]
                else:
                    target_base = base[s['shndx']] + s['value']
                if typ == R_MIPS_HI16:
                    lo = next(r for r in rels[k + 1:] if r[1] == R_MIPS_LO16 and r[2] == sym)
                    lo_word = struct.unpack_from('<I', blob, base[index] + lo[0])[0]
                    addend = ((word & 0xffff) << 16) + ((lo_word & 0xffff) ^ 0x8000) - 0x8000
                    out_relocs.append((0, at, target_base + addend))
                elif typ == R_MIPS_LO16:
                    addend = ((word & 0xffff) ^ 0x8000) - 0x8000
                    out_relocs.append((1, at, target_base + addend))
                else:
                    raise SystemExit(f'relocation type {typ} within a segment')
                continue
            if s['shndx'] != 0:
                raise SystemExit(f'segment refers to another section ({s["name"]})')
            name = s['name']
            if name not in runtime:
                raise SystemExit(f'the executable has no {name}')
            addr = runtime[name]
            if typ == R_MIPS_26:
                target = addr + ((word & 0x3ffffff) << 2)
                word = (word & 0xfc000000) | ((target >> 2) & 0x3ffffff)
            elif typ == R_MIPS_HI16:
                lo = next(r for r in rels[k + 1:] if r[1] == R_MIPS_LO16 and r[2] == sym)
                lo_word = struct.unpack_from('<I', blob, base[index] + lo[0])[0]
                value = addr + ((word & 0xffff) << 16) + ((lo_word & 0xffff) ^ 0x8000) - 0x8000
                word = (word & 0xffff0000) | (((value + 0x8000) >> 16) & 0xffff)
                # The low half is resolved with its own relocation.
            elif typ == R_MIPS_LO16:
                value = addr + ((word & 0xffff) ^ 0x8000) - 0x8000
                word = (word & 0xffff0000) | (value & 0xffff)
            elif typ == R_MIPS_32:
                word = (word + addr) & 0xffffffff
            else:
                raise SystemExit(f'relocation type {typ} to {name}')
            struct.pack_into('<I', blob, at, word)
    # Entry tables hold signed 16-bit label offsets.
    if len(blob) >= 0x8000:
        raise SystemExit('segment over 32 KiB')
    return blob, out_relocs


def main():
    if len(sys.argv) != 4:
        raise SystemExit(__doc__)
    aot_dir, exe, out = sys.argv[1:]
    index = json.load(open(os.path.join(aot_dir, 'aot_index.json')))
    runtime = exe_symbols(exe)
    n = index['segments']
    segs = [None] * n
    for name in index['files']:
        elf = Elf(os.path.join(aot_dir, 'obj', name + '.o'))
        by_name = {s['name']: i for i, s in enumerate(elf.sections)}
        for i, s in enumerate(elf.sections):
            if s['name'].startswith('.text.aot_s'):
                sid = int(s['name'][len('.text.aot_s'):])
                segs[sid] = pack_segment(elf, i, by_name.get(f'.rodata.aot_s{sid}'), runtime)
    missing = [i for i, s in enumerate(segs) if s is None]
    if missing:
        raise SystemExit(f'segments not compiled: {missing[:10]}')

    page_bytes = int(os.environ.get('AOT_PAGE_BYTES', '8192'))
    records = []
    for sid, (blob, relocs) in enumerate(segs):
        rec = struct.pack('<HH', len(blob), len(relocs)) + blob
        for typ, at, target in relocs:
            rec += struct.pack('<HH', typ << 14 | at // 4, target)
        strings = index['strings'][sid] or []
        rec += struct.pack('<H', len(strings))
        for addr, raw in strings:
            rec += struct.pack('<I', addr) + bytes.fromhex(raw)
        rec += b'\0' * (-len(rec) % 8)
        records.append(rec)
    # Pages of consecutive segments (the script's order: a function's neighbours are often
    # what runs with it).
    pages = []
    k = 0
    while k < n:
        first = k
        size = 0
        while k < n and (k == first or size + len(records[k]) + 2 <= page_bytes):
            size += len(records[k]) + 2
            k += 1
        pages.append((first, k))
    body = bytearray(b'\0' * 32)  # the header
    table = []
    code_bytes = sum(len(b) for b, _ in segs)
    for first, end in pages:
        body += b'\0' * (-len(body) % 8)
        table.append((len(body), first))
        head = 2 + 2 * (end - first)
        head += -head % 8
        offsets = []
        at = head
        for i in range(first, end):
            offsets.append(at)
            at += len(records[i])
        if at >= 0x10000:
            raise SystemExit('page over 64 KiB')
        page = struct.pack(f'<H{end - first}H', end - first, *offsets)
        page += b'\0' * (head - len(page))
        for i in range(first, end):
            page += records[i]
        body += page
    body += b'\0' * (-len(body) % 8)
    table_off = len(body)
    for off, first in table:
        body += struct.pack('<IHH', off, first, 0)
    body += struct.pack('<IHH', table_off, n, 0)  # where the last page ends
    thr_off = len(body)
    for addr, seg, nslots in index['threads']:
        body += struct.pack('<IHH', addr, seg, nslots)
    struct.pack_into('<4sIIIIIII', body, 0, b'AOT2', n, len(pages), table_off, thr_off,
                     len(index['threads']), index['globals'], index.get('hot_segments', 0))
    largest = max(table[i + 1][0] - table[i][0] for i in range(len(table) - 1))
    open(out, 'wb').write(body)
    print(f'aot_link: {n} segments in {len(pages)} pages (largest {largest} bytes), '
          f'{code_bytes} bytes of code, {len(body)} bytes')


if __name__ == '__main__':
    main()
