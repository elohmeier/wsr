"""Emulation harness for the PowerBASIC 8 RNG in WSR.EXE (RND / RND(a,b) / RANDOMIZE / TIMER).

Runs the original runtime code under Unicorn and writes test vectors for
engine/wsr-core/tests/rng.rs.

Usage (from the repo root):
    uv run -q --with unicorn --with pefile python -I tools/emu/rng.py \
        [extracted/app/WSR.EXE] [engine/wsr-core/tests/fixtures/rng.txt]

Routines exercised (addresses in WSR.EXE, image base 0x400000):
    0x6b735d  runtime init: zero-fills [0x6c3010,0x6c327c), 0xff-fills [0x6c327c,0x6c328c)
              -> RNG state dword 0x6c3280 = 0xffffffff, last value 0x6c327c = 0xffffffff
    0x6bc3d0  PB_Randomize   fstp dword [0x6c3280]  (seed in ST0 rounded to float32 bits)
    0x6bc3d7  PB_Rnd         RndNext; ST0 = fild dword [0x6c327c] * 2^-31
    0x6bc3fe  PB_RndInt      EAX=a, ECX=b -> EAX
    0x6bc473  pbrt_RndNext   state = state*0x08088405 + 1; [0x6c327c] = state >> 1
    0x6bd659  PB_Timer       ST0 = (h*3600000 + m*60000 + s*1000 + ms) / 1000 (GetLocalTime stubbed)

Fixture lines (all numbers hex unless noted, f64 values as IEEE bit patterns):
    D <n> <v1> .. <vn> <state>         RND x n from the startup state (runs 0x6b735d first)
    R <seed80> <st0> <n> <v1>..<vn> <state>
                                       RANDOMIZE seed (80-bit bits, 20 hex digits; the value is exactly an
                                       f64), state right after RANDOMIZE, then RND x n, final state
    I <state> <a> <b> <res> <state'>   a, b, res as decimal i32: set state, RND(a, b)
    T <h> <m> <s> <ms> <timer80> <state>
                                       decimal time; TIMER value (80-bit bits, 20 hex digits), state after
                                       RANDOMIZE TIMER
"""

import random
import struct
import sys

import pefile
from unicorn import UC_ARCH_X86, UC_HOOK_CODE, UC_MODE_32, Uc
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_EBP, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP

BASE = 0x400000
STUB = 0x10000000  # import stubs
CODE = 0x11000000  # generated driver code
DATA = 0x12000000  # scratch data (inputs / outputs)
STACK = 0x13000000
STACK_SIZE = 0x100000

RANDOMIZE = 0x6BC3D0
RND = 0x6BC3D7
RNDINT = 0x6BC3FE
TIMER = 0x6BD659
INIT = 0x6B735D
CW_DEFAULT = 0x6B713C  # 0x133f: round-nearest, 64-bit precision, all exceptions masked
STATE = 0x6C3280


