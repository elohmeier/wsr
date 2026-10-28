#!/usr/bin/env python3
"""Emulation harness for PowerBASIC 8 FORMAT$(x, mask$) as found in WSR.EXE.

Runs the original runtime routine PB_Format (0x6b9006) under Unicorn and records its output.
PB_Format takes x in ST0 and the mask as the top of the caller's string-expression stack
([ebp-0x78] = index, slot [ebp+idx-0x5c]); it pops the mask and pushes the formatted result.

Usage (run from anywhere):

    uv run -q --with unicorn --with pefile python -I tools/emu/format.py query 1234.5 "* ###,.00"
    uv run -q --with unicorn --with pefile python -I tools/emu/format.py fixture \
        engine/wsr-core/tests/fixtures/format.txt

`stress <path> <n> <seed>` writes n vectors with random masks/values in the fixture format (for
ad-hoc differential testing; point the Rust test at it). `query` prints the result for one value/mask (value may be a decimal literal or 0x<16 hex digits> of
the f64 bits). `fixture` regenerates the fixture used by engine/wsr-core/tests/format.rs.

Fixture format: one vector per line, tab separated: `<f64 bits as 16 hex digits>\t<mask>\t<result>`.
Mask and result are byte strings where `\\`, tab, and bytes outside 0x20..0x7e are written as
`\\xNN`. Lines starting with '#' are comments.

Emulation setup: PE sections mapped at their VAs, .data startup relocation applied (0x9b4a8 bytes
from 0x6c3028 to 0x6c3f18), every IAT slot redirected to a stub; SysAllocStringByteLen allocates
from a bump heap (length dword at ptr-4), SysFreeString is a no-op, anything else aborts. The FPU
control word is set to 0x133f, the value the PB runtime loads at startup (0x6b7339): 64-bit
precision, round to nearest, all exceptions masked.
"""
import random
import struct
import sys
from pathlib import Path

import pefile
from unicorn import UC_ARCH_X86, UC_HOOK_CODE, UC_MODE_32, Uc
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_EBP, UC_X86_REG_EIP, UC_X86_REG_ESP

REPO = Path(__file__).resolve().parents[2]
EXE = REPO / "extracted/app/WSR.EXE"
PB_FORMAT = 0x6B9006
ERROR_EXIT = 0x6B73FE

STUB_BASE = 0x10000000  # one byte per import (hlt), hooked
CODE = 0x10100000  # prologue code
DATA = 0x10101000  # scratch: x as tword, control word
HEAP = 0x20000000
HEAP_SIZE = 0x01000000
STACK_LO = 0x30000000
STACK_SIZE = 0x00400000
FRAME = STACK_LO + STACK_SIZE - 0x10000  # fake caller EBP


class Done(Exception):
    pass


