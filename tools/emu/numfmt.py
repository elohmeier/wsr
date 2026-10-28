#!/usr/bin/env python3
"""Emulation harness for PowerBASIC 8 number <-> text conversion as found in WSR.EXE.

Runs the original runtime routines under Unicorn:

  STR$/FORMAT$(x)  PB_StrNumDigits 0x6bd517 (STR$, keeps the sign column) and 0x6bd4e1 (FORMAT$
                   without mask, digits in EAX); both go through 0x6bd52d -> 0x6bf294 -> 0x6bf404.
                   The fixed-digit entry points (0x6bd503/0x6bd4ef/0x6bd4d0/0x6bd4bf) are spot-checked.
  WRITE #          PB_WriteNum 0x6b7d0b, PB_WriteStr 0x6b7d2f, PB_WriteSep 0x6b7d5d, CRLF 0x6bee67;
                   the file lookup 0x6b7d89 is patched to `ret` (EDI preset to a fake file block) and
                   WriteFile is stubbed to capture the bytes.
  VAL              PB_Val 0x6bd6bb (string on the PB string stack, result in ST0, stored as tword).
  INPUT #/EOF      PB_InputNum 0x6ba3ea, PB_InputItem 0x6b85e3, PB_LineInput 0x6b84bb, PB_Eof 0x6b7b17;
                   the file lookup 0x6bc656 is patched to `mov esi, FILE; clc; ret`; the fake file
                   block is a buffered input file ([+0xa]=1, [+0xc]=1, [+0xe]=1) whose buffer size
                   [+0x1c] is configurable, ReadFile/SetFilePointer are stubbed over an in-memory file.
                   Runtime errors (pbrt_RaiseError 0x6b7406 / 0x6b73fe) are reported as "!<code>".

Usage:

    uv run -q --with unicorn --with pefile python -I tools/emu/numfmt.py str 0.1 15
    uv run -q --with unicorn --with pefile python -I tools/emu/numfmt.py val "1.5e3"
    uv run -q --with unicorn --with pefile python -I tools/emu/numfmt.py input '1, "a,b" ,x\r\n' NSSL
    uv run -q --with unicorn --with pefile python -I tools/emu/numfmt.py fixture \
        engine/wsr-core/tests/fixtures/numfmt.txt

Fixture format (tab separated, '#' comments). x80 = 80-bit value as 20 hex digits, big-endian
(sign/exponent word first, then the 64-bit mantissa); bytes are hex strings ('-' = empty).
  S <digits> <keep> <x80> <text>     STR$ (keep=1, via 0x6bd517) / FORMAT$ (keep=0, via 0x6bd4e1)
  W <x80> <text>                     bytes written by PB_WriteNum
  Q <bytes> <bytes>                  bytes written by PB_WriteStr
  V <bytes> <x80>                    VAL(bytes) as the 80-bit ST0 result
  I <bufsize> <file bytes> <ops> <result>...   INPUT # script; ops: N=INPUT num, S=INPUT string,
                                     L=LINE INPUT, E=EOF(); results: N -> x80, S/L -> bytes,
                                     E -> 0/1, error -> !<code> (script stops at the first error).
FPU control word is 0x133f (set by the PB runtime at 0x6b7339): 64-bit precision, round to nearest.
"""
import math
import random
import struct
import sys
from fractions import Fraction
from pathlib import Path

import pefile
from unicorn import UC_ARCH_X86, UC_HOOK_CODE, UC_MODE_32, Uc
from unicorn.x86_const import (
    UC_X86_REG_EAX,
    UC_X86_REG_EBP,
    UC_X86_REG_EDI,
    UC_X86_REG_EIP,
    UC_X86_REG_ESI,
    UC_X86_REG_ESP,
)

REPO = Path(__file__).resolve().parents[2]
EXE = REPO / "extracted/app/WSR.EXE"

