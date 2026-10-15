#!/usr/bin/env python3
"""Ahead-of-time compiler from an HCB script to C for the PlayStation (see README.md).

    hcb_aot.py <script.hcb> <out dir> [function order [instruction counts]]

[function order]: script function addresses and call counts, one per line, in the order a run
of the game first called them (the desktop harness writes it: RFVP_HARNESS_FUNCS). The often
called functions get inlining and native nested calls; the code is laid out in that order, so
that starting the game reads one run of the disc.

[instruction counts]: how many times each instruction ran in a recorded run (script address and
count per line; the desktop harness writes it: RFVP_HARNESS_PCS). The segments that run all
the time are laid out first, in few pages, which the code cache then keeps.

Writes <out>/seg*.c (the script as C, one function per segment), <out>/aot_nslots.h (frame
sizes) and <out>/aot_index.json (what aot_link.py needs to pack the compiled segments).

Strings stay in the HCB file on the disc: the code only holds a string's address in it (which
is also its identity for TextPrint, as in the interpreter), and the runtime reads it from there
when a syscall needs its text.

Every HCB function becomes one or more segments: C functions `int aot_sN(Frame *f)` that run the
script from the frame's resume point until it calls a script function, returns, yields or leaves
the segment, and then go back to the driver (crates/rfvp/src/aot). The evaluation stack is
resolved here: values are C expressions and variables, and only what lives across a call or a
yield is kept in the frame. Values follow the VM's semantics exactly: ints are folded and
computed inline when their types are known, everything else (floats, strings, tables) goes
through the runtime's helpers, which use the engine's own operations.
"""

import bisect
import json
import os
import struct
import sys
from collections import defaultdict

# ---------------------------------------------------------------------------------------------
# Bytecode

OP_LEN = {0x01: 3, 0x02: 5, 0x03: 3, 0x06: 5, 0x07: 5, 0x0a: 5, 0x0b: 3, 0x0c: 2, 0x0d: 5,
          0x0f: 3, 0x10: 2, 0x11: 3, 0x12: 2, 0x15: 3, 0x16: 2, 0x17: 3, 0x18: 2}

# Syscalls that can stop the current thread (they post a thread request that yields): the code
# checks the runtime's yield flag after them.
YIELDING_SYSCALLS = {
    'ThreadNext', 'ThreadWait', 'ThreadExit', 'ThreadSleep', 'ThreadRaise', 'ThreadStart',
    'DissolveWait', 'TextPrint', 'SaveCreate', 'SaveWrite', 'Load', 'LoadFile', 'SaveFile',
    'ExitMode', 'ExitDialog',
}

# Strings up to this long (with the NUL) are stored with the segments that use them.
SHORT_STRING = 64

# Value tags (aot.h).
T_NIL, T_TRUE, T_INT, T_FLOAT, T_CSTR, T_STR, T_TABLE = 0, 1, 2, 3, 4, 5, 6

# Sets of tags (type inference): one bit per tag.
M_NIL, M_TRUE, M_INT, M_FLOAT, M_CSTR, M_STR, M_TABLE = (1 << t for t in range(7))
M_BOOL = M_NIL | M_TRUE
M_ANY = (1 << 7) - 1
# Values whose 64 bits are canonical, so that equal values are equal words.
M_SIMPLE = M_NIL | M_TRUE | M_INT

# Runtime binary operations (aot.h, AOT_OP_*), in opcode order from Add.
BINOPS = ['add', 'sub', 'mul', 'div', 'mod', 'bittest', 'and', 'or', 'eq', 'ne', 'gt', 'ge', 'lt', 'le']


class Hcb:
    def __init__(self, path):
        self.data = data = open(path, 'rb').read()
        self.orig = data
        self.code_end = struct.unpack_from('<I', data, 0)[0]
        # After inlining (inline_program), the script's own addresses: of functions (by their
        # new address) and of string payloads (by the new address of the PushString).
        self.orig_func = None
        self.str_addr = {}
        self.origin = {}
        o = self.code_end
        self.entry, self.nv_globals, self.v_globals = struct.unpack_from('<IHH', data, o)
        o += 10
        o += 1 + data[o]
        count = struct.unpack_from('<H', data, o)[0]
        o += 2
        self.syscalls = []
        for _ in range(count):
            argc, n = data[o], data[o + 1]
            self.syscalls.append((data[o + 2:o + 2 + n].rstrip(b'\0').decode('latin1'), argc))
            o += 2 + n
        self.decode()

    def decode(self):
        data = self.data
        self.insts = {}
        pc = 4
        while pc < self.code_end:
            op = data[pc]
            if op > 0x29:
                raise SystemExit(f'unknown opcode {op:#x} at {pc}')
            n = 2 + data[pc + 1] if op == 0x0e else OP_LEN.get(op, 1)
            self.insts[pc] = (op, n)
            pc += n
        if pc != self.code_end:
            raise SystemExit('code does not end on an instruction')
        self.funcs = sorted(p for p, (op, _) in self.insts.items() if op == 0x01)

    def u8(self, p): return self.data[p]
    def i8(self, p): return struct.unpack_from('<b', self.data, p)[0]
    def u16(self, p): return struct.unpack_from('<H', self.data, p)[0]
    def i16(self, p): return struct.unpack_from('<h', self.data, p)[0]
    def i32(self, p): return struct.unpack_from('<i', self.data, p)[0]
    def u32(self, p): return struct.unpack_from('<I', self.data, p)[0]


# ---------------------------------------------------------------------------------------------
# Values during compilation

class Val:
    """A value on the evaluation stack: a constant, or a C expression of a known or unknown tag.

    `cond` values are C ints (0 or 1) for True/Nil, kept as such so that comparisons feed
    branches directly; `int` values are C ints of a value known to be an int. `mask` is the set
    of tags the value can have (type inference, `Types`)."""
    __slots__ = ('kind', 'tag', 'num', 'expr', 'vmask')

    def __init__(self, kind, tag=None, num=0, expr=None, mask=None):
        self.kind, self.tag, self.num, self.expr, self.vmask = kind, tag, num, expr, mask

    @property
    def mask(self):
        if self.kind == 'const':
            return 1 << self.tag
        if self.kind == 'int':
            return M_INT
        if self.kind == 'cond':
            return M_BOOL
        return self.vmask or M_ANY

    @staticmethod
    def const(tag, num=0):
        return Val('const', tag, num & 0xffffffff)

    def is_const(self, tag=None):
        return self.kind == 'const' and (tag is None or self.tag == tag)

    def known_int(self):
        return self.mask == M_INT


NIL = Val.const(T_NIL)
TRUE = Val.const(T_TRUE)


def s32(n):
    n &= 0xffffffff
    return n - (1 << 32) if n & 0x80000000 else n


def c_int(n):
    n = s32(n)
    return str(n) if n != -0x80000000 else '(-0x7fffffff - 1)'


