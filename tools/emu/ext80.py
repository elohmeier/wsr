#!/usr/bin/env python3
"""Test vectors for the x87 80-bit soft-float model `pb::ext80::Ext80` (engine/wsr-core/src/pb/ext80.rs).

Runs single x87 instructions under Unicorn (QEMU's softfloat x87 model) with the PB runtime's FPU
control word 0x133f (all exceptions masked, 64-bit precision, round to nearest even) and records
operands and results.

Usage:

    uv run -q --with unicorn python -I tools/emu/ext80.py fixture engine/wsr-core/tests/fixtures/ext80.txt

Fixture format (tab separated, '#' comments). x80 = 80-bit value as 20 hex digits, big-endian
(sign/exponent word first, then the 64-bit mantissa), as in tools/emu/numfmt.py.
  A <x80 a> <x80 b> <x80 r>    FADD   r = a + b
  S <x80 a> <x80 b> <x80 r>    FSUB   r = a - b
  M <x80 a> <x80 b> <x80 r>    FMUL   r = a * b
  D <x80 a> <x80 b> <x80 r>    FDIV   r = a / b
  R <x80 a> <x80 r>            FRNDINT
  Q <x80 a> <int>              FISTP qword (decimal)
  L <x80 a> <int>              FISTP dword (decimal)
  F <x80 a> <hex>              FSTP qword (f64 bits, 16 hex digits)
  G <x80 a> <hex>              FSTP dword (f32 bits, 8 hex digits)
  C <x80 a> <x80 b> <lt|eq|gt> FCOM a, b
"""
import random
import struct
import sys

from unicorn import UC_ARCH_X86, UC_MODE_32, Uc
from unicorn.x86_const import UC_X86_REG_AX

CODE = 0x100000
DATA = 0x200000  # +0 a, +0x10 b, +0x20 result, +0x30 control word
A, B, R, CW = DATA, DATA + 0x10, DATA + 0x20, DATA + 0x30


def m(op, addr):
    """x87 instruction with a [disp32] memory operand (mod=00, rm=101)."""
    opcode, reg = op
    return bytes([opcode, (reg << 3) | 5]) + struct.pack("<I", addr)


FLD_T = (0xDB, 5)
FSTP_T = (0xDB, 7)
FLDCW = (0xD9, 5)
PRE = bytes([0xDB, 0xE3]) + m(FLDCW, CW) + m(FLD_T, A)  # fninit; fldcw; fld a
BIN = PRE + m(FLD_T, B)
OPS = {
    "A": BIN + bytes([0xDE, 0xC1]) + m(FSTP_T, R),  # faddp st1 = a + b
    "S": BIN + bytes([0xDE, 0xE9]) + m(FSTP_T, R),  # fsubp st1 = a - b
    "M": BIN + bytes([0xDE, 0xC9]) + m(FSTP_T, R),  # fmulp
    "D": BIN + bytes([0xDE, 0xF9]) + m(FSTP_T, R),  # fdivp st1 = a / b
    "T": PRE + bytes([0xD9, 0xFA]) + m(FSTP_T, R),  # fsqrt
    "R": PRE + bytes([0xD9, 0xFC]) + m(FSTP_T, R),  # frndint
    "Q": PRE + m((0xDF, 7), R),  # fistp qword
    "L": PRE + m((0xDB, 3), R),  # fistp dword
    "F": PRE + m((0xDD, 3), R),  # fstp qword
    "G": PRE + m((0xD9, 3), R),  # fstp dword
    # fld b; fld a; fcompp (compares st0 = a with st1 = b); fnstsw ax
    "C": bytes([0xDB, 0xE3]) + m(FLDCW, CW) + m(FLD_T, B) + m(FLD_T, A) + bytes([0xDE, 0xD9, 0xDF, 0xE0]),
}