STUB_BASE = 0x10000000
CODE = 0x10100000
DATA = 0x10101000  # +0 tword in, +0x10 cw, +0x20 tword out
FILEBLK = 0x10200000  # fake PB file block (0x44 header + buffer)
HEAP = 0x20000000
HEAP_SIZE = 0x01000000
STACK_LO = 0x30000000
STACK_SIZE = 0x00400000
FRAME = STACK_LO + STACK_SIZE - 0x10000
ERRS = (0x6B7406, 0x6B73FE)


class PbError(Exception):
    def __init__(self, code):
        self.code = code


class Emu:
    def __init__(self):
        pe = pefile.PE(str(EXE))
        img = bytearray(pe.get_memory_mapped_image())
        img = img + bytes(pe.OPTIONAL_HEADER.SizeOfImage - len(img))
        base = pe.OPTIONAL_HEADER.ImageBase
        src, dst, n = 0x6C3028 - base, 0x6C3F18 - base, 0x9B4A8
        img[dst : dst + n] = bytes(img[src : src + n])
        # patches: file lookups
        img[0x6B7D89 - base] = 0xC3  # WRITE: EDI preset
        img[0x6BC656 - base : 0x6BC656 - base + 7] = b"\xbe" + struct.pack("<I", FILEBLK) + b"\xf8\xc3"
        self.uc = uc = Uc(UC_ARCH_X86, UC_MODE_32)
        uc.mem_map(base, (len(img) + 0xFFF) & ~0xFFF)
        uc.mem_write(base, bytes(img))
        uc.mem_map(STUB_BASE, 0x1000)
        uc.mem_map(CODE, 0x2000)
        uc.mem_map(FILEBLK, 0x100000)
        uc.mem_map(HEAP, HEAP_SIZE)
        uc.mem_map(STACK_LO, STACK_SIZE)
        self.stubs = {}
        k = 0
        for e in pe.DIRECTORY_ENTRY_IMPORT:
            for i in e.imports:
                name = (i.name or b"ord%d" % i.ordinal).decode()
                self.stubs[STUB_BASE + k] = name
                uc.mem_write(i.address, struct.pack("<I", STUB_BASE + k))
                k += 1
        uc.mem_write(STUB_BASE, b"\xf4" * k)
        uc.mem_write(DATA + 0x10, struct.pack("<H", 0x133F))
        uc.hook_add(UC_HOOK_CODE, self._hook_stub, begin=STUB_BASE, end=STUB_BASE + 0xFFF)
        for a in ERRS:
            uc.hook_add(UC_HOOK_CODE, self._hook_err, begin=a, end=a)
        self.entries = {}
        self.written = bytearray()
        self.file = b""
        self.fpos = 0
        self.heap = HEAP

    def _entry(self, target, fld):
        """prologue: fldcw; [fld tword in]; call target; fstp tword out (if fld is None: no fpu in)."""
        key = (target, fld)
        if key in self.entries:
            return self.entries[key]
        at = CODE + 0x40 * len(self.entries)
        code = b"\xd9\x2d" + struct.pack("<I", DATA + 0x10)
        if fld == "in":
            code += b"\xdb\x2d" + struct.pack("<I", DATA)
        call_at = at + len(code)
        code += b"\xe8" + struct.pack("<i", target - (call_at + 5))
        if fld == "out":
            code += b"\xdb\x3d" + struct.pack("<I", DATA + 0x20)
        end = at + len(code)
        code += b"\xf4"
        self.uc.mem_write(at, code)
        self.uc.hook_add(UC_HOOK_CODE, lambda uc, a, s, u: uc.emu_stop(), begin=end, end=end)
        self.entries[key] = (at, end)
        return at, end

    def _hook_err(self, uc, addr, size, ud):
        al = uc.reg_read(UC_X86_REG_EAX) & 0xFF
        if addr == 0x6B73FE:
            from unicorn.x86_const import UC_X86_REG_EDX

            al = uc.reg_read(UC_X86_REG_EDX) & 0xFF
        self.err = al
        uc.emu_stop()

    def _pop_args(self, n):
        uc = self.uc
        esp = uc.reg_read(UC_X86_REG_ESP)
        ret, *args = struct.unpack("<%dI" % (n + 1), uc.mem_read(esp, 4 * (n + 1)))
        uc.reg_write(UC_X86_REG_ESP, esp + 4 * (n + 1))
        uc.reg_write(UC_X86_REG_EIP, ret)
        return args

    def alloc(self, data):
        p = self.heap + 4
        self.uc.mem_write(self.heap, struct.pack("<I", len(data)) + data + b"\0\0")
        self.heap = (p + len(data) + 2 + 7) & ~3
        if self.heap > HEAP + HEAP_SIZE - 0x10000:
            self.heap = HEAP
        return p

    def _hook_stub(self, uc, addr, size, ud):
        name = self.stubs[addr]
        if name == "SysAllocStringByteLen":
            psz, n = self._pop_args(2)
            data = bytes(uc.mem_read(psz, n)) if psz else bytes(n)
            uc.reg_write(UC_X86_REG_EAX, self.alloc(data))
        elif name == "SysFreeString":
            self._pop_args(1)
        elif name in ("SetErrorMode", "SetLastError"):
            self._pop_args(1)
            uc.reg_write(UC_X86_REG_EAX, 0)
        elif name == "GetLastError":
            self._pop_args(0)
            uc.reg_write(UC_X86_REG_EAX, 0)
        elif name == "WriteFile":
            h, buf, n, pw, ov = self._pop_args(5)
            self.written += uc.mem_read(buf, n)
            if pw:
                uc.mem_write(pw, struct.pack("<I", n))
            uc.reg_write(UC_X86_REG_EAX, 1)
        elif name == "SetFilePointer":
            h, lo, phi, meth = self._pop_args(4)
            assert meth == 0
            self.fpos = lo
            if phi:
                uc.mem_write(phi, b"\0\0\0\0")
            uc.reg_write(UC_X86_REG_EAX, lo)
        elif name == "ReadFile":
            h, buf, n, pr, ov = self._pop_args(5)
            chunk = self.file[self.fpos : self.fpos + n]
            uc.mem_write(buf, chunk)
            self.fpos += len(chunk)
            uc.mem_write(pr, struct.pack("<I", len(chunk)))
            uc.reg_write(UC_X86_REG_EAX, 1)
        else:
            raise RuntimeError("unhandled import " + name)

    def run(self, target, fld=None, regs=(), stack_idx=0):
        uc = self.uc
        at, end = self._entry(target, fld)
        uc.mem_write(FRAME - 0x78, struct.pack("<i", stack_idx))
        uc.reg_write(UC_X86_REG_EBP, FRAME)
        uc.reg_write(UC_X86_REG_ESP, FRAME - 0x400)
        for r, v in regs:
            uc.reg_write(r, v)
        self.err = None
        uc.mem_write(0x6C3010, b"\0\0\0\0")  # API-call lock used by 0x6bf5b6
        uc.emu_start(at, end + 1, count=20_000_000)
        if self.err is not None:
            raise PbError(self.err)
        if uc.reg_read(UC_X86_REG_EIP) != end:
            raise RuntimeError("did not finish, eip=%x" % uc.reg_read(UC_X86_REG_EIP))

    def top_str(self):
        (idx,) = struct.unpack("<i", self.uc.mem_read(FRAME - 0x78, 4))
        (p,) = struct.unpack("<I", self.uc.mem_read(FRAME + idx - 0x5C, 4))
        if p == 0:
            return idx, b""
        (n,) = struct.unpack("<I", self.uc.mem_read(p - 4, 4))
        return idx, bytes(self.uc.mem_read(p, n))

    # ---- STR$ / FORMAT$ ----
    def strnum(self, x80: bytes, digits: int, keep: bool, entry=None) -> bytes:
        self.heap = HEAP
        self.uc.mem_write(DATA, x80)
        target = entry or (0x6BD517 if keep else 0x6BD4E1)
        self.run(target, "in", [(UC_X86_REG_EAX, digits & 0xFFFFFFFF)])
        idx, s = self.top_str()
        assert idx == -4, idx
        return s

    # ---- WRITE # ----
    def _fileblk_out(self):
        self.uc.mem_write(FILEBLK, bytes(0x44))

    def writenum(self, x80: bytes) -> bytes:
        self._fileblk_out()
        self.written = bytearray()
        self.uc.mem_write(DATA, x80)
        self.run(0x6B7D0B, "in", [(UC_X86_REG_EDI, FILEBLK)])
        return bytes(self.written)

    def writestr(self, s: bytes) -> bytes:
        self.heap = HEAP
        self._fileblk_out()
        self.written = bytearray()
        p = self.alloc(s) if s else 0
        self.uc.mem_write(FRAME - 4 - 0x5C, struct.pack("<I", p))
        self.run(0x6B7D2F, None, [(UC_X86_REG_EDI, FILEBLK)], stack_idx=-4)
        (idx,) = struct.unpack("<i", self.uc.mem_read(FRAME - 0x78, 4))
        assert idx == 0
        return bytes(self.written)

    def writesep(self) -> bytes:
        self._fileblk_out()
        self.written = bytearray()
        self.run(0x6B7D5D, None, [(UC_X86_REG_EDI, FILEBLK)])
        return bytes(self.written)

    def writecrlf(self) -> bytes:
        self._fileblk_out()
        self.written = bytearray()
        self.run(0x6BEE67, None, [(UC_X86_REG_ESI, FILEBLK)])
        return bytes(self.written)

    # ---- VAL ----
    def val(self, s: bytes) -> bytes:
        self.heap = HEAP
        p = self.alloc(s) if s else 0
        self.uc.mem_write(FRAME - 4 - 0x5C, struct.pack("<I", p))
        self.run(0x6BD6BB, "out", [], stack_idx=-4)
        (idx,) = struct.unpack("<i", self.uc.mem_read(FRAME - 0x78, 4))
        assert idx == 0
        return bytes(self.uc.mem_read(DATA + 0x20, 10))

    # ---- INPUT # ----
    def open_input(self, data: bytes, bufsize: int):
        self.file = data
        self.fpos = 0
        hdr = bytearray(0x44)
        struct.pack_into("<I", hdr, 6, 0x1234)  # handle
        struct.pack_into("<H", hdr, 0xA, 1)  # buffered
        hdr[0xC] = 1  # input
        hdr[0xE] = 1  # mode for 0x6b87b1
        struct.pack_into("<I", hdr, 0x1C, bufsize)
        self.uc.mem_write(FILEBLK, bytes(hdr))
        self.heap = HEAP

    def input_op(self, op: str):
        if op == "N":
            self.run(0x6BA3EA, "out", [])
            return bytes(self.uc.mem_read(DATA + 0x20, 10))
        if op in "SL":
            self.run(0x6B85E3 if op == "S" else 0x6B84BB, None, [])
            idx, s = self.top_str()
            assert idx == -4, idx
            return s
        if op == "E":
            self.run(0x6B7B17, None, [(UC_X86_REG_EAX, 1)])
            return self.uc.reg_read(UC_X86_REG_EAX) & 0xFFFFFFFF
        raise ValueError(op)

    def input_script(self, data: bytes, ops: str, bufsize=4096):
        self.open_input(data, bufsize)
        out = []
        for op in ops:
            try:
                r = self.input_op(op)
            except PbError as e:
                out.append("!%d" % e.code)
                break
            if op == "N":
                out.append(x80hex(r))
            elif op == "E":
                out.append("1" if r else "0")
            else:
                out.append(hx(r))
        return out


