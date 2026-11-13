#!/usr/bin/env python3
"""Compile PR #57's offset-based patch.dat into a standalone, ordinary HCB."""

import argparse
from dataclasses import dataclass
import os
from pathlib import Path
import struct
import tempfile
import zlib


PATCH_LIMIT = 16 * 1024 * 1024
HCB_LIMIT = 256 * 1024 * 1024
ENCODINGS = {"sjis": "cp932", "gbk": "gbk", "utf8": "utf-8"}
SIZES = [1] * 0x28
for _opcode, _size in {
    0x01: 3, 0x02: 5, 0x03: 3, 0x06: 5, 0x07: 5,
    0x0A: 5, 0x0B: 3, 0x0C: 2, 0x0D: 5,
    0x0F: 3, 0x10: 2, 0x11: 3, 0x12: 2,
    0x15: 3, 0x16: 2, 0x17: 3, 0x18: 2,
}.items():
    SIZES[_opcode] = _size


def require(condition, message):
    if not condition:
        raise ValueError(message)


def read_file(path, limit):
    with path.open("rb") as stream:
        data = stream.read(limit + 1)
    require(len(data) <= limit, "{} exceeds {} bytes".format(path, limit))
    return data


class Reader:
    def __init__(self, data, offset=0):
        self.data = data
        self.pos = offset

    def take(self, size):
        end = self.pos + size
        require(end <= len(self.data), "truncated data at {:#x}".format(self.pos))
        value = self.data[self.pos:end]
        self.pos = end
        return value

    def number(self, fmt):
        return struct.unpack("<" + fmt, self.take(struct.calcsize("<" + fmt)))[0]

    def cstring(self, encoding, label):
        size = self.number("B")
        raw = self.take(size)
        require(size > 0 and raw[-1] == 0, label + ": missing NUL terminator")
        # Native readers stop at the first NUL; retain this padding on input.
        return raw.split(b"\0", 1)[0].decode(encoding, errors="strict")


def cstring(text, encoding, label):
    require("\0" not in text, label + ": interior NUL")
    raw = text.encode(encoding, errors="strict") + b"\0"
    require(len(raw) <= 255,
            label + ": exceeds the native 254-byte text limit in the output encoding")
    return bytes([len(raw)]) + raw


@dataclass
class Instruction:
    pc: int
    raw: bytes
    text: object = None

    @property
    def opcode(self):
        return self.raw[0]


class Hcb:
    def __init__(self, data, encoding):
        header = Reader(data)
        self.code_end = header.number("I")
        require(4 < self.code_end < len(data), "invalid HCB system-description offset")
        code = Reader(data[:self.code_end], 4)
        self.instructions = []
        while code.pos < self.code_end:
            pc = code.pos
            opcode = code.number("B")
            require(opcode < len(SIZES), "unknown opcode {:#x} at {:#x}".format(opcode, pc))
            text = None
            if opcode == 0x0E:
                text = code.cstring(encoding, "string at {:#x}".format(pc + 2))
            else:
                code.take(SIZES[opcode] - 1)
            self.instructions.append(Instruction(pc, data[pc:code.pos], text))
        self.starts = {ins.pc for ins in self.instructions}
        self.strings = {ins.pc + 2: ins for ins in self.instructions if ins.opcode == 0x0E}
        meta = Reader(data, self.code_end)
        self.entry = meta.number("I")
        # Preserve both global counts, mode, and the reserved byte verbatim.
        self.settings = meta.take(6)
        self.title = meta.cstring(encoding, "game title")
        self.syscalls = []
        for _ in range(meta.number("H")):
            args = meta.number("B")
            name = meta.cstring(encoding, "syscall name")
            self.syscalls.append((args, name))
        require(meta.number("H") == 0, "custom syscall tables are not supported")
        require(meta.pos == len(data), "unrecognized data after the HCB syscall table")
        require(self.entry in self.starts, "HCB entry is not an instruction boundary")
        for ins in self.instructions:
            if ins.opcode in (0x02, 0x06, 0x07):
                target = struct.unpack_from("<I", ins.raw, 1)[0]
                require(target in self.starts,
                        "branch at {:#x} targets a non-instruction address {:#x}".format(ins.pc, target))
            elif ins.opcode == 0x03:
                index = struct.unpack_from("<H", ins.raw, 1)[0]
                require(index < len(self.syscalls), "syscall index out of range at {:#x}".format(ins.pc))

    def metadata(self, entry, encoding):
        out = bytearray(struct.pack("<I", entry) + self.settings)
        out += cstring(self.title, encoding, "game title")
        out += struct.pack("<H", len(self.syscalls))
        for args, name in self.syscalls:
            out.append(args)
            out += cstring(name, encoding, "syscall name " + name)
        out += b"\0\0"  # No custom syscall table.
        return out