class Emu:
    def __init__(self):
        pe = pefile.PE(str(EXE))
        img = bytearray(pe.get_memory_mapped_image())
        img = img + bytes(pe.OPTIONAL_HEADER.SizeOfImage - len(img))
        base = pe.OPTIONAL_HEADER.ImageBase
        # .data startup relocation (memmove 0x9b4a8 bytes from 0x6c3028 to 0x6c3f18)
        src, dst, n = 0x6C3028 - base, 0x6C3F18 - base, 0x9B4A8
        img[dst : dst + n] = bytes(img[src : src + n])
        self.uc = uc = Uc(UC_ARCH_X86, UC_MODE_32)
        size = (len(img) + 0xFFF) & ~0xFFF
        uc.mem_map(base, size)
        uc.mem_write(base, bytes(img))
        uc.mem_map(STUB_BASE, 0x1000)
        uc.mem_map(CODE, 0x2000)
        uc.mem_map(HEAP, HEAP_SIZE)
        uc.mem_map(STACK_LO, STACK_SIZE)
        self.stubs = {}
        k = 0
        for e in pe.DIRECTORY_ENTRY_IMPORT:
            for i in e.imports:
                name = (i.name or b"ord%d" % i.ordinal).decode()
                addr = STUB_BASE + k
                self.stubs[addr] = name
                uc.mem_write(i.address, struct.pack("<I", addr))
                k += 1
        uc.mem_write(STUB_BASE, b"\xf4" * k)
        # prologue: fldcw [DATA+16]; fld tword [DATA]; call PB_FORMAT; hlt
        code = b"\xd9\x2d" + struct.pack("<I", DATA + 16)
        code += b"\xdb\x2d" + struct.pack("<I", DATA)
        call_at = CODE + len(code)
        code += b"\xe8" + struct.pack("<i", PB_FORMAT - (call_at + 5))
        self.end = CODE + len(code)
        code += b"\xf4"
        uc.mem_write(CODE, code)
        uc.mem_write(DATA + 16, struct.pack("<H", 0x133F))
        uc.hook_add(UC_HOOK_CODE, self._hook_stub, begin=STUB_BASE, end=STUB_BASE + 0xFFF)
        uc.hook_add(UC_HOOK_CODE, self._hook_end, begin=self.end, end=self.end)
        uc.hook_add(UC_HOOK_CODE, self._hook_err, begin=ERROR_EXIT, end=ERROR_EXIT)

    def _hook_end(self, uc, addr, size, ud):
        uc.emu_stop()

    def _hook_err(self, uc, addr, size, ud):
        raise RuntimeError("PB runtime error path reached")

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
        return p

    def _hook_stub(self, uc, addr, size, ud):
        name = self.stubs[addr]
        if name == "SysAllocStringByteLen":
            psz, n = self._pop_args(2)
            data = bytes(uc.mem_read(psz, n)) if psz else bytes(n)
            uc.reg_write(UC_X86_REG_EAX, self.alloc(data))
        elif name == "SysFreeString":
            self._pop_args(1)
        else:
            raise RuntimeError("unhandled import " + name)

    def format80(self, tword: bytes, mask: bytes) -> bytes:
        uc = self.uc
        self.heap = HEAP
        uc.mem_write(DATA, tword)
        ms = self.alloc(mask) if mask else 0
        uc.mem_write(FRAME - 0x78, struct.pack("<I", 0))
        uc.mem_write(FRAME - 0x5C, struct.pack("<I", ms))
        uc.reg_write(UC_X86_REG_EBP, FRAME)
        uc.reg_write(UC_X86_REG_ESP, FRAME - 0x200)
        uc.emu_start(CODE, self.end + 1, count=50_000_000)
        if uc.reg_read(UC_X86_REG_EIP) != self.end:
            raise RuntimeError("did not finish, eip=%x" % uc.reg_read(UC_X86_REG_EIP))
        (idx,) = struct.unpack("<i", uc.mem_read(FRAME - 0x78, 4))
        assert idx == 0, idx
        (p,) = struct.unpack("<I", uc.mem_read(FRAME - 0x5C, 4))
        if p == 0:
            return b""
        (n,) = struct.unpack("<I", uc.mem_read(p - 4, 4))
        return bytes(uc.mem_read(p, n))

    def format(self, x: float, mask: bytes) -> bytes:
        return self.format80(f64_to_x80(x), mask)


def f64_to_x80(x: float) -> bytes:
    (b,) = struct.unpack("<Q", struct.pack("<d", x))
    sign = b >> 63
    e = (b >> 52) & 0x7FF
    m = b & ((1 << 52) - 1)
    if e == 0 and m == 0:
        return struct.pack("<QH", 0, sign << 15)
    if e == 0x7FF:
        return struct.pack("<QH", (1 << 63) | (m << 11), (sign << 15) | 0x7FFF)
    if e == 0:  # subnormal: normalise
        sh = 53 - m.bit_length()
        m <<= sh
        e = 1 - sh
    else:
        m |= 1 << 52
    return struct.pack("<QH", m << 11, (sign << 15) | (e - 1023 + 16383))


def esc(b: bytes) -> str:
    return "".join(chr(c) if 0x20 <= c < 0x7F and c != 0x5C else "\\x%02x" % c for c in b)


def f64bits(x: float) -> str:
    return "%016x" % struct.unpack("<Q", struct.pack("<d", x))[0]


def from_bits(h: str) -> float:
    return struct.unpack("<d", struct.pack("<Q", int(h, 16)))[0]