# ---------------------------------------------------------------- value helpers
def f64_to_x80(x: float) -> bytes:
    (b,) = struct.unpack("<Q", struct.pack("<d", x))
    sign = b >> 63
    e = (b >> 52) & 0x7FF
    m = b & ((1 << 52) - 1)
    if e == 0 and m == 0:
        return struct.pack("<QH", 0, sign << 15)
    if e == 0x7FF:
        return struct.pack("<QH", (1 << 63) | (m << 11), (sign << 15) | 0x7FFF)
    if e == 0:
        sh = 53 - m.bit_length()
        m <<= sh
        e = 1 - sh
    else:
        m |= 1 << 52
    return struct.pack("<QH", m << 11, (sign << 15) | (e - 1023 + 16383))


def x80hex(b: bytes) -> str:
    m, se = struct.unpack("<QH", b)
    return "%04x%016x" % (se, m)


def hex2x80(h: str) -> bytes:
    return struct.pack("<QH", int(h[4:], 16), int(h[:4], 16))


def x80_to_fraction(b: bytes):
    m, se = struct.unpack("<QH", b)
    e = se & 0x7FFF
    s = -1 if se & 0x8000 else 1
    if e == 0x7FFF:
        return None
    return s * Fraction(m) * Fraction(2) ** (e - 16383 - 63)