# ---------------------------------------------------------------------------------------------
# Function analysis

class Func:
    def __init__(self, hcb, index, start, end):
        self.hcb, self.index, self.start, self.end = hcb, index, start, end
        self.nargs, self.nlocals = struct.unpack_from('<bb', hcb.data, start + 1)
        self.depth = {}      # pc -> stack depth before the instruction
        self.leaders = set()
        self.max_spill = 0

    def slot(self, offset):
        """Frame slot of a stack offset relative to the frame base (args below -1, locals from 0)."""
        if offset < -1:
            return offset + self.nargs + 1
        if offset >= 0:
            return self.nargs + offset
        raise SystemExit(f'function {self.start}: access to the saved frame info')

    def analyze(self, funcs_by_addr):
        h = self.hcb
        d = {self.start: 0}
        work = [self.start]
        self.leaders.add(self.start)
        while work:
            pc = work.pop()
            op, n = h.insts[pc]
            depth = d[pc]
            if op == 0x02:
                callee = funcs_by_addr[h.u32(pc + 1)]
                out = depth - callee.nargs
            elif op == 0x03:
                out = depth - h.syscalls[h.u16(pc + 1)][1]
            else:
                pops, pushes = STACK_EFFECT.get(op, (2, 1) if 0x1a <= op <= 0x27 else (0, 0))
                out = depth - pops + pushes
            if out < 0:
                raise SystemExit(f'stack underflow at {pc}')
            succ = []
            if op == 0x06:
                succ = [h.u32(pc + 1)]
                self.leaders.add(succ[0])
            elif op == 0x07:
                succ = [h.u32(pc + 1), pc + n]
                self.leaders.update(succ)
            elif op in (0x04, 0x05):
                pass
            else:
                succ = [pc + n]
            for s in succ:
                if not (self.start <= s < self.end):
                    raise SystemExit(f'jump out of function at {pc}')
                if s in d:
                    if d[s] != out:
                        raise SystemExit(f'inconsistent stack depth at {s}')
                else:
                    d[s] = out
                    work.append(s)
            if op in (0x04, 0x05, 0x06, 0x07) and pc + n < self.end:
                self.leaders.add(pc + n)
        self.depth = d
        self.pcs = sorted(d)


STACK_EFFECT = {
    0x08: (0, 1), 0x09: (0, 1), 0x0a: (0, 1), 0x0b: (0, 1), 0x0c: (0, 1), 0x0d: (0, 1),
    0x0e: (0, 1), 0x0f: (0, 1), 0x10: (0, 1), 0x11: (1, 1), 0x12: (1, 1), 0x13: (0, 1),
    0x14: (0, 1), 0x15: (1, 0), 0x16: (1, 0), 0x17: (2, 0), 0x18: (2, 0), 0x19: (1, 1),
    0x07: (1, 0), 0x05: (1, 0),
}

# Pseudo-instructions of inlined code (inline_program): set the return value (pops it), clear it.
OP_SETRET, OP_CLRRET = 0x28, 0x29
STACK_EFFECT[OP_SETRET] = (1, 0)


# ---------------------------------------------------------------------------------------------
# Inlining

INLINE_BYTES = 48
# A recorded function runs all the time when it was called at least this often.
HOT_CALLS = 100
# In the recorded run (7800 frames, after the start), a segment that ran at least this often
# runs every frame or so, and one that ran at least LINE_SEGMENT_RUNS times, with each line of
# text or so.
FRAME_SEGMENT_RUNS = 3000
LINE_SEGMENT_RUNS = 30
INLINE_DEPTH = 2


def read_profile(path):
    """A recorded run (see `layout`): script address -> call count, in first-call order."""
    out = {}
    if path and os.path.exists(path):
        for line in open(path):
            parts = line.split()
            if parts:
                out[int(parts[0])] = int(parts[1]) if len(parts) > 1 else 0
    return out


def hot_functions(profile):
    """Script addresses of the functions that run all the time (at least HOT_CALLS times in the
    recorded run): they get inlining and native nested calls; the rest, mostly run once, is
    compiled small (a code page read from the disc costs more than its speed saves)."""
    return {a for a, c in profile.items() if c >= HOT_CALLS}


def inline_program(h, funcs, by_addr, hot):
    """A copy of the script with calls to small functions replaced by their code.

    The callee's arguments and locals become locals of the caller, its returns set the return
    value and jump past the call. Only callees whose returns leave nothing on their stack, that
    do not call themselves and that fit INLINE_BYTES are inlined (down to INLINE_DEPTH)."""
    def inlinable(g):
        if g.end - g.start - 3 > INLINE_BYTES:
            return False
        for pc in g.pcs:
            op, _ = h.insts[pc]
            if op == 0x04 and g.depth[pc] != 0 or op == 0x05 and g.depth[pc] != 1:
                return False
            if op == 0x02 and h.u32(pc + 1) == g.start:
                return False
        return True
    can = {g.start: inlinable(g) for g in funcs}

    all_pcs = sorted(h.insts)
    origin = {}          # new address -> the script's
    code = bytearray(b'\0' * 4)
    fixups = []          # (offset in code, label) of jump and call operands
    labels = {}
    func_map = {}
    str_addr = {}
    ctx_counter = [0]

    def emit(raw):
        code.extend(raw)

    def body(g, ctx, slot_of, end_label, depth, top):
        for pc in all_pcs[bisect.bisect_left(all_pcs, g.start):bisect.bisect_left(all_pcs, g.end)]:
            op, n = h.insts[pc]
            labels[(ctx, pc)] = len(code)
            origin[len(code)] = h.origin.get(pc, pc)
            raw = bytearray(h.data[pc:pc + n])
            if op == 0x01:
                if ctx == 0:
                    top['init_at'] = len(code)
                    emit(raw)
                continue
            if op == 0x02:
                callee = by_addr[h.u32(pc + 1)]
                if (depth < INLINE_DEPTH and can[callee.start] and callee.start not in top['chain']
                        and top['nlocals'] + callee.nargs + callee.nlocals <= 127):
                    base = top['nlocals']
                    top['nlocals'] += callee.nargs + callee.nlocals
                    for i in reversed(range(callee.nargs)):
                        emit(bytes([0x16, (base + i) & 0xff]))
                    for j in range(callee.nlocals):
                        emit(bytes([0x08, 0x16, (base + callee.nargs + j) & 0xff]))
                    ctx_counter[0] += 1
                    sub = ctx_counter[0]
                    end = ('end', sub)

                    def remap(o, callee=callee, base=base):
                        return base + (o + callee.nargs + 1 if o < -1 else callee.nargs + o)
                    top['chain'].add(callee.start)
                    body(callee, sub, remap, end, depth + 1, top)
                    top['chain'].discard(callee.start)
                    labels[end] = len(code)
                    continue
                emit(raw[:1])
                fixups.append((len(code), ('func', callee.start)))
                emit(b'\0\0\0\0')
                continue
            if op in (0x06, 0x07):
                emit(raw[:1])
                fixups.append((len(code), (ctx, h.u32(pc + 1))))
                emit(b'\0\0\0\0')
                continue
            if op == 0x0e:
                str_addr[len(code)] = h.str_addr.get(pc, pc + 2)
                emit(raw)
                continue
            if slot_of is not None and op in (0x10, 0x12, 0x16, 0x18):
                o = struct.unpack_from('<b', raw, 1)[0]
                emit(bytes([op, slot_of(o) & 0xff]))
                continue
            if slot_of is not None and op in (0x04, 0x05):
                emit(bytes([OP_CLRRET if op == 0x04 else OP_SETRET, 0x06]))
                fixups.append((len(code), end_label))
                emit(b'\0\0\0\0')
                continue
            emit(raw)

    for f in funcs:
        func_map[f.start] = len(code)
        # The function's own locals keep their offsets; inlined ones go after them.
        top = {'nlocals': f.nlocals, 'chain': {f.start}, 'init_at': None}
        body(f, 0, None, None, 0 if f.start in hot else INLINE_DEPTH, top)
        # Its locals now include the inlined functions' arguments and locals.
        code[top['init_at'] + 2] = top['nlocals'] & 0xff
    for at, label in fixups:
        if label[0] == 'func':
            target = func_map[label[1]]
        else:
            target = labels[label]
        struct.pack_into('<I', code, at, target)
    new = Hcb.__new__(Hcb)
    new.__dict__.update(h.__dict__)
    new.data = bytes(code)
    new.code_end = len(code)
    new.entry = func_map[h.entry]
    new.orig_func = {v: k for k, v in func_map.items()}
    new.func_new = func_map
    new.str_addr = str_addr
    new.origin = origin
    new.decode()
    return new


