"""Emulation harness for the PowerBASIC 8 string functions in WSR.EXE (MID$, INSTR, PARSE$, REPLACE, ...).

Runs the original runtime routines under Unicorn and writes test vectors for
engine/wsr-core/tests/string.rs (Rust side: engine/wsr-core/src/pb/string.rs).

Usage (from the repo root):
    uv run -q --with unicorn --with pefile python -I tools/emu/pbstring.py \
        [extracted/app/WSR.EXE] [engine/wsr-core/tests/fixtures/string.txt]

Calling convention used by the helpers (see tools/ghidra/PBAnnotate.java HELPERS):
    Each PB frame has a string expression stack: index dword at [ebp-0x78] (push = sub 4),
    top slot [ebp+idx-0x5c], next [ebp+idx-0x58], ... Slots hold BSTR-like pointers (length dword at
    ptr-4); bit 31 set = borrowed (not freed), 0 = "". Helpers take ints in EAX/ECX/EBX/EDX, pop
    their string arguments and push the result. The harness starts with idx = 0, so slots live at
    ebp-0x60, ebp-0x64, ...; [ebp-0x5c..ebp-0x44] is scratch that some helpers (REPLACE, REMOVE$,
    PARSE arr()) use for themselves.
    Imports stubbed: SysAllocStringByteLen (bump heap), SysFreeString (no-op), CharUpperBuffA,
    CharLowerBuffA, CharNextA, GetStringTypeA (Windows code page 1252 semantics, see winapi_* below;
    these define the result of the ANSI mode 0 of UCASE$/LCASE$/MCASE$, which is what the game uses).
    Runtime errors (jumps to 0x6b73fe / 0x6b7406) are recorded and the call is stopped there.

Fixture format: one vector per line, `NAME arg ... = result ...`. Tokens: decimal i32, or `x<hex>` for a
byte string (`x` alone = ""), `!<n>` = runtime error n raised (the result tokens follow the error and give
the string-stack top left behind, which is always "" when an error was raised).
"""

import random
import struct
import sys
import unicodedata

import pefile
from unicorn import UC_ARCH_X86, UC_HOOK_CODE, UC_MODE_32, Uc
from unicorn.x86_const import (
    UC_X86_REG_EAX,
    UC_X86_REG_EBP,
    UC_X86_REG_EBX,
    UC_X86_REG_ECX,
    UC_X86_REG_EDI,
    UC_X86_REG_EDX,
    UC_X86_REG_EFLAGS,
    UC_X86_REG_EIP,
    UC_X86_REG_ESI,
    UC_X86_REG_ESP,
)

BASE = 0x400000
STUB = 0x10000000
HEAP = 0x20000000
HEAP_SIZE = 0x4000000
STACK = 0x30000000
STACK_SIZE = 0x100000
SENTINEL = 0x0F000000

REGS = {
    "eax": UC_X86_REG_EAX,
    "ebx": UC_X86_REG_EBX,
    "ecx": UC_X86_REG_ECX,
    "edx": UC_X86_REG_EDX,
    "esi": UC_X86_REG_ESI,
    "edi": UC_X86_REG_EDI,
}

ERR_ENTRIES = {0x6B73FE: "dl", 0x6B7406: "al"}


class RuntimeErr(Exception):
    pass


# ---------------------------------------------------------------- Windows-1252 API models
def _cp(b):
    return bytes([b]).decode("cp1252", errors="strict") if b not in (0x81, 0x8D, 0x8F, 0x90, 0x9D) else chr(b)


def _enc(c):
    try:
        e = c.encode("cp1252")
    except UnicodeEncodeError:
        return None
    return e[0] if len(e) == 1 else None


def winapi_upper(b):
    """CharUpperBuffA on code page 1252: Unicode simple uppercase, kept only if it maps back to one byte."""
    u = _cp(b).upper()
    if len(u) != 1:
        return b
    e = _enc(u)
    return b if e is None else e