class Emu:
    def __init__(self, exe):
        pe = pefile.PE(exe)
        self.pe = pe
        uc = Uc(UC_ARCH_X86, UC_MODE_32)
        self.uc = uc
        size = (pe.OPTIONAL_HEADER.SizeOfImage + 0xFFFF) & ~0xFFFF
        uc.mem_map(BASE, size)
        uc.mem_write(BASE, pe.header)
        for s in pe.sections:
            uc.mem_write(BASE + s.VirtualAddress, s.get_data())
        # .data startup relocation: runtime moves 0x9b4a8 bytes from 0x6c3028 to 0x6c3f18.
        blob = bytes(uc.mem_read(0x6C3028, 0x9B4A8))
        uc.mem_write(0x6C3F18, blob)
        for base, sz in ((STUB, 0x10000), (CODE, 0x1000000), (DATA, 0x100000), (STACK, STACK_SIZE)):
            uc.mem_map(base, sz)
        # IAT stubs
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
        self.systemtime = (2026, 1, 1, 1, 0, 0, 0, 0)
        self.code_next = CODE

    def _stub_hook(self, uc, address, size, _):
        name = self.stubs.get(address)
        esp = uc.reg_read(UC_X86_REG_ESP)
        ret = struct.unpack("<I", uc.mem_read(esp, 4))[0]
        args = lambda n: struct.unpack(f"<{n}I", uc.mem_read(esp + 4, 4 * n))
        if name == "GetLocalTime":
            (p,) = args(1)
            uc.mem_write(p, struct.pack("<8H", *self.systemtime))
            nargs, rv = 1, 0
        elif name == "SetErrorMode":
            nargs, rv = 1, 0
        elif name == "SetLastError":
            nargs, rv = 1, 0
        elif name == "GetLastError":
            nargs, rv = 0, 0
        else:
            raise RuntimeError(f"unstubbed import {name}")
        uc.reg_write(UC_X86_REG_EAX, rv)
        uc.reg_write(UC_X86_REG_ESP, esp + 4 * nargs)
        uc.mem_write(esp + 4 * nargs, struct.pack("<I", ret))
        # the stub's 'ret' now pops the return address we moved up

    def run(self, asm):
        """Run generated code with a fresh stack and a fake PB frame (EBP).

        Every run uses fresh code addresses (Unicorn may keep stale translated blocks otherwise)."""
        code = bytes(asm.b)
        self.uc.mem_write(asm.base, code)
        self.uc.reg_write(UC_X86_REG_ESP, STACK + STACK_SIZE - 0x1000)
        self.uc.reg_write(UC_X86_REG_EBP, STACK + STACK_SIZE - 0x800)
        self.uc.emu_start(asm.base, asm.base + len(code))

    def asm(self):
        if self.code_next + 0x10000 > CODE + 0x1000000:
            self.code_next = CODE
            self.uc.ctl_remove_cache(CODE, CODE + 0x1000000)
        a = Asm(self.code_next)
        self.code_next += 0x10000
        return a

    def u32(self, a):
        return struct.unpack("<I", self.uc.mem_read(a, 4))[0]

    def i32(self, a):
        return struct.unpack("<i", self.uc.mem_read(a, 4))[0]

    def ext(self, a):
        return bytes(self.uc.mem_read(a, 10))


def call(target, at):
    return b"\xe8" + struct.pack("<i", target - (at + 5))


class Asm:
    def __init__(self, base):
        self.base = base
        self.b = bytearray(b"\xd9\x2d" + struct.pack("<I", CW_DEFAULT))  # fldcw [CW_DEFAULT]

    @property
    def pc(self):
        return self.base + len(self.b)

    def call(self, t):
        self.b += call(t, self.pc)

    def fld_ext(self, a):
        self.b += b"\xdb\x2d" + struct.pack("<I", a)

    def fstp_ext(self, a):
        self.b += b"\xdb\x3d" + struct.pack("<I", a)

    def mov_eax(self, v):
        self.b += b"\xb8" + struct.pack("<I", v & 0xFFFFFFFF)

    def mov_ecx(self, v):
        self.b += b"\xb9" + struct.pack("<I", v & 0xFFFFFFFF)

    def store_eax(self, a):
        self.b += b"\xa3" + struct.pack("<I", a)

    def load_eax(self, a):
        self.b += b"\xa1" + struct.pack("<I", a)

    def mov_mem(self, a, v):
        self.b += b"\xc7\x05" + struct.pack("<II", a, v & 0xFFFFFFFF)


def f64_to_ext(x):
    """Exact conversion of a finite f64 to 80-bit extended bytes."""
    b = struct.unpack("<Q", struct.pack("<d", x))[0]
    sign = b >> 63
    e = (b >> 52) & 0x7FF
    m = b & ((1 << 52) - 1)
    if e == 0 and m == 0:
        return struct.pack("<QH", 0, sign << 15)
    if e == 0x7FF:
        return struct.pack("<QH", (1 << 63) | (m << 11), (sign << 15) | 0x7FFF)
    if e == 0:  # subnormal
        e = 1
        while not (m >> 52):
            m <<= 1
            e -= 1
    else:
        m |= 1 << 52
    return struct.pack("<QH", m << 11, (sign << 15) | (e - 1023 + 16383))