# Rough size of an instruction in compiled code (bytes), to cut functions into segments.
def native_estimate(op, argc=0):
    if op == 0x02:
        return 36 + 8 * argc
    if op == 0x03:
        return 28 + 10 * argc
    if op in (0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x01, 0x00):
        return 0
    return 12


# ---------------------------------------------------------------------------------------------
# Code generation

class Gen:
    """Generates the segments of one function."""

    def __init__(self, comp, func):
        self.c = comp
        self.f = func
        self.h = comp.hcb
        self.out = []
        self.tmp = 0

    # -- helpers ------------------------------------------------------------------------------

    def emit(self, line):
        self.out.append('    ' + line)

    def new_var(self, init, kind='var', tag=None, mask=None):
        self.tmp += 1
        name = f'v{self.tmp}'
        if kind == 'cond':
            self.emit(f'int {name} = {init};')
        else:
            self.emit(f'V {name} = {init};')
        return Val(kind, tag, expr=name, mask=mask)

    def v(self, val):
        """A value as a C expression of type V."""
        if val.kind == 'const':
            if val.tag == T_NIL:
                return '0'
            return f'AOT_MK({val.tag}, {c_int(val.num)})'
        if val.kind == 'cond':
            return f'AOT_BOOL({val.expr})'
        if val.kind == 'int':
            return f'AOT_MK(AOT_INT, {val.expr})'
        return val.expr

    def i(self, val):
        """A known-int value as a C int expression."""
        if val.kind == 'const':
            return c_int(val.num)
        if val.kind == 'int':
            return val.expr
        return f'(int32_t) AOT_VAL({val.expr})'

    def truth(self, val):
        """A value's truth (not Nil) as a C condition, or a Python bool when it is known."""
        if val.kind == 'const':
            return val.tag != T_NIL
        if val.kind == 'cond':
            return val.expr
        if not val.mask & M_NIL:
            return True
        if val.mask == M_NIL:
            return False
        return f'AOT_TAG({val.expr})'

    def materialize(self, val):
        """Keep a non-constant value in its own variable, so that later stores do not change it."""
        if val.kind == 'const':
            return val
        if val.kind == 'var' and val.expr.startswith('v'):
            return val
        if val.kind == 'cond':
            return self.new_var(val.expr, 'cond', T_TRUE)
        if val.kind == 'int':
            self.tmp += 1
            name = f'v{self.tmp}'
            self.emit(f'int32_t {name} = {val.expr};')
            return Val('int', T_INT, expr=name)
        return self.new_var(val.expr, 'var', val.tag, val.vmask)

    # -- operations ---------------------------------------------------------------------------

    def binop(self, op, a, b):
        name = BINOPS[op - 0x1a]
        # Constant folding (ints, nil and true only; floats and strings are left to the runtime).
        if a.kind == 'const' and b.kind == 'const':
            r = fold(name, a, b)
            if r is not None:
                return r
        if name in ('and', 'or'):
            ta, tb = self.truth(a), self.truth(b)
            if isinstance(ta, bool) or isinstance(tb, bool):
                if name == 'and':
                    if ta is False or tb is False:
                        return NIL
                    other = tb if ta is True else ta
                else:
                    if ta is True or tb is True:
                        return TRUE
                    other = tb if ta is False else ta
                if other is True or other is False:
                    return TRUE if other else NIL
                return Val('cond', T_TRUE, expr=f'({other} != 0)')
            op_c = '&&' if name == 'and' else '||'
            return Val('cond', T_TRUE, expr=f'(({ta}) {op_c} ({tb}))')
        if a.known_int() and b.known_int():
            x, y = self.i(a), self.i(b)
            if name in ('add', 'sub', 'mul'):
                sym = {'add': '+', 'sub': '-', 'mul': '*'}[name]
                return Val('int', T_INT, expr=f'(int32_t) ((uint32_t) {x} {sym} (uint32_t) {y})')
            if name in ('div', 'mod') and b.kind == 'const' and s32(b.num) not in (0, -1):
                sym = '/' if name == 'div' else '%'
                return Val('int', T_INT, expr=f'({x} {sym} {c_int(b.num)})')
            if name in ('eq', 'ne', 'gt', 'ge', 'lt', 'le'):
                # SetLE computes >= and SetGE computes <= in the VM (BINOPS names what they do).
                sym = {'eq': '==', 'ne': '!=', 'gt': '>', 'ge': '>=', 'lt': '<', 'le': '<='}[name]
                return Val('cond', T_TRUE, expr=f'({x} {sym} {y})')
            if name == 'bittest' and b.kind == 'const' and 0 <= s32(b.num) < 32:
                return Val('cond', T_TRUE, expr=f'((((uint32_t) {x}) >> {s32(b.num)}) & 1)')
        # A constant that the other side can never equal (the VM's equal() never mixes types).
        if name in ('eq', 'ne'):
            k, other = (b, a) if b.kind == 'const' else (a, b) if a.kind == 'const' else (None, None)
            if k is not None and k.tag in (T_NIL, T_TRUE, T_INT) and not other.mask & k.mask:
                return NIL if name == 'eq' else TRUE
        # Nil, true and ints only: equal values are equal words, and the rest is a check of the
        # tags (comparisons of anything but two numbers are Nil, so >= and <= are True).
        if a.mask & ~M_SIMPLE == 0 and b.mask & ~M_SIMPLE == 0:
            r = self.simple_binop(name, a, b)
            if r is not None:
                return r
        # Equality with a constant int, nil or true: equal only to the same tag and value
        # (the VM's equal() never mixes types).
        if name in ('eq', 'ne'):
            k, other = (b, a) if b.kind == 'const' else (a, b) if a.kind == 'const' else (None, None)
            if k is not None and k.tag in (T_NIL, T_TRUE, T_INT) and other.kind in ('var',):
                e = other.expr
                if k.tag == T_INT:
                    cond = f'({e} == AOT_MK(AOT_INT, {c_int(k.num)}))'
                else:
                    cond = f'(AOT_TAG({e}) == {k.tag})'
                if name == 'ne':
                    cond = f'!{cond}'
                return Val('cond', T_TRUE, expr=cond)
        helper_cmp = f'aot_cmp(AOT_OP_{name.upper()}, {self.v(a)}, {self.v(b)})'
        helper_arith = f'aot_arith(AOT_OP_{name.upper()}, {self.v(a)}, {self.v(b)})'
        # Types not known statically, but ints possible: the int case inline, behind a check of
        # the tags; the runtime only for the rest (floats, strings, tables).
        if a.mask & M_INT and b.mask & M_INT and all(v.kind != 'cond' for v in (a, b)):
            guard = self.both_int(a, b)
            xi = self.i(a) if a.known_int() else f'(int32_t) AOT_VAL({self.v(a)})'
            yi = self.i(b) if b.known_int() else f'(int32_t) AOT_VAL({self.v(b)})'
            if name in ('eq', 'ne'):
                simple = ' && '.join(f'AOT_TAG({v.expr}) <= AOT_INT' for v in (a, b) if v.kind != 'const')
                sym = '==' if name == 'eq' else '!='
                return Val('cond', T_TRUE, expr=f'(({simple}) ? ({self.v(a)} {sym} {self.v(b)}) : {helper_cmp})')
            if name in ('gt', 'ge', 'lt', 'le'):
                sym = {'gt': '>', 'ge': '>=', 'lt': '<', 'le': '<='}[name]
                return Val('cond', T_TRUE, expr=f'(({guard}) ? ({xi} {sym} {yi}) : {helper_cmp})')
            if name in ('add', 'sub', 'mul'):
                sym = {'add': '+', 'sub': '-', 'mul': '*'}[name]
                r = f'AOT_MK(AOT_INT, (uint32_t) {xi} {sym} (uint32_t) {yi})'
                return Val('var', None, expr=f'(({guard}) ? {r} : {helper_arith})')
        if name in ('eq', 'ne', 'gt', 'ge', 'lt', 'le', 'bittest'):
            return Val('cond', T_TRUE, expr=helper_cmp)
        return Val('var', None, expr=helper_arith)

    def both_int(self, a, b):
        """A C condition that both values are ints (only the unknown ones are checked)."""
        checks = [f'AOT_TAG({x.expr}) == AOT_INT' for x in (a, b) if not x.known_int()]
        return ' && '.join(checks) if checks else None

    def simple_binop(self, name, a, b):
        x, y = self.v(a), self.v(b)
        if name not in ('eq', 'ne') and any(v.kind == 'const' and v.tag != T_INT for v in (a, b)):
            # Nil or true against anything: no number comparison or arithmetic applies.
            return TRUE if name in ('ge', 'le') else NIL
        if name in ('eq', 'ne'):
            return Val('cond', T_TRUE, expr=f'({x} {"==" if name == "eq" else "!="} {y})')
        guard = self.both_int(a, b)
        xi = self.i(a) if a.known_int() else f'(int32_t) AOT_VAL({x})'
        yi = self.i(b) if b.known_int() else f'(int32_t) AOT_VAL({y})'
        if name in ('gt', 'ge', 'lt', 'le'):
            sym = {'gt': '>', 'ge': '>=', 'lt': '<', 'le': '<='}[name]
            other = '1' if name in ('ge', 'le') else '0'
            return Val('cond', T_TRUE, expr=f'(({guard}) ? ({xi} {sym} {yi}) : {other})')
        if name in ('add', 'sub', 'mul'):
            sym = {'add': '+', 'sub': '-', 'mul': '*'}[name]
            r = f'AOT_MK(AOT_INT, (uint32_t) {xi} {sym} (uint32_t) {yi})'
            return Val('var', None, expr=f'(({guard}) ? {r} : 0)', mask=M_INT | M_NIL)
        if name == 'bittest':
            return Val('cond', T_TRUE,
                       expr=f'(({guard}) && (uint32_t) {yi} < 32 && (((uint32_t) {xi} >> {yi}) & 1))')
        return None

    def neg(self, a):
        if a.kind == 'const':
            if a.tag == T_INT:
                return Val.const(T_INT, -s32(a.num))
            if a.tag != T_FLOAT:
                return a
        if a.known_int():
            return Val('int', T_INT, expr=f'(int32_t) (0u - (uint32_t) {self.i(a)})')
        if a.kind == 'cond' or not a.mask & (M_INT | M_FLOAT):
            return a
        return Val('var', None, expr=f'aot_neg({self.v(a)})', mask=a.mask)