def winapi_lower(b):
    u = _cp(b).lower()
    if len(u) != 1:
        return b
    e = _enc(u)
    return b if e is None else e


def winapi_alpha(b):
    """GetStringTypeA(CT_CTYPE1) C1_ALPHA (0x100) on code page 1252: Unicode letters."""
    return unicodedata.category(_cp(b)).startswith("L")


class Emu:
    def __init__(self, exe):
        pe = pefile.PE(exe)
        uc = Uc(UC_ARCH_X86, UC_MODE_32)
        self.uc = uc
        size = (pe.OPTIONAL_HEADER.SizeOfImage + 0xFFFF) & ~0xFFFF
        uc.mem_map(BASE, size)
        uc.mem_write(BASE, pe.header)
        for s in pe.sections:
            uc.mem_write(BASE + s.VirtualAddress, s.get_data())
        blob = bytes(uc.mem_read(0x6C3028, 0x9B4A8))  # .data startup relocation
        uc.mem_write(0x6C3F18, blob)
        for base, sz in ((STUB, 0x10000), (HEAP, HEAP_SIZE), (STACK, STACK_SIZE), (SENTINEL, 0x1000)):
            uc.mem_map(base, sz)
        uc.mem_write(SENTINEL, b"\xf4")
        self.stubs = {}
        addr = STUB
        for entry in pe.DIRECTORY_ENTRY_IMPORT:
            for imp in entry.imports:
                name = imp.name.decode() if imp.name else f"ord{imp.ordinal}"
                uc.mem_write(imp.address, struct.pack("<I", addr))
                uc.mem_write(addr, b"\xc3")
                self.stubs[addr] = name
                addr += 4
        uc.hook_add(UC_HOOK_CODE, self._stub_hook, begin=STUB, end=STUB + 0xFFFF)
        for a in ERR_ENTRIES:
            uc.hook_add(UC_HOOK_CODE, self._err_hook, begin=a, end=a)
        self.ebp = STACK + STACK_SIZE - 0x800
        self.reset()

    # -- memory helpers
    def u32(self, a):
        return struct.unpack("<I", self.uc.mem_read(a, 4))[0]

    def w32(self, a, v):
        self.uc.mem_write(a, struct.pack("<I", v & 0xFFFFFFFF))

    def alloc(self, data, n=None):
        n = len(data) if n is None else n
        if self.heap + n + 16 > HEAP + HEAP_SIZE:
            raise RuntimeError("heap exhausted")
        p = self.heap + 4
        self.w32(self.heap, n)
        buf = bytes(data[:n]) + b"\0" * (n - len(data[:n])) + b"\0\0"
        self.uc.mem_write(p, buf)
        self.heap = (p + n + 2 + 15) & ~15
        return p

    def reset(self):
        self.heap = HEAP
        self.uc.mem_write(self.ebp - 0x100, b"\0" * 0x100)
        self.err = None

    # -- string stack
    @property
    def idx(self):
        v = self.u32(self.ebp - 0x78)
        return v - (1 << 32) if v >= 1 << 31 else v

    def push(self, s, borrowed=False):
        p = 0 if (s is None or (len(s) == 0 and not borrowed)) else self.alloc(s)
        if borrowed and p:
            p |= 0x80000000
        self.w32(self.ebp - 0x78, self.idx - 4)
        self.w32(self.ebp + self.idx - 0x5C, p)

    def slot(self, k=0):
        return self.u32(self.ebp + self.idx - 0x5C + 4 * k)

    def read_str(self, p):
        p &= 0x7FFFFFFF
        if p == 0:
            return b""
        n = self.u32(p - 4)
        return bytes(self.uc.mem_read(p, n))

    def top(self):
        return self.read_str(self.slot())

    # -- hooks
    def _err_hook(self, uc, address, size, _):
        reg = ERR_ENTRIES[address]
        v = uc.reg_read(UC_X86_REG_EDX if reg == "dl" else UC_X86_REG_EAX) & 0xFF
        self.err = v
        uc.emu_stop()

    def _stub_hook(self, uc, address, size, _):
        name = self.stubs.get(address)
        esp = uc.reg_read(UC_X86_REG_ESP)
        ret = self.u32(esp)
        args = lambda n: struct.unpack(f"<{n}I", uc.mem_read(esp + 4, 4 * n))
        if name == "SysAllocStringByteLen":
            psz, n = args(2)
            if n >= 0x10000000:
                rv = 0
            else:
                data = bytes(uc.mem_read(psz, n)) if psz else b""
                rv = self.alloc(data, n)
            nargs = 2
        elif name == "SysFreeString":
            nargs, rv = 1, 0
        elif name in ("CharUpperBuffA", "CharLowerBuffA"):
            p, n = args(2)
            f = winapi_upper if name == "CharUpperBuffA" else winapi_lower
            uc.mem_write(p, bytes(f(b) for b in uc.mem_read(p, n)))
            nargs, rv = 2, n
        elif name == "CharNextA":
            (p,) = args(1)
            nargs, rv = 1, p + 1 if uc.mem_read(p, 1)[0] else p
        elif name == "GetStringTypeA":
            lcid, typ, src, n, out = args(5)
            assert typ == 1
            vals = []
            for b in uc.mem_read(src, n):
                t = 0x100 if winapi_alpha(b) else 0
                vals.append(t)
            uc.mem_write(out, struct.pack(f"<{n}H", *vals))
            nargs, rv = 5, 1
        else:
            raise RuntimeError(f"unstubbed import {name}")
        uc.reg_write(UC_X86_REG_EAX, rv)
        uc.reg_write(UC_X86_REG_ESP, esp + 4 * nargs)
        self.w32(esp + 4 * nargs, ret)

    def call(self, addr, stack_args=(), **regs):
        uc = self.uc
        esp = STACK + STACK_SIZE - 0x2000
        for a in stack_args:  # pushed in order
            esp -= 4
            self.w32(esp, a)
        esp -= 4
        self.w32(esp, SENTINEL)
        uc.reg_write(UC_X86_REG_ESP, esp)
        uc.reg_write(UC_X86_REG_EBP, self.ebp)
        for r in REGS.values():
            uc.reg_write(r, 0)
        for k, v in regs.items():
            uc.reg_write(REGS[k], v & 0xFFFFFFFF)
        uc.reg_write(UC_X86_REG_EFLAGS, 0x202)
        self.err = None
        uc.emu_start(addr, SENTINEL, count=50_000_000)
        out = {k: uc.reg_read(r) for k, r in REGS.items()}
        out["eflags"] = uc.reg_read(UC_X86_REG_EFLAGS)
        out["esp"] = uc.reg_read(UC_X86_REG_ESP)
        if self.err is None:
            assert uc.reg_read(UC_X86_REG_EIP) == SENTINEL, hex(uc.reg_read(UC_X86_REG_EIP))
        return out