class Fpu:
    def __init__(self):
        self.uc = Uc(UC_ARCH_X86, UC_MODE_32)
        self.uc.mem_map(CODE, 0x1000)
        self.uc.mem_map(DATA, 0x1000)
        self.uc.mem_write(CW, struct.pack("<H", 0x133F))

    def run(self, op, a, b=None):
        code = OPS[op]
        self.uc.mem_write(CODE, code)
        self.uc.mem_write(A, pack(a))
        if b is not None:
            self.uc.mem_write(B, pack(b))
        self.uc.mem_write(R, bytes(16))
        self.uc.emu_start(CODE, CODE + len(code))
        r = bytes(self.uc.mem_read(R, 10))
        if op in "ASMDRT":
            return x80(unpack(r))
        if op == "Q":
            return str(struct.unpack("<q", r[:8])[0])
        if op == "L":
            return str(struct.unpack("<i", r[:4])[0])
        if op == "F":
            return r[:8][::-1].hex()
        if op == "G":
            return r[:4][::-1].hex()
        sw = self.uc.reg_read(UC_X86_REG_AX)
        c0, c3 = sw >> 8 & 1, sw >> 14 & 1
        return "eq" if c3 else ("lt" if c0 else "gt")


def pack(v):
    se, mant = v
    return struct.pack("<QH", mant, se)


def unpack(b):
    mant, se = struct.unpack("<QH", b[:10])
    return se, mant


def x80(v):
    return f"{v[0]:04x}{v[1]:016x}"


def from_f64(x):
    b = struct.unpack("<Q", struct.pack("<d", x))[0]
    sign = (b >> 63) << 15
    e = (b >> 52) & 0x7FF
    f = b & ((1 << 52) - 1)
    if e == 0 and f == 0:
        return sign, 0
    if e == 0x7FF:
        return sign | 0x7FFF, (1 << 63) | (f << 11)
    if e == 0:
        sh = 53 - f.bit_length()
        f <<= sh
        e = 1 - sh
    else:
        f |= 1 << 52
    return sign | (e - 1023 + 16383), f << 11


def rnd_value(r):
    """A random operand: mostly game-like decimals, plus full-mantissa, extreme and special values."""
    k = r.random()
    if k < 0.25:  # cents / prices
        x = r.randrange(-10**9, 10**9) / r.choice([1, 100, 1000, 10000])
        return from_f64(x)
    if k < 0.35:  # ties of FISTP after scaling
        return from_f64(r.randrange(-10**6, 10**6) + r.choice([0.5, 0.25, 0.005, 0.015]))
    if k < 0.55:  # random f64 of any moderate magnitude
        return from_f64(r.uniform(-1, 1) * 10 ** r.randint(-20, 20))
    if k < 0.8:  # full 64-bit mantissa, moderate exponent
        return (r.choice([0, 0x8000]) | (16383 + r.randint(-70, 70))), (1 << 63) | r.getrandbits(63)
    if k < 0.85:  # integers
        return from_f64(float(r.randint(-(2**40), 2**40)))
    if k < 0.9:  # huge / tiny (overflow, underflow, denormals)
        e = r.choice([r.randint(1, 80), r.randint(0x7FFE - 80, 0x7FFE), 0])
        mant = r.getrandbits(63) | (0 if e == 0 else 1 << 63)  # no pseudo-denormals (never produced by the x87)
        return r.choice([0, 0x8000]) | e, mant
    if k < 0.95:  # zeros, ones, infinities
        return r.choice([(0, 0), (0x8000, 0), (0x3FFF, 1 << 63), (0xBFFF, 1 << 63), (0x7FFF, 1 << 63), (0xFFFF, 1 << 63)])
    # values just around integers / halves
    base = r.randint(-(2**20), 2**20)
    se, mant = from_f64(float(base) + 0.5 if r.random() < 0.5 else float(base) or 1.0)
    return se, (mant + r.randint(-3, 3)) & ((1 << 64) - 1) | (1 << 63)


def related(r, a):
    """An operand near `a` (cancellation in FSUB/FADD)."""
    se, mant = a
    if se & 0x7FFF in (0, 0x7FFF):
        return rnd_value(r)
    k = r.random()
    if k < 0.3:
        return se ^ 0x8000, mant
    if k < 0.6:
        return se ^ (0x8000 * r.randint(0, 1)), ((mant + r.randint(-1000, 1000)) & ((1 << 64) - 1)) | (1 << 63)
    e = max(1, min(0x7FFE, (se & 0x7FFF) + r.randint(-70, 70)))
    return (se & 0x8000) ^ (0x8000 * r.randint(0, 1)) | e, r.getrandbits(63) | (1 << 63)