def fold(name, a, b):
    ta, tb = a.tag, b.tag
    if name == 'and':
        return TRUE if ta != T_NIL and tb != T_NIL else NIL
    if name == 'or':
        return TRUE if ta != T_NIL or tb != T_NIL else NIL
    if ta == T_INT and tb == T_INT:
        x, y = s32(a.num), s32(b.num)
        if name == 'add': return Val.const(T_INT, x + y)
        if name == 'sub': return Val.const(T_INT, x - y)
        if name == 'mul': return Val.const(T_INT, x * y)
        if name in ('div', 'mod'):
            if y == 0 or (x == -0x80000000 and y == -1):
                return NIL
            q = abs(x) // abs(y) * (1 if (x < 0) == (y < 0) else -1)
            return Val.const(T_INT, q if name == 'div' else x - q * y)
        if name == 'bittest':
            return TRUE if 0 <= y < 32 and (x >> y) & 1 else NIL
        cmp = {'eq': x == y, 'ne': x != y, 'gt': x > y, 'ge': x >= y, 'lt': x < y, 'le': x <= y}
        return TRUE if cmp[name] else NIL
    if ta in (T_NIL, T_TRUE, T_INT) and tb in (T_NIL, T_TRUE, T_INT):
        if name in ('eq', 'ne'):
            same = ta == tb and (ta != T_INT or a.num == b.num)
            return TRUE if same == (name == 'eq') else NIL
        if name in ('add', 'sub', 'mul', 'div', 'mod', 'bittest', 'gt', 'lt'):
            return NIL
        if name == 'ge':  # not less
            return TRUE
        if name == 'le':  # not greater
            return TRUE
    return None