def i32(v):
    v &= 0xFFFFFFFF
    return v - (1 << 32) if v >= 1 << 31 else v


def tok(v):
    if isinstance(v, (bytes, bytearray)):
        return "x" + v.hex()
    return str(v)


# ---------------------------------------------------------------- wrappers around the routines
class PB:
    def __init__(self, emu):
        self.e = emu

    def _run(self, addr, strs, pops_pushes, stack_args=(), **regs):
        e = self.e
        e.reset()
        for s in strs:
            e.push(s)
        before = e.idx
        out = e.call(addr, stack_args, **regs)
        if e.err is None:
            assert e.idx == before + pops_pushes, (hex(addr), e.idx, before)
        return out

    def _res(self, out=None):
        e = self.e
        r = [] if e.err is None else ["!%d" % e.err]
        return r + [e.top()]

    def mid(self, s, start, count):
        self._run(0x6BA1C9, [s], 0, ecx=start, eax=count)
        return self._res()

    def right(self, s, n):
        self._run(0x6BA227, [s], 0, eax=n)
        return self._res()

    def mid_assign(self, t, start, count, r):
        e = self.e
        e.reset()
        p = e.alloc(t) if t else 0
        e.push(r)
        before = e.idx
        e.call(0x6BA15A, ebx=p, edx=len(t), ecx=start, eax=count)
        assert e.err is None and e.idx == before + 4
        return [e.read_str(p)]

    def ltrim(self, s):
        self._run(0x6BD16F, [s], 0)
        return self._res()

    def rtrim(self, s):
        self._run(0x6BD1C0, [s], 0)
        return self._res()

    def ucase(self, s, mode):
        self._run(0x6B9EA0, [s], 0, eax=mode)
        return self._res()

    def lcase(self, s, mode):
        self._run(0x6B9E5C, [s], 0, eax=mode)
        return self._res()

    def mcase(self, s, mode):
        self._run(0x6B9EEF, [s], 0, eax=mode)
        return self._res()

    def instr(self, start, s, f):
        o = self._run(0x6B8BE4, [s, f], 8, eax=start)
        return [i32(o["eax"])]

    def instr_any(self, start, s, f):
        o = self._run(0x6B8C54, [s, f], 8, eax=start)
        return [i32(o["eax"])]

    def parse(self, s, d, n):
        self._run(0x6BD993, [s, d], 4, eax=n)
        return self._res()

    def parse_any(self, s, d, n):
        self._run(0x6BD9E6, [s, d], 4, eax=n)
        return self._res()

    def parsecount(self, s, d):
        o = self._run(0x6BD90C, [s, d], 8)
        return [i32(o["eax"])]

    def parsecount_any(self, s, d):
        o = self._run(0x6BD935, [s, d], 8)
        return [i32(o["eax"])]

    def parse_array(self, s, d, n):
        """PARSE s, d, arr() with arr a 0-based dynamic string array of n elements (descriptor type '.')."""
        e = self.e
        e.reset()
        for x in (s, d):
            e.push(x)
        before = e.idx
        data = e.alloc(b"\0" * (4 * n))
        desc = e.alloc(b"\0" * 0x40)
        e.w32(desc, data)
        e.uc.mem_write(desc + 6, b"\x2e")
        e.w32(desc + 8, n)
        e.w32(desc + 0x14, 4)
        e.call(0x6BDDC5, ebx=desc)
        assert e.err is None and e.idx == before + 8
        return [e.read_str(e.u32(data + 4 * k)) for k in range(n)]

    def replace(self, a, b, s):
        self._run(0x6BD2EE, [a, b, s], 8)
        return self._res()

    def replace_any(self, a, b, s):
        self._run(0x6BD44B, [a, b, s], 8)
        return self._res()

    def remove(self, s, sub):
        self._run(0x6BE36A, [s, sub], 4)
        return self._res()

    def remove_any(self, s, chars):
        self._run(0x6BE447, [s, chars], 4)
        return self._res()

    def retain_any(self, s, chars):
        self._run(0x6BE55C, [s, chars], 4)
        return self._res()

    def extract(self, start, s, m):
        self._run(0x6BCDCF, [s, m], 4, eax=start)
        return self._res()

    def remain(self, start, s, m):
        self._run(0x6BCE7A, [s, m], 4, eax=start)
        return self._res()

    def string_fill(self, n, s):
        self._run(0x6BC210, [s], 0, ebx=n)
        return self._res()

    def string_chr(self, n, code):
        o = self._run(0x6BC1E5, [], -4, ebx=n, eax=code)
        return self._res()

    def space(self, n):
        self._run(0x6BC054, [], -4, eax=n)
        return self._res()

    def chr(self, *codes):
        o = self._run(0x6B76FF, [], -4, stack_args=[c & 0xFFFFFFFF for c in codes] + [len(codes)])
        assert o["esp"] == STACK + STACK_SIZE - 0x2000
        return self._res()

    def asc(self, s):
        o = self._run(0x6BD709, [s], 4)
        return [i32(o["eax"])]

    def len(self, s):
        o = self._run(0x6BF094, [s], 4)
        return [i32(o["eax"])]

    def strcmp(self, a, b):
        o = self._run(0x6BE966, [a, b], 8)
        f = o["eflags"]
        zf, cf = (f >> 6) & 1, f & 1
        return [0 if zf else (-1 if cf else 1)]