def fixture(path, n=4000, seed=80):
    r = random.Random(seed)
    fpu = Fpu()
    lines = ["# generated by tools/emu/ext80.py fixture (see header there for the format)"]
    for op in "ASMD":
        for _ in range(n):
            a = rnd_value(r)
            b = related(r, a) if op in "AS" and r.random() < 0.4 else rnd_value(r)
            lines.append(f"{op}\t{x80(a)}\t{x80(b)}\t{fpu.run(op, a, b)}")
    for op in "RQLFG":
        for _ in range(n):
            a = rnd_value(r)
            lines.append(f"{op}\t{x80(a)}\t{fpu.run(op, a)}")
    for _ in range(n):
        a = rnd_value(r)
        b = a if r.random() < 0.1 else (related(r, a) if r.random() < 0.5 else rnd_value(r))
        lines.append(f"C\t{x80(a)}\t{x80(b)}\t{fpu.run('C', a, b)}")
    with open(path, "w") as f:
        f.write("\n".join(lines) + "\n")
    print(f"{len(lines) - 1} vectors -> {path}")


def real_fixture(path, n=2500, seed=0x8033):
    """FSQRT vectors. FPREM is tested with integer identities in Rust: Unicorn's
    implementation narrows both operands and its result to Double (fpu_helper.c).
    """
    r = random.Random(seed)
    fpu = Fpu()
    lines = ["# generated by tools/emu/ext80.py real-fixture; FSQRT, full EXT operands"]
    for _ in range(n):
        a = rnd_value(r)
        lines.append(f"T\t{x80(a)}\t{fpu.run('T', a)}")
    with open(path, "w") as f:
        f.write("\n".join(lines) + "\n")
    print(f"{n} vectors -> {path}")


def power_fixture(path, n=512, seed=0x80F):
    """Independent reference for the transcendental kernel: 100 decimal digits.
    Unicorn's FYL2X/F2XM1 implementations use host Double, so they cannot check
    extended precision. Results here are rounded once to the nearest EXT.
    """
    from decimal import Decimal as D, localcontext, ROUND_HALF_EVEN
    r = random.Random(seed)
    with localcontext() as ctx:
        ctx.prec = 100
        two = D(2)
        def decimal(v):
            se, mant = v
            return D(mant) * two ** ((se & 0x7fff) - 16383 - 63) * (-1 if se & 0x8000 else 1)
        def rounded(value):
            e = int(value.ln() / two.ln())
            m = value / two ** e
            if m < 1: e -= 1; m *= 2
            mant = int((m * two ** 63).to_integral_value(rounding=ROUND_HALF_EVEN))
            if mant == 1 << 64: e += 1; mant >>= 1
            return e + 16383, mant
        lines = ["# tools/emu/ext80.py power-fixture; Decimal(100), nearest-even EXT"]
        for _ in range(n):
            a = (16383 + r.randint(-3, 3), (1 << 63) | r.getrandbits(63))
            b = ((0x8000 if r.randrange(2) else 0) | (16383 + r.randint(-3, 3)), (1 << 63) | r.getrandbits(63))
            result = (decimal(a).ln() * decimal(b)).exp()
            lines.append(f"{x80(a)}\t{x80(b)}\t{x80(rounded(result))}")
    with open(path, "w") as f:
        f.write("\n".join(lines) + "\n")
    print(f"{n} vectors -> {path}")


if __name__ == "__main__":
    if len(sys.argv) >= 3 and sys.argv[1] == "fixture":
        fixture(sys.argv[2])
    elif len(sys.argv) >= 3 and sys.argv[1] == "real-fixture":
        real_fixture(sys.argv[2])
    elif len(sys.argv) >= 3 and sys.argv[1] == "power-fixture":
        power_fixture(sys.argv[2])
    elif len(sys.argv) >= 3:
        op = sys.argv[1]
        args = [(int(h[:4], 16), int(h[4:], 16)) for h in sys.argv[2:4]]
        print(Fpu().run(op, *args))
    else:
        print(__doc__)