# ---------------------------------------------------------------------------------------------
# Type inference

def arith_mask(name, a, b):
    """The tags `name` (an arithmetic operation of BINOPS) can give for operands of tags a, b,
    as the VM's vm_add, vm_sub... do."""
    nums = (T_INT, T_FLOAT)
    strs = (T_CSTR, T_STR)
    r = 0
    for ta in range(7):
        if not a >> ta & 1:
            continue
        for tb in range(7):
            if not b >> tb & 1:
                continue
            if ta in nums and tb in nums:
                if ta == tb == T_INT:
                    r |= M_INT | (M_NIL if name in ('div', 'mod') else 0)
                elif name == 'mod':
                    r |= M_NIL
                else:
                    r |= M_FLOAT | (M_NIL if name == 'div' else 0)
            elif name == 'add' and ta in strs and tb in strs:
                r |= M_STR
            else:
                r |= M_NIL
    return r


class Types:
    """What each value can be, for the whole script: each function's argument and return value
    tags and each global's, to a fixed point; then, per instruction, the tags of the value it
    pushes and of the stack at block entries.

    Local variables start Nil and are tracked along the control flow; table entries and syscall
    results can be anything."""

    def __init__(self, comp):
        self.c = comp
        h = comp.hcb
        self.globals = [M_NIL] * (h.nv_globals + h.v_globals)
        self.args = {f.start: [0] * f.nargs for f in comp.funcs}
        self.rets = {f.start: 0 for f in comp.funcs}
        for addr in comp.thread_entries():
            self.args[addr] = [M_NIL] * len(self.args[addr])
        self.pushed = {}
        self.stack_at = {}
        changed = True
        passes = 0
        while changed:
            passes += 1
            self.changed = False
            for f in comp.funcs:
                self.function(f, record=False)
            changed = self.changed
        for f in comp.funcs:
            self.function(f, record=True)
        self.passes = passes

    def widen(self, table, key, mask):
        if table[key] | mask != table[key]:
            table[key] |= mask
            self.changed = True

    def function(self, f, record):
        h = self.c.hcb
        nslots = f.nargs + f.nlocals
        # The return value is the thread's: on entry, whatever the caller last got.
        entry = (tuple(), tuple(self.args[f.start] + [M_NIL] * f.nlocals), M_ANY)
        states = {f.start: entry}
        work = [f.start]
        while work:
            pc = work.pop()
            stack, slots, ret = states[pc]
            stack = list(stack)
            slots = list(slots)
            op, n = h.insts[pc]
            a = pc + 1
            nxt = pc + n
            succ = [nxt]
            pushed = None
            if op in (0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e):
                pushed = {0x08: M_NIL, 0x09: M_TRUE, 0x0d: M_FLOAT, 0x0e: M_CSTR}.get(op, M_INT)
            elif op == 0x0f:
                pushed = self.globals[h.u16(a)]
            elif op == 0x10:
                pushed = slots[f.slot(h.i8(a))]
            elif op in (0x11, 0x12):
                stack.pop()
                pushed = M_ANY
            elif op == 0x14:
                pushed = ret or M_ANY
                ret = M_NIL
            elif op == 0x15:
                self.widen(self.globals, h.u16(a), stack.pop())
            elif op == 0x16:
                slots[f.slot(h.i8(a))] = stack.pop()
            elif op == 0x17:
                stack.pop()
                stack.pop()
                self.widen(self.globals, h.u16(a), M_TABLE | M_NIL)
            elif op == 0x18:
                stack.pop()
                key = stack.pop()
                k = f.slot(h.i8(a))
                slots[k] = (M_TABLE if key & M_INT else 0) | (M_NIL if key & ~M_INT else 0)
            elif op == 0x19:
                pushed = stack.pop()
            elif op == OP_SETRET:
                ret = stack.pop()
            elif op == OP_CLRRET:
                ret = M_NIL
            elif 0x1a <= op <= 0x27:
                y = stack.pop()
                x = stack.pop()
                name = BINOPS[op - 0x1a]
                pushed = arith_mask(name, x, y) if name in ('add', 'sub', 'mul', 'div', 'mod') else M_BOOL
            elif op == 0x02:
                callee = self.c.by_addr[h.u32(a)]
                if callee.nargs:
                    args = stack[-callee.nargs:]
                    del stack[-callee.nargs:]
                    for i, m in enumerate(args):
                        self.widen(self.args[callee.start], i, m)
                ret = self.rets[callee.start] or M_ANY
            elif op == 0x03:
                argc = h.syscalls[h.u16(a)][1]
                if argc:
                    del stack[-argc:]
                ret = M_ANY
            elif op in (0x04, 0x05):
                self.widen(self.rets, f.start, stack.pop() if op == 0x05 else M_NIL)
                succ = []
            elif op == 0x06:
                succ = [h.u32(a)]
            elif op == 0x07:
                stack.pop()
                succ = [h.u32(a), nxt]
            if pushed is not None:
                stack.append(pushed or M_ANY)
                if record:
                    self.pushed[pc] = self.pushed.get(pc, 0) | (pushed or M_ANY)
            new = (tuple(stack), tuple(slots), ret)
            for t in succ:
                if t not in f.depth:
                    continue
                old = states.get(t)
                if old is None:
                    states[t] = new
                    work.append(t)
                    continue
                merged = (tuple(x | y for x, y in zip(old[0], new[0])),
                          tuple(x | y for x, y in zip(old[1], new[1])), old[2] | new[2])
                if merged != old:
                    states[t] = merged
                    work.append(t)
        if record:
            for pc, st in states.items():
                self.stack_at[pc] = st[0]


class Compiler:
    def __init__(self, hcb, seg_limit):
        self.hcb = hcb
        self.seg_limit = seg_limit
        h = hcb
        ends = h.funcs[1:] + [h.code_end]
        self.funcs = [Func(h, i, s, e) for i, (s, e) in enumerate(zip(h.funcs, ends))]
        self.by_addr = {f.start: f for f in self.funcs}
        for f in self.funcs:
            f.analyze(self.by_addr)

    def split(self, f):
        """Cut a function into segments at depth-0 instructions, about `seg_limit` bytes each."""
        h = self.hcb
        cuts = [f.start]
        size = 0
        for pc in f.pcs:
            op, _ = h.insts[pc]
            if size >= self.seg_limit and f.depth[pc] == 0 and pc != cuts[-1]:
                cuts.append(pc)
                size = 0
            argc = 0
            if op == 0x02:
                argc = self.by_addr[h.u32(pc + 1)].nargs
            elif op == 0x03:
                argc = h.syscalls[h.u16(pc + 1)][1]
            size += native_estimate(op, argc)
        return cuts

    def thread_entries(self):
        """Functions ThreadStart can be given: every int constant naming a function (as the
        script's address; after inlining, functions have new ones)."""
        h = self.hcb
        new = h.func_new if h.orig_func else {a: a for a in self.by_addr}
        out = {h.entry}
        for pc, (op, _) in h.insts.items():
            if op == 0x0a and h.u32(pc + 1) in new:
                out.add(new[h.u32(pc + 1)])
        return out

    def script_addr(self, addr):
        h = self.hcb
        return h.orig_func[addr] if h.orig_func else addr