# ---------------------------------------------------------------- vector generation
def gen(pb, rnd):
    out = []

    def emit(name, args, res):
        out.append(" ".join([name] + [tok(a) for a in args] + ["="] + [tok(r) for r in res]))

    def rstr(alpha=b"ab, ", lo=0, hi=8):
        return bytes(rnd.choice(alpha) for _ in range(rnd.randint(lo, hi)))

    samples = [b"", b"a", b"ab", b"abc", b"hello world", b" x ", b"   ", b"  lead", b"trail  ", b"a\0b", b"\xff\x80z"]
    ints = [-100, -12, -11, -10, -5, -4, -3, -2, -1, 0, 1, 2, 3, 4, 5, 10, 11, 12, 100, 0x7FFFFFFF, -0x80000000]

    # MID$ / RIGHT$ / LEFT$ (= MID$(s, 1, n))
    for s in samples:
        for st in ints:
            for c in ints:
                if abs(st) > 20 and abs(c) > 20 and rnd.random() < 0.7:
                    continue
                emit("MID", [s, st, c], pb.mid(s, st, c))
        for n in ints:
            emit("RIGHT", [s, n], pb.right(s, n))
    # MID$ assignment
    for t in [b"", b"abcdef", b"x", b"hello"]:
        for r in [b"", b"Z", b"XYZ", b"0123456789"]:
            for st in [-8, -7, -6, -3, -1, 0, 1, 2, 5, 6, 7, 10, 0x7FFFFFFF]:
                for c in [-8, -7, -2, -1, 0, 1, 2, 3, 6, 100, 0x7FFFFFFF]:
                    emit("MIDSET", [t, st, c, r], pb.mid_assign(t, st, c, r))

    # trims
    for s in samples + [b"\t a \t", b" a b ", b"  ", b"\0 ", b" \0"]:
        emit("LTRIM", [s], pb.ltrim(s))
        emit("RTRIM", [s], pb.rtrim(s))
    # case mapping: all bytes, both modes
    allb = bytes(range(256))
    for mode in (0, 1):
        emit("UCASE", [allb, mode], pb.ucase(allb, mode))
        emit("LCASE", [allb, mode], pb.lcase(allb, mode))
        emit("MCASE", [allb, mode], pb.mcase(allb, mode))
        for s in [b"", b"hello WORLD", b"o'neil mcdonald-smith", b"ABC def\x8a\x9a\xe9\xc9 x1y 2nd", b"\0a\0b",
                  b"\x84\x8e\x94\x99\x81\x9a\x82\x90\xa4\xa5\x86\x8f\x87\x80 \x84a \x83b \x85c \x88d"]:
            emit("UCASE", [s, mode], pb.ucase(s, mode))
            emit("LCASE", [s, mode], pb.lcase(s, mode))
            emit("MCASE", [s, mode], pb.mcase(s, mode))
        for b in range(256):  # MCASE: each byte before/after letters and separators
            for s in (bytes([b, 0x61, 0x41]), bytes([0x61, b, 0x61, 0x41]), bytes([0x20, b, 0x41, 0x61])):
                emit("MCASE", [s, mode], pb.mcase(s, mode))
        for _ in range(150):
            s = bytes(rnd.choice(b"aAzZ \x8a\x9a\xc0\xe0\xff\x9f\x84\x8e.'-1\xd7\xf7") for _ in range(rnd.randint(0, 12)))
            emit("MCASE", [s, mode], pb.mcase(s, mode))

    # INSTR
    istr = [b"", b"a", b"abcabc", b"aaaa", b"hello world", b"ab,cd,,ef"]
    ifind = [b"", b"a", b"b", b"c", b"bc", b"abc", b"abcabc", b"abcabcx", b"aa", b"aaa", b"o", b"l", b"lo", b",", b",,", b"z"]
    starts = [-0x80000000, -100, -10, -7, -6, -5, -4, -3, -2, -1, 0, 1, 2, 3, 4, 5, 6, 7, 10, 100, 0x7FFFFFFF]
    for s in istr:
        for f in ifind:
            for st in starts:
                emit("INSTR", [st, s, f], pb.instr(st, s, f))
                emit("INSTRANY", [st, s, f], pb.instr_any(st, s, f))
    for _ in range(600):
        s, f = rstr(b"abc", 0, 10), rstr(b"abc", 0, 3)
        st = rnd.randint(-12, 12)
        emit("INSTR", [st, s, f], pb.instr(st, s, f))
        emit("INSTRANY", [st, s, f], pb.instr_any(st, s, f))

    # PARSE family
    pstr = [b"", b"a", b"a,b,c", b",a,,b,", b",", b",,", b"abc", b"a, b ,c", b" a , b ", b"aXYbXYc", b"XYaXY", b"XYXY",
            b"\"a,b\",c", b" \"q\" ,x", b"\"unterminated,x", b"a\"b\"c,d", b"  ,  ,", b"x \"y\" z, w", b"\"\"", b"a\tb ,c"]
    pdel = [b"", b",", b"XY", b" ", b"b", b"abc", b",b", b"\"", b"XYZ"]
    pn = [-0x80000000, -5, -4, -3, -2, -1, 0, 1, 2, 3, 4, 5, 6, 0x7FFFFFFF]
    for s in pstr:
        for d in pdel:
            emit("PARSECOUNT", [s, d], pb.parsecount(s, d))
            emit("PARSECOUNTANY", [s, d], pb.parsecount_any(s, d))
            for n in pn:
                emit("PARSE", [s, d, n], pb.parse(s, d, n))
                emit("PARSEANY", [s, d, n], pb.parse_any(s, d, n))
            for n in (1, 2, 4, 7):
                emit("PARSEARR", [s, d, n], pb.parse_array(s, d, n))
    for _ in range(500):
        s = rstr(b"ab,; \"", 0, 12)
        d = rnd.choice([b"", b",", b";", b",;", b"; ", b"a", b"ab"])
        n = rnd.randint(-6, 7)
        emit("PARSECOUNT", [s, d], pb.parsecount(s, d))
        emit("PARSECOUNTANY", [s, d], pb.parsecount_any(s, d))
        emit("PARSE", [s, d, n], pb.parse(s, d, n))
        emit("PARSEANY", [s, d, n], pb.parse_any(s, d, n))
        k = rnd.randint(1, 6)
        emit("PARSEARR", [s, d, k], pb.parse_array(s, d, k))

    # REPLACE / REMOVE$ / RETAIN$
    rs = [b"", b"a", b"aaaa", b"abcabc", b"hello world", b"xaxbxcx", b"ab"]
    rf = [b"", b"a", b"aa", b"b", b"abc", b"x", b"ab", b"abcabc", b"abcabcab", b"o w", b"xx"]
    rw = [b"", b"Z", b"ZZZ", b"a", b"aa"]
    for s in rs:
        for f in rf:
            for w in rw:
                emit("REPLACE", [f, w, s], pb.replace(f, w, s))
            emit("REMOVE", [s, f], pb.remove(s, f))
            emit("REMOVEANY", [s, f], pb.remove_any(s, f))
            emit("RETAINANY", [s, f], pb.retain_any(s, f))
    for s in rs:
        for f, w in [(b"", b""), (b"a", b"Z"), (b"ab", b"ZY"), (b"aa", b"XY"), (b"abc", b"cab"), (b"x", b""), (b"", b"x"),
                     (b"ab", b"Z"), (b"o", b"0"), (b" ", b"_")]:
            emit("REPLACEANY", [f, w, s], pb.replace_any(f, w, s))
    for _ in range(500):
        s, f, w = rstr(b"abc", 0, 12), rstr(b"abc", 0, 3), rstr(b"abZ", 0, 3)
        emit("REPLACE", [f, w, s], pb.replace(f, w, s))
        emit("REMOVE", [s, f], pb.remove(s, f))
        emit("REMOVEANY", [s, f], pb.remove_any(s, f))
        emit("RETAINANY", [s, f], pb.retain_any(s, f))
        w2 = bytes(rnd.choice(b"XYZ") for _ in f)
        emit("REPLACEANY", [f, w2, s], pb.replace_any(f, w2, s))

    # EXTRACT$ / REMAIN$
    es = [b"", b"a", b"abc", b"abcabc", b"a=b=c", b"key: value", b"aaaa"]
    em = [b"", b"a", b"b", b"c", b"bc", b"=", b": ", b"aa", b"abcd", b"z"]
    for s in es:
        for m in em:
            for st in [-0x80000000, -3, -1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 100, 0x7FFFFFFF]:
                emit("EXTRACT", [st, s, m], pb.extract(st, s, m))
                emit("REMAIN", [st, s, m], pb.remain(st, s, m))
    for _ in range(300):
        s, m, st = rstr(b"ab=", 0, 10), rstr(b"ab=", 0, 2), rnd.randint(-2, 12)
        emit("EXTRACT", [st, s, m], pb.extract(st, s, m))
        emit("REMAIN", [st, s, m], pb.remain(st, s, m))

    # STRING$ / SPACE$ / CHR$ / ASC / LEN
    for n in [-0x80000000, -5, -1, 0, 1, 2, 3, 7, 64]:
        for s in [b"", b"a", b"xyz", b"\0", b"\xff"]:
            emit("STRINGS", [n, s], pb.string_fill(n, s))
        for c in [0, 32, 65, 255, 256, 321, -1, -191]:
            emit("STRINGC", [n, c], pb.string_chr(n, c))
        emit("SPACE", [n], pb.space(n))
    for codes in [[65], [0], [255], [256], [-1], [-2], [65, 66, 67], [65, -1, 66], [-1, -1], [13, 10], [0x141, 0x1FF, 0x7FFFFFFF], [-256]]:
        emit("CHR", [len(codes)] + codes, pb.chr(*codes))
    for s in samples + [b"\x80", b"\0"]:
        emit("ASC", [s], pb.asc(s))
        emit("LEN", [s], pb.len(s))

    # string comparison
    cs = [b"", b"a", b"A", b"b", b"ab", b"abc", b"abd", b"a\0", b"\0", b"\xff", b"\x80", b"Z", b"z", b"ab ", b"ab\x01"]
    for a in cs:
        for b in cs:
            emit("CMP", [a, b], pb.strcmp(a, b))
    return out


def main():
    exe = sys.argv[1] if len(sys.argv) > 1 else "extracted/app/WSR.EXE"
    dst = sys.argv[2] if len(sys.argv) > 2 else "engine/wsr-core/tests/fixtures/string.txt"
    pb = PB(Emu(exe))
    lines = gen(pb, random.Random(20261026))
    with open(dst, "w") as f:
        f.write("# PB string function vectors from WSR.EXE under emulation; generated by tools/emu/pbstring.py\n")
        f.write("\n".join(lines) + "\n")
    print(f"{len(lines)} vectors -> {dst}")


if __name__ == "__main__":
    main()