def frac_to_x80(f: Fraction) -> bytes:
    """Round a rational to the nearest 80-bit value (ties to even); normal range only."""
    if f == 0:
        return bytes(10)
    s = 0x8000 if f < 0 else 0
    f = abs(f)
    e = f.numerator.bit_length() - f.denominator.bit_length()
    while Fraction(2) ** e > f:
        e -= 1
    while Fraction(2) ** (e + 1) <= f:
        e += 1
    q = f / Fraction(2) ** (e - 63)
    m = q.numerator // q.denominator
    r = q - m
    if r > Fraction(1, 2) or (r == Fraction(1, 2) and m & 1):
        m += 1
    if m >> 64:
        m >>= 1
        e += 1
    return struct.pack("<QH", m, s | (e + 16383))


def hx(b: bytes) -> str:
    return b.hex() if b else "-"


# ---------------------------------------------------------------- vector generation
def interesting_f64(rng):
    xs = [0.0, -0.0, 1.0, -1.0, 0.5, 1.5, 2.5, -2.5, 0.25, 0.125, 1 / 3, 2 / 3, -1 / 3, 0.1, 0.2, 0.3,
          0.7, 1.1, 1e15 - 1, 1e15, 1e15 + 1, 1e16, 1e16 - 2, 1e17, 1e18, 1e19, 2 ** 53, 2 ** 63, 2 ** 64,
          123456789012345.6, 999999999999999.4, 999999999999999.5, 99999999999999.95, 9999999.5,
          0.9999999999999995, 0.99999995, 0.0001, 0.00001, 1e-15, 1e-16, 1e-20, 1e-100, 1e-300,
          1e-307, 2.2250738585072014e-308, 5e-324, 1e-310, 1.7976931348623157e308, 1e300, 1e100,
          3.14159265358979, math.pi, math.e, 100.0, 1000000.0, 12345.678, 0.05, 0.15, 0.35, 1.005,
          2.675, 1234.5, 0.045, 1e7, 1e8 - 0.5, 9999999.0, 99999995.0, 0.000123456789,
          float("inf"), float("-inf")]
    for k in range(-25, 309):
        xs += [10.0 ** k, -(10.0 ** k), 10.0 ** k * 0.999999999999999, 3 * 10.0 ** k]
    for k in range(-40, 70):
        xs += [2.0 ** k, 2.0 ** k + 2.0 ** (k - 40)]
    for n in range(1, 18):
        b = 10 ** n
        xs += [float(b - 1), float(b + 1), (b - 0.5), b - 0.05, 1 / b * 5, float(b // 2)]
    for _ in range(600):  # money amounts
        c = rng.randrange(-10 ** 13, 10 ** 13)
        xs.append(c / 100)
        xs.append(c / 1000 * rng.choice([1, 7, 13]))
    for _ in range(400):  # rates
        xs.append(rng.random() * rng.choice([1, 0.1, 0.01, 100]))
        xs.append(rng.randrange(1, 10000) / 10000)
    for _ in range(300):  # ints
        xs.append(float(rng.randrange(-2 ** 31, 2 ** 31)))
    for _ in range(400):  # arbitrary doubles
        while True:
            (x,) = struct.unpack("<d", struct.pack("<Q", rng.getrandbits(64)))
            if math.isfinite(x):
                break
        xs.append(x)
    for _ in range(300):  # n.5 at digit 15/16
        d = rng.randrange(10 ** 14, 10 ** 15)
        xs.append(float(d) + 0.5)
        xs.append((d + 0.5) / 10 ** rng.randrange(0, 20))
        xs.append(d * 10 + 5.0)
    return xs


def gen_fixture(path):
    rng = random.Random(1234)
    emu = Emu()
    lines = ["# generated by tools/emu/numfmt.py fixture (see header there for the format)"]
    xs = interesting_f64(rng)
    # STR$/FORMAT$ with the digit counts the game uses (15, 7) plus all others on a subset
    for x in xs:
        x80 = f64_to_x80(x)
        for digits, keep in ((15, 1), (7, 1), (15, 0), (7, 0)):
            lines.append("S\t%d\t%d\t%s\t%s" % (digits, keep, x80hex(x80), emu.strnum(x80, digits, keep).decode()))
    for x in xs[:: 7]:
        x80 = f64_to_x80(x)
        for digits in (0, 1, 2, 3, 5, 10, 14, 16, 17, 18, 19, -1):
            keep = rng.randrange(2)
            lines.append("S\t%d\t%d\t%s\t%s" % (digits, keep, x80hex(x80), emu.strnum(x80, digits, keep).decode()))
    # random 80-bit values (EXT variables)
    for _ in range(500):
        x80 = struct.pack("<QH", rng.getrandbits(64) | (1 << 63), rng.randrange(0x3FFF - 200, 0x3FFF + 200) | rng.choice([0, 0x8000]))
        lines.append("S\t15\t1\t%s\t%s" % (x80hex(x80), emu.strnum(x80, 15, 1).decode()))
        lines.append("S\t18\t0\t%s\t%s" % (x80hex(x80), emu.strnum(x80, 18, 0).decode()))
    # sanity: fixed entry points equal the digit variants
    for x in xs[:: 13]:
        x80 = f64_to_x80(x)
        assert emu.strnum(x80, 0, 1, 0x6BD503) == emu.strnum(x80, 15, 1)
        assert emu.strnum(x80, 0, 1, 0x6BD4EF) == emu.strnum(x80, 7, 1)
        assert emu.strnum(x80, 0, 0, 0x6BD4D0) == emu.strnum(x80, 15, 0)
        assert emu.strnum(x80, 0, 0, 0x6BD4BF) == emu.strnum(x80, 7, 0)
    # WRITE #
    assert emu.writesep() == b",", emu.writesep()
    assert emu.writecrlf() == b"\r\n"
    for x in xs[:: 3]:
        x80 = f64_to_x80(x)
        lines.append("W\t%s\t%s" % (x80hex(x80), emu.writenum(x80).decode()))
    for s in [b"", b"abc", b'a"b', b"a,b", b" lead", b"trail ", b"\r\n", bytes(range(1, 256, 7)), b'""']:
        lines.append("Q\t%s\t%s" % (hx(s), hx(emu.writestr(s))))
    # VAL
    vals = val_inputs(rng, xs)
    for s in vals:
        lines.append("V\t%s\t%s" % (hx(s), x80hex(emu.val(s))))
    # INPUT #
    for data, ops, bs in input_scripts(rng):
        lines.append("I\t%d\t%s\t%s\t%s" % (bs, hx(data), ops, "\t".join(emu.input_script(data, ops, bs))))
    Path(path).write_text("\n".join(lines) + "\n")
    print("wrote %d vectors to %s" % (len(lines) - 1, path))


def val_inputs(rng, xs):
    v = [b"", b" ", b"0", b"-0", b"-", b"+", b".", b"-.", b"1", b"-1", b"+1", b"  12", b"\t12", b"\n12", b"\r12",
         b"12abc", b"1.5", b"1.50", b"1.500", b"1.500x", b"10.", b"100.", b".5", b"0.5", b"00012", b"0.0001",
         b"1e3", b"1E3", b"1d3", b"1D3", b"1e+3", b"1e-3", b"1e", b"1e+", b"1e-", b"1e-x", b"1ex", b"e5",
         b"1.2.3", b"1,000", b"1 000", b"--1", b"+-1", b"-+1", b"&H10", b"&h1F", b"&HFF", b"&HFFFF", b"&HFFFF&",
         b"&H8000", b"&H7FFF", b"&H0FFFF", b"&HFFFFFFFF", b"&HFFFFFFFF&", b"&H80000000", b"&H100000000",
         b"&HFFFFFFFFFFFFFFFF", b"&H10000000000000000", b"&O17", b"&17", b"&B101", b"&b2", b"&Q7", b"&", b"&H",
         b"&HG", b"&H1G", b"&HFF%", b"&HFF?", b"&HFFFF?", b"&HFFFF??", b"&HFFFFFFFFFFFFFFFF?", b"&HFF&&",
         b"&HFFFF&&", b"-&H10", b" &H10", b"1e4932", b"1e4931", b"9e4931", b"1e4933", b"1e5000", b"1e-4931",
         b"1e-4932", b"1e-4933", b"1e-5000", b"1e400", b"1e-400", b"1e308", b"2e308", b"1e-320", b"1e99999",
         b"123456789012345678", b"1234567890123456789", b"12345678901234567890123", b"0.1234567890123456789",
         b"999999999999999999", b"9999999999999999999", b"0.000000000000000000000001", b"1e0", b"1e00009",
         b"1e-0", b"1.e5", b".e5", b"0e5", b"5 ", b"5\x00", b"1e3.5", b"1.5e3e2", b"$12", b"12$", b"1d", b"%"]
    v += [b"0" * k + b"1" for k in range(0, 30, 3)]
    v += [b"0." + b"0" * k + b"1" for k in range(0, 30, 3)]
    for x in xs[:: 2]:
        if math.isfinite(x):
            v.append(repr(x).encode())
            v.append(("%.17g" % x).encode())
            v.append(("%.15g" % x).encode())
            v.append(("%.20e" % x).encode())
    for _ in range(600):
        n = rng.randrange(1, 25)
        s = "".join(rng.choice("0123456789") for _ in range(n))
        if rng.random() < 0.6:
            p = rng.randrange(0, n + 1)
            s = s[:p] + "." + s[p:]
        if rng.random() < 0.4:
            s += rng.choice("eEdD") + rng.choice(["", "+", "-"]) + str(rng.randrange(0, 400))
        if rng.random() < 0.3:
            s = rng.choice("+- ") + s
        v.append(s.encode())
    for _ in range(300):  # money-ish strings the game itself writes back
        c = rng.randrange(-10 ** 13, 10 ** 13)
        v.append(("%d.%02d" % (c // 100, c % 100)).encode())
    for _ in range(200):
        v.append(bytes(rng.choice(b"0123456789.eE+-&Hh ,\t") for _ in range(rng.randrange(1, 12))))
    return v


def input_scripts(rng):
    S = [
        (b"1,2,3\r\n", "NNNE"),
        (b"1,2,3", "NNNEN"),
        (b"", "N"),
        (b"", "S"),
        (b"", "L"),
        (b"", "E"),
        (b"\x1a", "E"),
        (b"\x1a", "S"),
        (b"abc\x1a", "LL"),
        (b"abc\x1adef", "LE"),
        (b"  12  ,  34  \r\n", "NNE"),
        (b"12 34\r\n", "NNE"),
        (b"1\t2,3\n4\r\n", "NNN"),
        (b"\t5,6", "NN"),
        (b'"12",3', "NN"),
        (b"1e3,1d2,&H10,-.5", "NNNN"),
        (b'"hello, world",x\r\n', "SSE"),
        (b'"ab"cd,e\r\n', "SSS"),
        (b'"ab"  ,cd\r\n', "SS"),
        (b'"ab"  \r\ncd\r\n', "SS"),
        (b'"ab\r\ncd"\r\n', "SE"),
        (b'"unterminated', "SE"),
        (b'a "b" c,d', "SS"),
        (b"  lead, trail  ,x", "SSS"),
        (b"a\tb,c\nd,e", "SSS"),
        (b",,\r\n", "SSSE"),
        (b"\r\n\r\n", "SSE"),
        (b"\r\n\r\n", "LLE"),
        (b"line1\r\nline2\r\n", "LLE"),
        (b"line1\nline2\n", "LE"),
        (b"line1\rline2\r", "LLE"),
        (b"a\r\r\nb", "LLL"),
        (b"a\n\r\nb", "LL"),
        (b'x, "y" ,z\r\n', "LE"),
        (b"  spaces  \r\n", "L"),
        (b"1,2\r\nabc\r\n", "NNL"),
        (b"1\r\nabc\r\n", "NL"),
        (b"1 \r\nabc\r\n", "NL"),
        (b'"q"\r\nabc\r\n', "SL"),
        (b'"q" \r\n\r\nabc\r\n', "SLL"),
        (b"12abc,5", "NN"),
        (b"\x01\x02,3", "NSN"),
        (b"-,+,.", "NNN"),
        (b"1,\r\n2", "NNN"),
        (b"1\r\n\r\n2", "NN"),
        (b"1\n\n2", "NN"),
        (b'"a""b"', "SS"),
        (b"x" * 300 + b",y", "SS"),
        (b"x" * 600 + b"\r\ny", "LL"),
        (b"9" * 300 + b",1", "NN"),
        (b'"' + b"z" * 300 + b'",1', "SN"),
    ]
    out = []
    for data, ops in S:
        for bs in (4096, 1, 2, 3, 5):
            out.append((data, ops, bs))
    # random scripts from the WRITE # style of the game
    for _ in range(300):
        items = []
        for _ in range(rng.randrange(1, 6)):
            t = rng.random()
            if t < 0.4:
                items.append(rng.choice(["%d", "%.2f", "%g"]) % (rng.random() * 10 ** rng.randrange(0, 12) * rng.choice([1, -1])))
            elif t < 0.8:
                s = "".join(rng.choice("abc XYZ,.-\t") for _ in range(rng.randrange(0, 10)))
                items.append('"%s"' % s)
            else:
                items.append("".join(rng.choice("ab 1.,\t\r\n\"") for _ in range(rng.randrange(0, 8))))
        data = (rng.choice([",", "\r\n", " , "]).join(items) + rng.choice(["\r\n", "", "\n"])).encode()
        ops = "".join(rng.choice("NSLE") for _ in range(rng.randrange(1, 8)))
        out.append((data, ops, rng.choice([4096, 1, 3, 7])))
    return out


def main():
    cmd = sys.argv[1]
    emu = Emu()
    if cmd == "str":
        x = sys.argv[2]
        x80 = hex2x80(x[2:]) if x.startswith("0x") else f64_to_x80(float(x))
        d = int(sys.argv[3]) if len(sys.argv) > 3 else 15
        print(repr(emu.strnum(x80, d, True)), repr(emu.strnum(x80, d, False)), repr(emu.writenum(x80)))
    elif cmd == "val":
        r = emu.val(sys.argv[2].encode())
        f = x80_to_fraction(r)
        print(x80hex(r), float(f) if f is not None else "inf/nan")
    elif cmd == "input":
        data = sys.argv[2].encode().decode("unicode_escape").encode("latin-1")
        print(emu.input_script(data, sys.argv[3], int(sys.argv[4]) if len(sys.argv) > 4 else 4096))
    elif cmd == "fixture":
        gen_fixture(sys.argv[2])
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main()