class FuncGen(Gen):
    """Generates the segments of one function."""

    def __init__(self, comp, func):
        super().__init__(comp, func)
        self.hot = func.start in getattr(comp, 'hot', ())
        self.nslots = func.nargs + func.nlocals
        self.spill_base = self.nslots
        self.ret_live = self.ret_liveness()
        self.seg_entries = self.cross_entries()

    def seg_index(self, pc):
        k = 0
        for i, c in enumerate(self.f.cuts):
            if c <= pc:
                k = i
        return k

    def cross_entries(self):
        """For each segment, the blocks other segments jump to: entry index 0 is the segment's
        start, the others follow."""
        f, h = self.f, self.h
        entries = [[c] for c in f.cuts]
        for pc in f.pcs:
            op, n = h.insts[pc]
            targets = []
            if op in (0x06, 0x07):
                targets.append(h.u32(pc + 1))
            if op not in (0x04, 0x05, 0x06) and pc + n in f.depth:
                targets.append(pc + n)
            for t in targets:
                ks, kt = self.seg_index(pc), self.seg_index(t)
                if ks != kt and t not in entries[kt]:
                    entries[kt].append(t)
        return entries

    def ret_liveness(self):
        """Whether the return value can be read (PushReturn) before it is next written, at the
        start of each instruction."""
        f, h = self.f, self.h
        live = {}
        changed = True
        while changed:
            changed = False
            for pc in reversed(f.pcs):
                op, n = h.insts[pc]
                if op == 0x14:
                    val = True
                elif op in (0x02, 0x03, 0x04, 0x05, OP_CLRRET):
                    val = False
                elif op == OP_SETRET:
                    val = False
                else:
                    if op == 0x06:
                        succ = [h.u32(pc + 1)]
                    elif op == 0x07:
                        succ = [h.u32(pc + 1), pc + n]
                    else:
                        succ = [pc + n]
                    val = any(live.get(s, False) for s in succ if s in f.depth)
                if live.get(pc) != val:
                    live[pc] = val
                    changed = True
        return live

    def generate(self):
        f = self.f
        results = []
        cuts = f.cuts + [f.end]
        for k, start in enumerate(f.cuts):
            sid = f.seg_ids[k]
            self.tmp = 0
            self.max_d = 0
            self.cur_k = k
            self.cur_range = (start, cuts[k + 1])
            self.strings = set()
            body = self.gen_segment(start, cuts[k + 1])
            head = [f'/* Function {f.start}, from {start}. */',
                    f'int aot_s{sid}(Frame *f)', '{']
            if self.max_d:
                head.append('    V ' + ', '.join(f'd{i}' for i in range(self.max_d)) + ';')
            if len(self.labels) > 1:
                # Every place the segment is entered at: its start, the blocks other segments
                # go to, and the resume points after calls and yields.
                tbl = ', '.join(f'&&{lab} - &&L0' for lab in self.labels)
                head.append(f'    static const int16_t at[] __attribute__((section(".rodata.aot_s{sid}"))) = {{ {tbl} }};')
                head.append('    goto *(void *) ((char *) &&L0 + at[f->state]);')
            results.append((sid, '\n'.join(head + body + ['}', '']), sorted(self.strings)))
        return results

    # -- stack values across calls and yields --------------------------------------------------

    def spill(self, stack):
        """Store the non-constant stack values in the frame; returns the stack with None for them."""
        new = []
        for i, val in enumerate(stack):
            if val.kind == 'const':
                new.append(val)
                continue
            slot = self.spill_base + i
            self.nslots = max(self.nslots, slot + 1)
            self.emit(f'f->s[{slot}] = {self.v(val)};')
            new.append(('spilled', val.mask))
        return new

    def unspill(self, spilled):
        return [self.new_var(f'f->s[{self.spill_base + i}]', mask=val[1]) if isinstance(val, tuple) else val
                for i, val in enumerate(spilled)]

    # -- control flow ---------------------------------------------------------------------------

    def set_d(self, stack, pre=''):
        """Put the stack into d0..dn (the variables blocks are entered with)."""
        moves = [(i, val) for i, val in enumerate(stack)
                 if not (val.kind == 'var' and val.expr == f'd{i}')]
        self.max_d = max(self.max_d, len(stack))
        # Values that read d variables are copied first, so the moves do not clobber them.
        reads_d = any(val.kind != 'const' and 'd' in val.expr for _, val in moves)
        if reads_d and len(moves) > 1:
            tmps = []
            for i, val in moves:
                self.tmp += 1
                self.emit(f'{pre}V t{self.tmp} = {self.v(val)};')
                tmps.append((i, f't{self.tmp}'))
            for i, t in tmps:
                self.emit(f'{pre}d{i} = {t};')
        else:
            for i, val in moves:
                self.emit(f'{pre}d{i} = {self.v(val)};')

    def flow_to(self, tgt, stack, ret, pre=''):
        """Go to `tgt` with `stack` (its depth): in this segment, or through the driver."""
        f = self.f
        if isinstance(ret, Val) and self.ret_live.get(tgt, False):
            self.emit(f'{pre}aot_ret = {self.v(ret)};')
        start, end = self.cur_range
        if start < tgt < end:
            self.set_d(stack, pre)
            self.emit(f'{pre}goto B{tgt};')
            return
        if tgt >= f.end:
            raise SystemExit(f'function {f.start} runs past its end')
        k = self.seg_index(tgt)
        idx = self.seg_entries[k].index(tgt)
        for i, val in enumerate(stack):
            slot = self.spill_base + i
            self.nslots = max(self.nslots, slot + 1)
            self.emit(f'{pre}f->s[{slot}] = {self.v(val)};')
        self.emit(f'{pre}return aot_goto(f, AOT_GOTO({f.seg_ids[k]}, {idx}));')

    # -- the segment ----------------------------------------------------------------------------

    def gen_segment(self, start, end):
        f, h = self.f, self.h
        self.out = []
        self.emit_label('L0')
        entries = set(self.seg_entries[self.cur_k][1:])
        self.labels = ['L0'] + [f'E{pc}' for pc in self.seg_entries[self.cur_k][1:]]
        stack = []
        ret = 'mem'          # the return value: 'mem' (aot_ret) or a Val
        reachable = True
        for pc in (p for p in f.pcs if start <= p < end):
            op, n = h.insts[pc]
            depth = f.depth[pc]
            if pc in f.leaders and pc != start:
                if reachable:
                    self.flow_to(pc, stack, ret)
                if pc in entries:
                    # Entered from another segment: the stack comes in the frame.
                    self.emit_label(f'E{pc}')
                    self.max_d = max(self.max_d, depth)
                    for i in range(depth):
                        self.emit(f'd{i} = f->s[{self.spill_base + i}];')
                        self.nslots = max(self.nslots, self.spill_base + i + 1)
                self.emit_label(f'B{pc}')
                self.max_d = max(self.max_d, depth)
                masks = self.c.types.stack_at.get(pc, ())
                stack = [Val('var', None, expr=f'd{i}', mask=masks[i] if i < len(masks) else None)
                         for i in range(depth)]
                ret = 'mem'
                reachable = True
            elif pc == start and depth:
                # A segment starts at depth 0 (see split()), except a function's own start.
                raise SystemExit(f'segment at {pc} starts with values on the stack')
            if not reachable:
                continue
            stack, ret, reachable = self.gen_inst(pc, op, n, stack, ret)
        if reachable:
            self.flow_to(end, stack, ret)
        return self.out

    def emit_label(self, lab):
        self.out.append(f'{lab}:;')

    def gen_inst(self, pc, op, n, stack, ret):
        f, h = self.f, self.h
        a = pc + 1
        nxt = pc + n
        mask = self.c.types.pushed.get(pc)
        if op in (0x00, 0x01):
            pass
        elif op == 0x08:
            stack.append(NIL)
        elif op == 0x09:
            stack.append(TRUE)
        elif op == 0x0a:
            stack.append(Val.const(T_INT, h.i32(a)))
        elif op == 0x0b:
            stack.append(Val.const(T_INT, h.i16(a)))
        elif op == 0x0c:
            stack.append(Val.const(T_INT, h.i8(a)))
        elif op == 0x0d:
            stack.append(Val.const(T_FLOAT, h.u32(a)))
        elif op == 0x0e:
            # The string's address in the script (its length is the byte before). Short ones
            # also go with the segment (SHORT_STRING), so that using them reads nothing more.
            addr = h.str_addr.get(pc, pc + 2)
            stack.append(Val.const(T_CSTR, addr))
            if h.data[pc + 1] <= SHORT_STRING:
                self.strings.add(addr)
        elif op == 0x0f:
            stack.append(self.new_var(f'aot_g[{h.u16(a)}]', mask=mask))
        elif op == 0x10:
            stack.append(self.new_var(f'f->s[{f.slot(h.i8(a))}]', mask=mask))
        elif op in (0x11, 0x12):
            key = stack.pop()
            where = f'aot_g[{h.u16(a)}]' if op == 0x11 else f'f->s[{f.slot(h.i8(a))}]'
            stack.append(self.new_var(f'aot_table_get({where}, {self.v(key)})', mask=mask))
        elif op == 0x13:
            raise SystemExit(f'PushTop at {pc} is not supported')
        elif op == OP_SETRET:
            ret = stack.pop()
        elif op == OP_CLRRET:
            ret = NIL
        elif op == 0x14:
            if ret == 'mem':
                stack.append(self.new_var('aot_ret', mask=mask))
                if self.ret_live.get(nxt, False):
                    self.emit('aot_ret = 0;')
                    ret = 'mem'
                else:
                    ret = NIL
            else:
                stack.append(ret)
                ret = NIL
        elif op in (0x15, 0x16):
            val = stack.pop()
            where = f'aot_g[{h.u16(a)}]' if op == 0x15 else f'f->s[{f.slot(h.i8(a))}]'
            self.emit(f'{where} = {self.v(val)};')
        elif op in (0x17, 0x18):
            val = stack.pop()
            key = stack.pop()
            where = f'aot_g[{h.u16(a)}]' if op == 0x17 else f'f->s[{f.slot(h.i8(a))}]'
            self.emit(f'aot_table_set(&{where}, {self.v(key)}, {self.v(val)});')
        elif op == 0x19:
            r = self.neg(stack.pop())
            if r.kind == 'var' and mask:
                r.vmask = r.mask & mask
            if r.kind == 'var' and 'aot_' in r.expr:
                r = self.materialize(r)
            stack.append(r)
        elif 0x1a <= op <= 0x27:
            b = stack.pop()
            x = stack.pop()
            r = self.binop(op, x, b)
            if r.kind == 'var' and mask:
                r.vmask = r.mask & mask
            if r.kind != 'const' and 'aot_' in r.expr:
                r = self.materialize(r)
            stack.append(r)
        elif op == 0x02:
            callee = self.c.by_addr[h.u32(a)]
            k = len(stack) - callee.nargs
            args, stack = stack[k:], stack[:k]
            spilled = self.spill(stack)
            self.emit('{')
            self.labels.append(f'R{nxt}')
            self.emit(f'    Frame *c = aot_call(f, AOT_PACK({callee.seg_ids[0]}, NSLOTS_{callee.index}), {len(self.labels) - 1});')
            for i, val in enumerate(args):
                if not val.is_const(T_NIL):
                    self.emit(f'    c->s[{i}] = {self.v(val)};')
            if self.hot:
                # Run the callee here and go on when it returns; if it yields (or nesting is
                # too deep), back to the driver, which resumes this frame at the label.
                self.emit('    int r = aot_invoke(c);')
                self.emit('    if (r >= 0) return r;')
            else:
                # Smaller: the driver runs the callee, then this frame from the label.
                self.emit('    return 1;')
            self.emit('}')
            self.emit_label(f'R{nxt}')
            stack = self.unspill(spilled)
            ret = 'mem'
        elif op == 0x03:
            sid = h.u16(a)
            name, argc = h.syscalls[sid]
            k = len(stack) - argc
            args, stack = stack[k:], stack[:k]
            if args:
                self.tmp += 1
                arr = f'a{self.tmp}'
                # Stored one by one: an initializer would become a copy of shared constant data.
                self.emit(f'V {arr}[{argc}];')
                for i, val in enumerate(args):
                    self.emit(f'{arr}[{i}] = {self.v(val)};')
            else:
                arr = '0'
            if name in YIELDING_SYSCALLS:
                # The thread may stop here: what lives on is kept in the frame.
                spilled = self.spill(stack)
                if self.ret_live.get(nxt, False):
                    self.emit(f'aot_ret = aot_syscall({sid}, {arr});')
                else:
                    self.emit(f'aot_syscall({sid}, {arr});')
                self.labels.append(f'R{nxt}')
                self.emit(f'if (aot_yield) {{ f->state = {len(self.labels) - 1}; return 0; }}')
                self.emit_label(f'R{nxt}')
                stack = self.unspill(spilled)
                ret = 'mem'
            else:
                ret = self.new_var(f'aot_syscall({sid}, {arr})')
        elif op in (0x04, 0x05):
            if op == 0x05:
                self.emit(f'aot_ret = {self.v(stack.pop())};')
            else:
                self.emit('aot_ret = 0;')
            self.emit('return aot_return(f);')
            return stack, ret, False
        elif op == 0x06:
            self.flow_to(h.u32(a), stack, ret)
            return stack, ret, False
        elif op == 0x07:
            cond = stack.pop()
            t = self.truth(cond)
            if t is False:
                self.flow_to(h.u32(a), stack, ret)
                return stack, ret, False
            if t is not True:
                self.emit(f'if (!({t})) {{')
                self.flow_to(h.u32(a), list(stack), ret, pre='    ')
                self.emit('}')
        else:
            raise SystemExit(f'opcode {op:#x} at {pc}')
        return stack, ret, True