# Every mask literal used by the game (decompiled/bas/*.c, directly and via variables).
GAME_MASKS = """* ###,|* ###,.00|* ###.00|* ##.00|##.00|* ###|##.0|###.00|* ###.0|0#.00|######,|* ######,|* ##|
* ##########|####|* #################|* #####.00|* ####.00|* ####,|* ##,|##############.00|####.##|###|
* #############,.00|* ######.00|* #####,.00|* ####|* ####,.00|#################|#############,.00|
* ###########.0|* ##########,.00|* ######,.00|###############,.0|############.00|#####|* ##############,|
* #############,|* ###########,|* ###########,.00|* ###,.000|* ###,.0|################,|###############,.00|
######.00|* ############,.00|* ##########,|* #########.0|* #########,|000#|##################,|
################|##############|##############,.00|############|###########,|##########|##########,|
#######,|#####.00|####,|###.0|##|* ################|* ##############,.00|* #############|* ############,|
* ###########.00|* #########|* #########.00|* #######.00|* #######,|* ######|* ######.0|* #####|* #####,|
0#|0.0|#####0.0|#####################,|##################|################,.00|###############|
###############.00|###############,|##############,|#############|#############,|############.0|
###########.0|##########,.00|#########|#########,|########|########.0|######|#####.000|#####,.00|####.00|
####.0|####.#|###,|###,.00|##.###|##.#|#|#.00|* ######################,|* ##################,|
* #################.00|* ############.00|* #########,.##|* ########|* ########,|* #######,.00|* ###.##|
* ###,00|* ###,.##|* ##.0|* ##,.00|* ***,| * ###| * ###,|* ##############.00|* #################,|
* #############.00|#################.00|* ###########|* #############0.00|#################,.00|
#################,"""
GAME_MASKS = [m for m in "".join(GAME_MASKS.split("\n")).split("|")]

# Extra masks exercising features the game does not use (documented semantics, not fixture-critical).
EXTRA_MASKS = [
    "$#,##0", "$#,##0.00", "0.00%", "#,###.00", "#,##0", "0", "0.00", "#.##", ".##", "#", "00000",
    "#,###,###", "0.0E+00", "##.##E-0", "+#.##", "-#.##", "(#.##)", "#.##;(#.##)", "#.##;(#.##);\"zero\"",
    "\"abc\"#.#", "\\X#.#", "*-#####", "*0###", "#,##0.00 \"DM\"", "##%", "#.#%", "x#y", "", "  #",
]


def gen_values(rng):
    vals = [0.0, -0.0, 1.0, -1.0, 0.5, -0.5, 1.5, 2.5, -2.5, 0.05, 0.15, 0.25, 0.35, 0.045, 0.005, 0.015,
            0.125, 0.375, 1e-5, 9.995, 99.5, 999.5, 9999.5, 0.95, 0.995, 0.9995, 123.456, -123.456,
            1234.5678, -1234.5678, 1e6, -1e6, 1234567.891, 12345678.9, 1e9, 1e12, 1e15, 1e16, 1e17,
            1e18, 1e19, 1e20, 1e22, 1e30, 1e100, 1e300, -1e300, 999999999999999.0, 9999999999999999.0,
            123456789012345678.0, 2.0**53, 2.0**63, 2.0**64, 5e-324, 1e-300, 0.1, 0.2, 0.3, 0.7, 1.005,
            2.675, 1.115, 10.0, 100.0, 1000.0, 10000.0, 99999.0, 100000.0, 0.0001, 0.00049, 0.0005,
            0.00051, 0.049, 0.051, 4.5, 5.5, 6.5, 0.45, 0.55, 0.65, 1.25, 1.35, 12.5, 13.5, 125.0,
            1250.0, 0.9, 0.09, 0.009, 1.0 / 3, 2.0 / 3, -1.0 / 3, 3.14159265358979, 2.718281828459045,
            float(10**15 - 1), float(10**15 + 1), 123456.0, 1234567.0, 12345678.0, 123456789.0,
            float("inf"), float("-inf"), float("nan")]
    for k in range(-6, 23):
        p = 10.0 ** k
        vals += [p, -p, p * 1.5, p * 9.999, p * 0.9999999]
    for _ in range(25):
        vals.append(rng.uniform(-1, 1) * 10 ** rng.uniform(-4, 20))
    for _ in range(20):  # exact halves at various decimal positions
        d = rng.randint(0, 3)
        vals.append(rng.choice([-1, 1]) * (rng.randint(0, 10**6) * 10 + 5) / 10 ** (d + 1))
    for _ in range(10):
        vals.append(float(rng.randint(-10**9, 10**9)))
    out, seen = [], set()
    for v in vals:
        k = f64bits(v)
        if k not in seen:
            seen.add(k)
            out.append(v)
    return out