def ext_to_f64(eb):
    """Exact conversion of 80-bit bytes to f64 (asserts no precision loss)."""
    mant, se = struct.unpack("<QH", eb)
    sign = -1.0 if se >> 15 else 1.0
    e = se & 0x7FFF
    if mant == 0:
        return sign * 0.0
    v = sign * mant * 2.0 ** (e - 16383 - 63)
    assert int(abs(v) / 2.0 ** (e - 16383 - 63)) == mant if mant < (1 << 53) else True
    assert f64_to_ext(v) == eb, (eb.hex(), v)
    return v


def d2h(x):
    return struct.unpack("<Q", struct.pack("<d", x))[0]


def rnd_seq(emu, asm, n, out):
    for i in range(n):
        asm.call(RND)
        asm.fstp_ext(out + 10 * i)


def main():
    exe = sys.argv[1] if len(sys.argv) > 1 else "extracted/app/WSR.EXE"
    outp = sys.argv[2] if len(sys.argv) > 2 else "engine/wsr-core/tests/fixtures/rng.txt"
    emu = Emu(exe)
    rng = random.Random(1234)
    lines = []
    OUT = DATA + 0x1000

    # Default (startup) state
    n = 200
    a = emu.asm()
    a.call(INIT)
    rnd_seq(emu, a, n, OUT)
    emu.run(a)
    vals = [ext_to_f64(emu.ext(OUT + 10 * i)) for i in range(n)]
    lines.append("D %d %s %08x" % (n, " ".join("%016x" % d2h(v) for v in vals), emu.u32(STATE)))
    assert emu.u32(0x6C327C) != 0xFFFFFFFF

    # RANDOMIZE seeds
    seeds = [0.0, -0.0, 1.0, -1.0, 2.0, 0.5, 0.1, -0.1, 3.14159, 12345.0, 65535.0, 65536.0, 1e6, 1e10,
             -1e10, 1e38, 3.5e38, -3.5e38, 1e300, -1e300, 1e-40, -1e-40, 1e-46, 1e-300, 5e-324,
             16777217.0, 16777219.0, 0.1 + 2**-30, 86399.999, 43200.5, 32500.0, 10.0, 32501.0]
    seeds += [float(rng.randint(0, 86400000)) / 1000.0 for _ in range(150)]
    seeds += [rng.uniform(-1e6, 1e6) for _ in range(80)]
    seeds += [float(rng.randint(-2**31, 2**31)) for _ in range(40)]
    seeds += [struct.unpack("<d", struct.pack("<Q", rng.getrandbits(64) & ~(0x7FF << 52) |
                                              (rng.randint(0x380, 0x47F) << 52)))[0] for _ in range(80)]
    # f32 rounding midpoints / ties
    for _ in range(40):
        f = struct.unpack("<f", struct.pack("<I", rng.getrandbits(31) & 0x7F7FFFFF))[0]
        nxt = struct.unpack("<f", struct.pack("<I", struct.unpack("<I", struct.pack("<f", f))[0] + 1))[0]
        seeds.append((f + nxt) / 2)
    for s in seeds:
        nseq = 8
        a = emu.asm()
        a.call(INIT)
        a.fld_ext(DATA)
        a.call(RANDOMIZE)
        a.load_eax(STATE)
        a.store_eax(DATA + 0x20)
        rnd_seq(emu, a, nseq, OUT)
        emu.uc.mem_write(DATA, f64_to_ext(s))
        try:
            emu.run(a)
        except Exception:
            print("seed", s, hex(emu.uc.reg_read(UC_X86_REG_EIP)))
            raise
        vals = [ext_to_f64(emu.ext(OUT + 10 * i)) for i in range(nseq)]
        lines.append("R %s %08x %d %s %08x" % (f64_to_ext(s)[::-1].hex(), emu.u32(DATA + 0x20), nseq,
                                               " ".join("%016x" % d2h(v) for v in vals), emu.u32(STATE)))

    # RND(a, b)
    pairs = [(1, 6), (6, 1), (0, 0), (5, 5), (-5, 5), (5, -5), (-10, -1), (-1, -10), (1, 100), (100, 1),
             (0, 1), (1, 0), (-2**31, 2**31 - 1), (2**31 - 1, -2**31), (0, 2**31 - 1), (-2**31, 0),
             (-2**31, -2**31), (2**31 - 1, 2**31 - 1), (-1, 0), (0x28, 1), (0x640, 0xB), (0x4E2, 500),
             (0x96, 0x32), (-2**31, 2**31 - 2), (-2**31 + 1, 2**31 - 1), (2**30, -2**30)]
    for _ in range(300):
        pairs.append((rng.randint(-2**31, 2**31 - 1), rng.randint(-2**31, 2**31 - 1)))
    for _ in range(300):
        x = rng.randint(-1000, 1000)
        pairs.append((x, x + rng.randint(-1000, 1000)))
    states = [0xFFFFFFFF, 0, 1, 0xFFFFFFFE, 0x7FFFFFFF, 0x80000000]
    vec = []
    for (pa, pb) in pairs:
        for st in [rng.getrandbits(32), rng.choice(states)]:
            vec.append((st, pa, pb))
    # states that make RndNext yield the extreme n values (n = 0, n = 2^31-1)
    inv = pow(0x08088405, -1, 1 << 32)
    for target in (0, 1, 0xFFFFFFFE, 0xFFFFFFFF, 0x80000000, 0x7FFFFFFF):
        st = ((target - 1) * inv) & 0xFFFFFFFF
        for (pa, pb) in pairs[:26]:
            vec.append((st, pa, pb))
    CH = 400
    for k in range(0, len(vec), CH):
        chunk = vec[k:k + CH]
        a = emu.asm()
        for i, (st, pa, pb) in enumerate(chunk):
            a.mov_mem(STATE, st)
            a.mov_eax(pa)
            a.mov_ecx(pb)
            a.call(RNDINT)
            a.store_eax(OUT + 8 * i)
            a.load_eax(STATE)
            a.store_eax(OUT + 8 * i + 4)
        emu.run(a)
        for i, (st, pa, pb) in enumerate(chunk):
            lines.append("I %08x %d %d %d %08x" % (st, pa, pb, emu.i32(OUT + 8 * i), emu.u32(OUT + 8 * i + 4)))

    # TIMER + RANDOMIZE TIMER
    times = [(0, 0, 0, 0), (23, 59, 59, 999), (12, 0, 0, 0), (0, 0, 0, 1), (0, 0, 0, 125), (0, 0, 0, 100)]
    times += [(rng.randint(0, 23), rng.randint(0, 59), rng.randint(0, 59), rng.randint(0, 999)) for _ in range(300)]
    for (h, m, s, ms) in times:
        emu.systemtime = (2026, 1, 1, 1, h, m, s, ms)
        a = emu.asm()
        a.call(INIT)
        a.call(TIMER)
        a.b += b"\xd9\xc0"  # fld st(0)
        a.fstp_ext(OUT)
        a.call(RANDOMIZE)
        emu.run(a)
        eb = emu.ext(OUT)  # TIMER as 80-bit (64-bit-mantissa quotient, generally not an f64)
        lines.append("T %d %d %d %d %s %08x" % (h, m, s, ms, eb[::-1].hex(), emu.u32(STATE)))

    with open(outp, "w") as f:
        f.write("# PowerBASIC 8 RNG vectors from WSR.EXE (tools/emu/rng.py); see harness for line formats\n")
        f.write("\n".join(lines) + "\n")
    print(f"wrote {len(lines)} vectors to {outp}")


if __name__ == "__main__":
    main()