def layout(comp, order_path):
    """The functions in disc order, from a recorded run (`order_path`: script address and call
    count per line, in the order of first call): first those that ran often (every few frames:
    the code that runs all the time stays in the cache in few pages), then the others it used
    in the order they first ran (starting the game reads one run of pages), then the rest in
    script order."""
    if not order_path or not os.path.exists(order_path):
        return list(comp.funcs)
    new = comp.hcb.func_new if comp.hcb.orig_func else {}
    profile = []
    for line in open(order_path):
        parts = line.split()
        if not parts:
            continue
        f = comp.by_addr.get(new.get(int(parts[0]), int(parts[0])))
        if f is not None:
            profile.append((f, int(parts[1]) if len(parts) > 1 else 0))
    hot = [f for f, c in profile if c >= HOT_CALLS]
    used = [f for f, c in profile if c < HOT_CALLS]
    seen = set()
    out = []
    for f in hot + used + list(comp.funcs):
        if f.start not in seen:
            seen.add(f.start)
            out.append(f)
    return out


def read_counts(path):
    """Instruction runs from a recorded run: script address -> (count, order of first run)."""
    out = {}
    if path and os.path.exists(path):
        for line in open(path):
            parts = line.split()
            if len(parts) == 3:
                out[int(parts[0])] = (int(parts[1]), int(parts[2]))
    return out