def write_fixture(path):
    emu = Emu()
    rng = random.Random(8)
    lines = ["# FORMAT$(x, mask) vectors from WSR.EXE PB_Format 0x6b9006 (tools/emu/format.py)",
             "# f64-bits<TAB>mask<TAB>result"]
    for mask in GAME_MASKS + EXTRA_MASKS:
        mb = mask.encode()
        vals = gen_values(rng)
        for v in vals:
            r = emu.format(v, mb)
            lines.append("%s\t%s\t%s" % (f64bits(v), esc(mb), esc(r)))
    # random f64 bit patterns over the whole range through masks exposing all 18 digits / STR$ path
    for i in range(3000):
        b = rng.getrandbits(64)
        if i % 2:
            b = (b & 0x800FFFFFFFFFFFFF) | (rng.randint(0x3B0, 0x450) << 52)
        v = from_bits("%016x" % b)
        if v != v:
            continue
        mb = [b"0.00000000000000000E+000", b"", b"#.##", b"* ###,.00"][i % 4]
        lines.append("%s\t%s\t%s" % ("%016x" % b, esc(mb), esc(emu.format(v, mb))))
    # random masks (all mask features, odd combinations)
    for _ in range(3000):
        mb, v = rand_mask(rng), rand_value(rng)
        lines.append("%s\t%s\t%s" % (f64bits(v), esc(mb), esc(emu.format(v, mb))))
    Path(path).write_text("\n".join(lines) + "\n")
    print("wrote", len(lines) - 2, "vectors to", path)


def rand_mask(rng):
    alpha = ["#"] * 8 + ["0"] * 4 + [",", ",", ".", ".", "%", "*", "* ", "*0", "E+", "E-", "e+", "E", "\"", ";",
             "\\", "$", "(", ")", "-", "+", " ", "x", "**", "#,", "0,", ";;"]
    return "".join(rng.choice(alpha) for _ in range(rng.randint(1, 14))).encode()


def rand_value(rng):
    r = rng.random()
    if r < 0.3:
        return rng.choice([-1, 1]) * rng.randint(0, 10**rng.randint(1, 18)) / 10 ** rng.randint(0, 8)
    if r < 0.4:
        return rng.choice([-1, 1]) * (rng.randint(0, 10**rng.randint(1, 8)) * 10 + 5) / 10 ** rng.randint(1, 9)
    if r < 0.5:
        return float(rng.choice([0, 1, -1, 0.5, -0.5, 1e15, 1e16, 999999999999999.5, 0.0001, 0.00001]))
    return rng.choice([-1, 1]) * 10 ** rng.uniform(-12, 25)


def write_stress(path, n, seed):
    """Random masks x random values (for ad-hoc differential testing)."""
    emu = Emu()
    rng = random.Random(seed)
    lines = []
    for _ in range(n):
        m, v = rand_mask(rng), rand_value(rng)
        try:
            r = emu.format(v, m)
        except Exception as e:  # original crashes / loops on some degenerate masks
            print("skip", m, v, e, file=sys.stderr)
            emu = Emu()
            continue
        lines.append("%s\t%s\t%s" % (f64bits(v), esc(m), esc(r)))
    Path(path).write_text("\n".join(lines) + "\n")


def main():
    if len(sys.argv) >= 2 and sys.argv[1] == "query":
        emu = Emu()
        s = sys.argv[2]
        x = from_bits(s[2:]) if s.startswith("0x") and len(s) == 18 else float(s)
        for m in sys.argv[3:]:
            print(repr(emu.format(x, m.encode()).decode("latin-1")))
    elif len(sys.argv) == 5 and sys.argv[1] == "stress":
        write_stress(sys.argv[2], int(sys.argv[3]), int(sys.argv[4]))
    elif len(sys.argv) == 3 and sys.argv[1] == "fixture":
        write_fixture(sys.argv[2])
    else:
        print(__doc__)


if __name__ == "__main__":
    main()