def read_patch(data, hcb, encoding):
    require(4 <= len(data) <= PATCH_LIMIT, "invalid patch size")
    if struct.unpack_from("<I", data)[0] == 0:
        require(len(data) >= 16, "truncated compressed patch header")
        unpacked, packed = struct.unpack_from("<II", data, 8)
        require(4 <= unpacked <= PATCH_LIMIT and packed + 16 == len(data),
                "invalid compressed patch lengths")
        inflater = zlib.decompressobj()
        decoded = inflater.decompress(data[16:], unpacked + 1)
        require(len(decoded) == unpacked and inflater.eof
                and not inflater.unused_data and not inflater.unconsumed_tail,
                "compressed patch length/stream mismatch")
        data = decoded
    reader = Reader(data)
    count = reader.number("I")
    require(count <= (len(data) - 4) // 6, "invalid patch record count")
    seen = set()
    translations = {}
    for _ in range(count):
        offset = reader.number("I")
        size = reader.number("H")
        raw = reader.take(size)
        require(offset in hcb.strings,
                "patch offset {:#x} is not an original PushString payload".format(offset))
        require(offset not in seen, "duplicate patch offset {:#x}".format(offset))
        seen.add(offset)
        if size == 0:  # Original format: retain this literal, rather than erase it.
            continue
        require(raw[-1] == 0 and b"\0" not in raw[:-1],
                "invalid patch string at {:#x}".format(offset))
        translations[offset] = raw[:-1].decode(encoding, errors="strict")
    require(reader.pos == len(data), "trailing patch bytes")
    return translations


def replacements(hcb, translations, encoding):
    return {
        ins.pc: b"\x0e" + cstring(translations.get(ins.pc + 2, ins.text), encoding,
                                  "string at {:#x}".format(ins.pc + 2))
        for ins in hcb.instructions if ins.opcode == 0x0E
    }


def assemble_preserving(hcb, strings, original, encoding):
    # Do not guess whether an integer immediate is a thread entry or a number.
    # Keep all original slots and move only growing literals to native JMP islands.
    code = bytearray(original[4:hcb.code_end])
    islands = bytearray()
    moved = 0
    for ins in hcb.instructions:
        if ins.opcode != 0x0E:
            continue
        new = strings[ins.pc]
        slot = len(ins.raw)
        if len(new) <= slot:
            # Keep the original encoded length; native readers ignore NUL padding.
            new = b"\x0e" + bytes([slot - 2]) + new[2:]
            code[ins.pc - 4:ins.pc - 4 + slot] = new.ljust(slot, b"\0")
        else:
            require(slot >= 5,
                    "growing string at {:#x} has a {}-byte slot; use --layout relocate "
                    "and classify any integer address references".format(ins.pc + 2, slot))
            destination = hcb.code_end + len(islands)
            require(destination + len(new) + 5 <= HCB_LIMIT, "output HCB too large")
            islands += new + b"\x06" + struct.pack("<I", ins.pc + slot)
            jump = b"\x06" + struct.pack("<I", destination)
            code[ins.pc - 4:ins.pc - 4 + slot] = jump.ljust(slot, b"\0")
            moved += 1
    # Appending islands must not turn a formerly invalid fallthrough into a loop.
    require(not islands or hcb.instructions[-1].opcode in (0x04, 0x05, 0x06),
            "cannot append string islands after bytecode that falls through its end")
    end = hcb.code_end + len(islands)
    return struct.pack("<I", end) + code + islands + hcb.metadata(hcb.entry, encoding), moved


def assemble_relocated(hcb, strings, encoding, address_pushes, literal_pushes):
    require(not address_pushes & literal_pushes, "a push cannot be both an address and a literal")
    locations = {}
    integer_pushes = {}
    end = 4
    for ins in hcb.instructions:
        locations[ins.pc] = end
        if ins.opcode == 0x0E:
            locations[ins.pc + 2] = end + 2
        if ins.opcode in (0x0A, 0x0B, 0x0C):
            fmt = {0x0A: "i", 0x0B: "h", 0x0C: "b"}[ins.opcode]
            integer_pushes[ins.pc] = struct.unpack("<" + fmt, ins.raw[1:])[0]
        # Explicit address operands use PushI32 even if the original was shorter.
        end += 5 if ins.pc in address_pushes else len(strings.get(ins.pc, ins.raw))
    require(end <= HCB_LIMIT, "output HCB too large")
    require((address_pushes | literal_pushes) <= integer_pushes.keys(),
            "--address-push/--literal-push must identify original integer-push opcode offsets")
    require(all(integer_pushes[pc] in locations for pc in address_pushes),
            "an --address-push value is not an original instruction/string address")
    ambiguous = [pc for pc, value in integer_pushes.items()
                 if value in locations and locations[value] != value
                 and pc not in address_pushes and pc not in literal_pushes]
    require(not ambiguous,
            "ambiguous integer pushes (original opcode offsets): {}. Classify each with "
            "--address-push OFFSET or --literal-push OFFSET; numbers are never guessed to be pointers."
            .format(", ".join(hex(pc) for pc in ambiguous)))
    code = bytearray()
    for ins in hcb.instructions:
        raw = strings.get(ins.pc, ins.raw)
        if ins.opcode in (0x02, 0x06, 0x07):
            target = struct.unpack_from("<I", ins.raw, 1)[0]
            raw = bytes([ins.opcode]) + struct.pack("<I", locations[target])
        elif ins.pc in address_pushes:
            raw = b"\x0a" + struct.pack("<i", locations[integer_pushes[ins.pc]])
        code += raw
    return struct.pack("<I", end) + code + hcb.metadata(locations[hcb.entry], encoding), 0


def write_new_file(path, data):
    # Stage beside the output, then link without replacing any existing file.
    # Both source files are fully read before this point and are never opened for writing.
    fd, temporary = tempfile.mkstemp(prefix=".hcb-patch-", dir=str(path.parent))
    try:
        with os.fdopen(fd, "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.link(temporary, str(path))
    finally:
        os.unlink(temporary)


def offset(value):
    try:
        number = int(value, 0)
        if number < 4:
            raise ValueError()
        return number
    except ValueError:
        raise argparse.ArgumentTypeError("expected an opcode offset >= 4 (decimal or 0xHEX)")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("hcb", type=Path, help="original HCB matching patch.dat's offsets")
    parser.add_argument("patch", type=Path, help="original raw or zlib-wrapped patch.dat")
    parser.add_argument("-o", "--output", required=True, type=Path, help="new HCB; must not exist")
    parser.add_argument("--input-encoding", choices=ENCODINGS, default="sjis")
    parser.add_argument("--patch-encoding", choices=ENCODINGS, default="gbk")
    parser.add_argument("--output-encoding", choices=ENCODINGS, default="utf8")
    parser.add_argument("--layout", choices=("preserve", "relocate"), default="preserve")
    parser.add_argument("--address-push", action="append", type=offset, default=[], metavar="OFFSET",
                        help="relocate: original integer-push opcode holding a code/string address; repeatable")
    parser.add_argument("--literal-push", action="append", type=offset, default=[], metavar="OFFSET",
                        help="relocate: explicitly retain an ambiguous ordinary integer; repeatable")
    args = parser.parse_args()
    try:
        require(not os.path.lexists(args.output), "output already exists: " + str(args.output))
        require(args.layout == "relocate" or not (args.address_push or args.literal_push),
                "integer classifications require --layout relocate")
        original = read_file(args.hcb, HCB_LIMIT)
        hcb = Hcb(original, ENCODINGS[args.input_encoding])
        translations = read_patch(read_file(args.patch, PATCH_LIMIT), hcb, ENCODINGS[args.patch_encoding])
        encoding = ENCODINGS[args.output_encoding]
        strings = replacements(hcb, translations, encoding)
        if args.layout == "preserve":
            output, islands = assemble_preserving(hcb, strings, original, encoding)
        else:
            output, islands = assemble_relocated(hcb, strings, encoding,
                                                 set(args.address_push), set(args.literal_push))
        require(len(output) <= HCB_LIMIT, "output HCB too large")
        write_new_file(args.output, output)
        print("Wrote {}: {} translated literals, {} bytes, {} string islands; nls={}".format(
            args.output, len(translations), len(output), islands, args.output_encoding))
    except (OSError, ValueError, struct.error, zlib.error) as error:
        parser.exit(1, "hcb_patch: {}\n".format(error))


if __name__ == "__main__":
    main()