def compile_all(comp, outdir, order_path=None, counts_path=None):
    h = comp.hcb
    comp.pc_counts = read_counts(counts_path)
    funcs = layout(comp, order_path)
    for f in funcs:
        f.cuts = comp.split(f)
        f.leaders.update(f.cuts)
        f.seg_ids = [None] * len(f.cuts)
    # Segment ids are the disc order: the segments that run all the time first (in few pages,
    # which the cache then keeps), then every other one in function order.
    counts = comp.pc_counts
    origin = comp.hcb.origin

    def recorded(f, k):
        """The segment's most runs of an instruction and when it first ran."""
        end = f.cuts[k + 1] if k + 1 < len(f.cuts) else f.end
        seen = [counts[o] for o in (origin.get(p, p) for p in f.pcs if f.cuts[k] <= p < end) if o in counts]
        return (max(n for n, _ in seen), min(t for _, t in seen)) if seen else (0, 0)
    every = [(f, k) for f in funcs for k in range(len(f.cuts))]
    runs = {(f.start, k): recorded(f, k) for f, k in every}
    # First the code that ran every frame (the cache keeps it), then what ran with each line of
    # text, then the rest of what the recorded run used, each in the order it first ran: code
    # that runs together is together in few pages (read whole) rather than spread over the
    # disc.
    def band(fk):
        n, first = runs[(fk[0].start, fk[1])]
        return (0 if n >= FRAME_SEGMENT_RUNS else 1 if n >= LINE_SEGMENT_RUNS else 2, first)
    hot = sorted((fk for fk in every if runs[(fk[0].start, fk[1])][0] > 0), key=band)
    hot_set = {(f.start, k) for f, k in hot}
    seg = 0
    for f, k in hot + [(f, k) for f, k in every if (f.start, k) not in hot_set]:
        f.seg_ids[k] = seg
        seg += 1
    comp.types = Types(comp)
    texts = [None] * seg
    seg_strings = [None] * seg
    for f in comp.funcs:
        gen = FuncGen(comp, f)
        for sid, text, strings in gen.generate():
            texts[sid] = text
            seg_strings[sid] = [[a, h.orig[a - 1:a - 1 + 1 + h.orig[a - 1]].hex()] for a in strings]
        f.nslots = gen.nslots
    os.makedirs(outdir, exist_ok=True)
    with open(os.path.join(outdir, 'aot_nslots.h'), 'w') as fp:
        fp.write('/* Generated by hcb_aot.py: the frame size of each script function. */\n')
        for f in comp.funcs:
            fp.write(f'#define NSLOTS_{f.index} {f.nslots}\n')
    per_file = 64
    files = []
    for k in range(0, seg, per_file):
        name = f'seg{k // per_file:04d}.c'
        files.append(name)
        with open(os.path.join(outdir, name), 'w') as fp:
            fp.write('/* Generated by hcb_aot.py from the game script. */\n')
            fp.write('#include "aot.h"\n#include "aot_nslots.h"\n\n')
            fp.write('\n'.join(texts[k:k + per_file]))
    threads = sorted(comp.thread_entries())
    index = {
        'segments': seg,
        'hot_segments': len(hot),
        'files': files,
        'entry': h.entry,
        'threads': [[comp.script_addr(a), comp.by_addr[a].seg_ids[0], comp.by_addr[a].nslots] for a in threads],
        'globals': h.nv_globals + h.v_globals,
        'syscalls': [[n, a] for n, a in h.syscalls],
        # Per segment: its short strings, as [address, hex of the length byte and the bytes].
        'strings': seg_strings,
    }
    with open(os.path.join(outdir, 'aot_index.json'), 'w') as fp:
        json.dump(index, fp)
    return index


def main():
    if len(sys.argv) not in (3, 4, 5):
        raise SystemExit(__doc__)
    hcb = Hcb(sys.argv[1])
    seg_bytes = int(os.environ.get('AOT_SEGMENT_BYTES', '2560'))
    order = sys.argv[3] if len(sys.argv) >= 4 else None
    instruction_counts = sys.argv[4] if len(sys.argv) >= 5 else None
    hot = hot_functions(read_profile(order))
    first = Compiler(hcb, seg_bytes)
    comp = Compiler(inline_program(hcb, first.funcs, first.by_addr, hot), seg_bytes)
    comp.hot = {comp.hcb.func_new[a] for a in hot if a in comp.hcb.func_new}
    index = compile_all(comp, sys.argv[2], order, instruction_counts)
    print(f'hcb_aot: {len(comp.funcs)} functions, {index["segments"]} segments '
          f'({index["hot_segments"]} ran in the recorded run)')


if __name__ == '__main__':
    main()
