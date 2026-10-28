"""Differential harness: run an ORIGINAL WSR.EXE game routine on a given state under Unicorn.

The global state is exchanged as the JSON snapshot of `wsr_core::state::Globals::to_json` (keys = original
hex addresses, see engine/wsr-core/src/snapshot.rs). The field list (name, address, PB type, DIM bounds) is
parsed from the doc comments of the generated engine/wsr-core/src/state.rs, so harness and Rust always
agree on the layout. Rust side: engine/wsr-core/tests/diff/mod.rs (`run_original`, `assert_same`).

CLI (from the repo root; `uv run -q --with unicorn --with pefile --with capstone python -I tools/emu/game.py`):

    game.py run  ADDR [--state in.json] [--arg SPEC]... [--answer A]... [--hook ADDR]... [--rng N]
                      [--out out.json] [--log log.txt] [--result meta.json] [--timeout SEC] [--budget N]
    game.py diff ADDR [state.json] [same options]   # print changed fields by name, then the UI/API log
    game.py dump [--out state.json]                 # the state right after boot (see below)

Python API:  run(addr, args, state_json, answers, hooks=(), rng=None)
             -> {state_json, state, ui_log, return_value, err, args_out, regs, elapsed, warnings, rng}

How a run works (~0.45 s per process, of which the routine itself is usually a few 10 ms):
  * Boot: the PE sections are mapped at their VAs and the real startup code is emulated: the PB runtime
    init from the entry point 0x6b71be (its .data relocation 0x6c3028 -> 0x6c3f18, BSS/runtime-state
    zeroing, TLS error block, FPU control word 0x133f), then WinMain/PBMAIN 0x40148f up to 0x4023e0, i.e.
    through PBMAIN's block of global DIMs (all arrays get their original descriptors and bounds) and
    InitCommonControlsEx, just before the first game-initialisation routine.
  * Imports: every IAT slot points at a stub address; a code hook implements the stub in Python, logs
    the call and pops the stdcall arguments (counts in NARGS). Strings and arrays live on an emulator
    heap (`Heap`: 8-byte header [capacity][BSTR length], free lists) shared by SysAllocStringByteLen /
    SysFreeString / SysStringByteLen and GlobalAlloc / GlobalReAlloc / GlobalFree (PB array data).
    MessageBoxA pops the next scripted answer (default IDYES for yes/no boxes, else IDOK) and logs the
    text; INPUTBOX$ (DialogBoxIndirectParamA with the runtime's dialog proc 0x6b82bb) returns the next
    answer as the entered text; windows and dialogs go to the window manager (below); GetLocalTime is fixed (Mon 2000-01-03 10:00:00); file functions go to the virtual filesystem; GDI
    functions return fake handles / 1; anything else is logged and returns 0.
  * State in: scalars are written at their address (EXT as 80-bit from f64, SINGLE f32, CUR/CUX raw
    int64), strings as heap BSTRs (pointer stored at the global, "" = null pointer), arrays as a
    dimensioned PB descriptor at the global's address (as PB_DimArray 0x6b79ed builds it: +0 data,
    +4 flags=4 (dimensioned), +5 =2, +6 type, +7 ndims, +8 count, +0xc bytes, +0x14 elem size, dim d
    (1-based): lo +0xc+12d, hi +0x10+12d, product of the counts of dims < d at +0x14+12(d-1); PB_ArrayElem
    0x6bc34a reads exactly these) pointing at a fresh data block (string arrays: BSTR pointers). Keys
    missing from the input keep their boot value.
  * Call: from a trampoline (fninit; fldcw 0x133f; jmp ADDR), stdcall with the given arguments (default
    BYREF LONG cells — PB passes most params BYREF), returning to a `hlt` sentinel (0x0f000080; calling
    the sentinel itself runs no game code, which the self-test uses). The routine's own prologue builds
    its PB frame (PB_OnErrorSet/PB_ErrCheck/PB_FrameRelease only use that frame and the runtime state
    set up at boot). PB runtime errors (0x6b73fe / 0x6b7406) are logged as "PBERR n in 0xFUNC" and
    execution continues as in the original (ON ERROR handler or error state).
  * Faults are reported with registers and a disassembly of the last basic blocks (the run is repeated
    deterministically with a block trace to get them); timeouts (default 60 s) / instruction budget
    (default off: counting slows emulation ~4x) with the stop address.
  * State out: every global is read back into a snapshot of exactly the input format/keys (same
    escaping as the Rust writer). Arrays whose bounds the routine changed (REDIM) are mapped by
    subscript into the declared bounds, with a warning.

Window manager (`WindowManager`, PB DDT dialogs): CreateDialogIndirectParamA / CreateWindowExA /
SetWindowTextA / SendMessageA (LB_/CB_ ADDSTRING, INSERTSTRING, DELETESTRING, RESETCONTENT, GETCOUNT,
GETTEXT, CUR/ITEMDATA, WM_SET/GETTEXT, BM_SET/GETCHECK) / GetDlgItem / Get/SetWindowLongA / ShowWindow /
DestroyWindow / EndDialog / IsWindow ... keep a model of every dialog and control (class, id, text, list
items, selection), logged as "DIALOG NEW", "CONTROL ADD", "SET TEXT", "DIALOG SHOW", "DIALOG END" and
returned as result["dialogs"]. Dialog procedures are called back into emulated code (WM_INITDIALOG on
creation, every message sent/dispatched to a dialog, WM_DESTROY): API implementations are generators that
yield (proc, args); the stub hook makes the stub's `ret` jump to the callback with CBRET as its return
address and resumes the generator when it returns (nesting works). Modal dialogs (PB's DIALOG SHOW MODAL
loop polls PeekMessageA until IsWindow fails; DialogBoxIndirectParamA): when the queue is empty, the next
scripted action is posted to the newest shown dialog: answers "click:ID" (WM_COMMAND/BN_CLICKED),
"select:ID:N" / "dblclick:ID:N" (list selection + LBN_SELCHANGE / LBN_DBLCLK), "close" (WM_CLOSE),
"sysclose" (title-bar WM_SYSCOMMAND/SC_CLOSE), "end" (destroy),
"text:ID:value" (set a control's text first); default: click button 1 (IDOK) or the
BS_DEFPUSHBUTTON, else WM_CLOSE. After 5 actions without the dialog going away it is destroyed ("FORCED
END"). MSGBOX/INPUTBOX answers and dialog actions share the answers list but each only takes its kind.
Return values: EAX, EAX as a string if it points to a live BSTR (FUNCTION AS STRING: return_str), ST0
if the FPU stack is not empty at return (the sentinel stores fnstsw + fstp tword: return_float,
return_float_raw = 80-bit as 20 hex digits).
x87 PE flag: Unicorn never sets the precision exception (FPSW bit 5); every frndint is hooked to set it
when the value changed (PB_Pow 0x6bcaf8 tests it to choose its integer-exponent path; the only PE reader
in WSR.EXE).

Virtual filesystem (`VFS`): CreateFileA/ReadFile/WriteFile/SetFilePointer/SetEndOfFile/CloseHandle/
GetFileType/DeleteFileA/SetFileAttributesA/FindFirstFileA/FindNextFileA/FindClose (all file APIs WSR.EXE
imports; there is no GetFileSize/GetFileAttributesA/MoveFileA import) work on an in-memory {name: bytes}
keyed by upper-case base name (so DataPath prefixes and case don't matter). Inputs: `files` (CLI --file
NAME=PATH or --files JSON {name: base64}); names not given are read (lazily, as copies) from
extracted/app/ unless default_files=False / --no-default-files. Outputs: every file written, created or
deleted (None) in result["files"]; CLI: meta "files" (base64 / null), "files_written", "files_deleted",
and --files-out DIR receives the written files under their upper-case base names.
--skip ADDR[:NARGS] makes a game routine return 0 at once (stdcall, NARGS dword args), logged as
"SKIP Name", e.g. for routines the port doesn't implement yet.

Arg specs (CLI --arg / Python tuples): `5` = BYREF LONG (python int), `v:5` BYVAL dword, `i:5` BYREF
INTEGER, `q:5` BYREF QUAD, `e:1.5` BYREF EXT, `d:1.5` BYREF DOUBLE, `f:1.5` BYREF SINGLE, `s:text` BYREF
STRING, `vs:text` BYVAL STRING (pointer). `e:x87:SSSSMMMMMMMMMMMMMMMM` passes exact EXT bits. Python: int or (kind, value) with the same kind letters.
Final values of BYREF cells are returned as args_out.
Answers (one queue, in call order): for MSGBOX IDOK IDCANCEL IDABORT IDRETRY IDIGNORE IDYES IDNO or a
number; for INPUTBOX$ the text entered ("" = Cancel / empty).
--hook ADDR logs every call of that game routine as "CALL Name(ptr->value, ...) [from 0x..]" (first four
stack args, dereferenced as LONG), e.g. to check calls of routines the port doesn't implement yet.
--rng N sets the PB RND state dword 0x6c3280 (not part of the snapshot) before the call; the final state
is returned as `rng`.
"""

import argparse
import base64
import json
import math
import os
from pathlib import Path
import re
import struct
import sys
import time
import types
from fractions import Fraction

import pefile
from unicorn import (
    UC_ARCH_X86,
    UC_HOOK_BLOCK,
    UC_HOOK_CODE,
    UC_HOOK_MEM_INVALID,
    UC_MODE_32,
    Uc,
    UcError,
)
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
    UC_X86_REG_FP0,
    UC_X86_REG_FPSW,
    UC_X86_REG_ESP,
)

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DEFAULT_EXE = os.path.join(ROOT, "extracted", "app", "WSR.EXE")
STATE_RS = os.path.join(ROOT, "engine", "wsr-core", "src", "state.rs")

BASE = 0x400000
STUB = 0x10000000
TRAMP = 0x0F000000  # trampoline; SENTINEL = return address
SENTINEL = TRAMP + 0x80
CW_ADDR = TRAMP + 0x100
HEAP = 0x20000000
HEAP_SIZE = 0x10000000
STACK = 0x30000000
STACK_SIZE = 0x200000
RNG_STATE = 0x6C3280  # PB RND state dword (see tools/emu/rng.py); not part of the Globals snapshot
CBRET = STUB + 0xFFF0  # return address of callbacks into emulated code (dialog procedures)
FAKE_WNDPROC = STUB + 0xFFE0  # "previous window procedure" handed out when PB subclasses a control
SENTINEL_END = SENTINEL + 0x10  # fnstsw [SW_ADDR]; fstp tword [ST0_ADDR]; hlt
SW_ADDR = TRAMP + 0x110
ST0_ADDR = TRAMP + 0x120
INPUTBOX_DLGPROC = 0x6B82BB  # dialog proc of the runtime's INPUTBOX$ dialog
BOOT_STOP = 0x4023E0  # in PBMAIN, right after the global DIMs and InitCommonControlsEx
FPU_CW = 0x133F
ERR_ENTRIES = {0x6B73FE: "dl", 0x6B7406: "al"}

REGS = {
    "eax": UC_X86_REG_EAX, "ebx": UC_X86_REG_EBX, "ecx": UC_X86_REG_ECX, "edx": UC_X86_REG_EDX,
    "esi": UC_X86_REG_ESI, "edi": UC_X86_REG_EDI, "ebp": UC_X86_REG_EBP, "esp": UC_X86_REG_ESP,
    "eip": UC_X86_REG_EIP,
}

# PB type -> (element size, struct format or special, PB array type code)
TYPES = {
    "BYTE": (1, "B"), "INTEGER": (2, "h"), "LONG": (4, "i"), "QUAD": (8, "q"), "SINGLE": (4, "f"),
    "DOUBLE": (8, "d"), "EXT": (10, "ext"), "CUR": (8, "q"), "CUX": (8, "q"), "STRING": (4, "str"),
}
TYPE_CODE = {"INTEGER": 0x08, "LONG": 0x0A, "QUAD": 0x10, "SINGLE": 0x14, "DOUBLE": 0x16, "EXT": 0x18,
             "CUR": 0x1A, "STRING": 0x2E}

ANSWERS = {"IDOK": 1, "IDCANCEL": 2, "IDABORT": 3, "IDRETRY": 4, "IDIGNORE": 5, "IDYES": 6, "IDNO": 7}

# stdcall argument counts of every import of WSR.EXE
NARGS = {
    # comdlg32
    "CommDlgExtendedError": 0, "PrintDlgA": 1,
    # gdi32
    "AbortDoc": 1, "Arc": 9, "BitBlt": 9, "CreateCompatibleBitmap": 3, "CreateCompatibleDC": 1, "CreateDCA": 4,
    "CreateFontIndirectA": 1, "CreateHatchBrush": 2, "CreatePen": 3, "CreateSolidBrush": 1, "DeleteDC": 1,
    "DeleteObject": 1, "EndDoc": 1, "EndPage": 1, "ExtFloodFill": 5, "GetCurrentObject": 2,
    "GetCurrentPositionEx": 2, "GetDeviceCaps": 2, "GetDIBits": 7, "GetObjectA": 3, "GetPixel": 3,
    "GetStockObject": 1, "GetTextExtentPoint32A": 4, "GetTextMetricsA": 2, "LineTo": 3, "MoveToEx": 4,
    "Pie": 9, "Polygon": 3, "Polyline": 3, "ResetDCA": 2, "RoundRect": 7, "SelectObject": 2, "SetBkColor": 2,
    "SetBkMode": 2, "SetBrushOrgEx": 4, "SetDIBits": 7, "SetPixelV": 4, "SetPolyFillMode": 2, "SetROP2": 2,
    "SetStretchBltMode": 2, "SetTextAlign": 2, "SetTextColor": 2, "StartDocA": 2, "StartPage": 1,
    "StretchDIBits": 13, "TextOutA": 5,
    # kernel32
    "CloseHandle": 1, "CreateFileA": 7, "CreateProcessA": 10, "DeleteFileA": 1, "EnumResourceNamesA": 4,
    "ExitProcess": 1, "FindClose": 1, "FindFirstFileA": 2, "FindNextFileA": 2, "GetCommandLineA": 0,
    "GetExitCodeProcess": 2, "GetFileType": 1, "GetLastError": 0, "GetLocalTime": 1, "GetModuleFileNameA": 3,
    "GetModuleHandleA": 1, "GetStartupInfoA": 1, "GetStringTypeA": 5, "GetVersionExA": 1,
    "GetVolumeInformationA": 8, "GlobalAlloc": 2, "GlobalFree": 1, "GlobalLock": 1, "GlobalReAlloc": 3,
    "GlobalUnlock": 1, "MultiByteToWideChar": 6, "OpenProcess": 3, "OutputDebugStringA": 1, "ReadFile": 5,
    "SetCurrentDirectoryA": 1, "SetEndOfFile": 1, "SetErrorMode": 1, "SetFileAttributesA": 2,
    "SetFilePointer": 4, "SetLastError": 1, "Sleep": 1, "TlsAlloc": 0, "TlsFree": 1, "TlsGetValue": 1,
    "TlsSetValue": 2, "WaitForSingleObject": 2, "WideCharToMultiByte": 8, "WriteFile": 5,
    # ole32 / oleaut32
    "CLSIDFromProgID": 2, "CoCreateInstance": 5, "CoInitialize": 1, "CoUninitialize": 0, "ProgIDFromCLSID": 2,
    "GetActiveObject": 3, "SafeArrayCreate": 3, "SysAllocStringByteLen": 2, "SysFreeString": 1,
    "SysStringByteLen": 1, "VariantClear": 1, "VariantCopy": 2,
    # user32
    "CallWindowProcA": 5, "CharLowerBuffA": 2, "CharNextA": 1, "CharUpperBuffA": 2, "CheckRadioButton": 4,
    "ClientToScreen": 2, "CreateDialogIndirectParamA": 5, "CreateMenu": 0, "CreatePopupMenu": 0,
    "CreateWindowExA": 12, "DeleteMenu": 3, "DestroyIcon": 1, "DestroyWindow": 1, "DialogBoxIndirectParamA": 5,
    "DispatchMessageA": 1, "DrawIconEx": 9, "DrawMenuBar": 1, "EnableWindow": 2, "EndDialog": 2, "FillRect": 3,
    "GetClientRect": 2, "GetCursorPos": 1, "GetDC": 1, "GetDlgCtrlID": 1, "GetDlgItem": 2,
    "GetForegroundWindow": 0, "GetMenu": 1, "GetMenuItemInfoA": 4, "GetSysColor": 1, "GetSysColorBrush": 1,
    "GetWindowLongA": 2, "GetWindowRect": 2, "GetWindowTextA": 3, "GetWindowTextLengthA": 1,
    "InsertMenuItemA": 4, "InvalidateRect": 3, "IsDialogMessageA": 2, "IsWindow": 1, "LoadImageA": 6,
    "MapDialogRect": 2, "MessageBeep": 1, "MessageBoxA": 4, "PeekMessageA": 5, "PostMessageA": 4,
    "RedrawWindow": 4, "ReleaseDC": 2, "ScreenToClient": 2, "SendDlgItemMessageA": 5, "SendMessageA": 4,
    "SetFocus": 1, "SetForegroundWindow": 1, "SetMenu": 2, "SetMenuItemInfoA": 4, "SetWindowLongA": 3,
    "SetWindowPos": 7, "SetWindowTextA": 2, "ShowWindow": 2, "SystemParametersInfoA": 4, "TranslateMessage": 1,
    "UpdateWindow": 1, "EnableMenuItem": 3, "GetSystemMetrics": 1, "IsIconic": 1,
    # winspool / comctl32 / shell32
    "ClosePrinter": 1, "DeviceCapabilitiesA": 5, "DocumentPropertiesA": 6, "OpenPrinterA": 3,
    "InitCommonControlsEx": 1, "ShellExecuteA": 6,
    # game DLLs (counts from the call sites 0x40cfea/0x446900, 0x40cf34, 0x480dc7)
    "HiScore": 4, "LawFirm": 3, "RemarksData": 1,
}
# which stack args are char* (for the log)
STR_ARGS = {"MessageBoxA": (1, 2), "SetWindowTextA": (1,), "OutputDebugStringA": (0,), "CreateFileA": (0,),
            "DeleteFileA": (0,), "SetCurrentDirectoryA": (0,), "SetFileAttributesA": (0,), "ShellExecuteA": (1, 2),
            "TextOutA": (3,), "FindFirstFileA": (0,), "LoadImageA": (), "CreateProcessA": (0, 1)}
WM_QUIET = {"SendMessageA", "SendDlgItemMessageA", "GetDlgItem", "IsWindow", "PeekMessageA", "IsDialogMessageA",
            "DispatchMessageA", "TranslateMessage", "GetWindowLongA", "SetWindowLongA", "CallWindowProcA",
            "CreateWindowExA", "CreateDialogIndirectParamA", "DestroyWindow", "SetWindowTextA", "ShowWindow",
            "GetWindowTextA", "GetWindowTextLengthA", "GetDlgCtrlID", "SetFocus", "EnableWindow",
            "SetForegroundWindow", "SetWindowPos", "UpdateWindow", "InvalidateRect", "RedrawWindow", "Sleep",
            "PostMessageA", "EndDialog", "MapDialogRect", "GetSysColor", "GetWindowRect", "GetClientRect"}
QUIET = {"SysAllocStringByteLen", "SysFreeString", "SysStringByteLen", "GlobalAlloc", "GlobalFree", "GlobalLock",
         "GlobalUnlock", "GlobalReAlloc", "TlsGetValue", "TlsSetValue", "TlsAlloc", "CharUpperBuffA",
         "CharLowerBuffA", "CharNextA", "GetStringTypeA", "GetLastError", "SetLastError",
         "MultiByteToWideChar", "WideCharToMultiByte", "ReadFile", "WriteFile", "SetFilePointer", "CloseHandle",
         "GetFileType", "SetEndOfFile", "SetErrorMode"}


class EmuFault(Exception):
    pass


_FUNCS = None


def func_names():
    """{address: name} of the game routines named in tools/names.tsv."""
    global _FUNCS
    if _FUNCS is None:
        _FUNCS = {}
        p = os.path.join(ROOT, "tools", "names.tsv")
        if os.path.exists(p):
            for ln in Path(p).read_text(encoding="utf-8").splitlines():
                f = ln.split("\t")
                if len(f) > 1 and re.match(r"^[0-9a-f]{6}$", f[0]) and not f[0].startswith("6c"):
                    _FUNCS[int(f[0], 16)] = f[1]
    return _FUNCS


# ---------------------------------------------------------------- layout (parsed from state.rs)
_LAYOUT = None


def layout(path=STATE_RS):
    """[(addr_key, name, type, bounds or None)] in state.rs (= snapshot) order."""
    global _LAYOUT
    if _LAYOUT is None:
        out = []
        rx_s = re.compile(r"^\s*/// `(\w+) AS (\w+)` at 0x([0-9a-f]+)\.$")
        rx_a = re.compile(r"^\s*/// `DIM (\w+)\((.*)\) AS (\w+)` at 0x([0-9a-f]+)\.$")
        for ln in Path(path).read_text(encoding="utf-8").splitlines():
            m = rx_a.match(ln)
            if m:
                b = [tuple(int(v) for v in x.split(" TO ")) for x in m.group(2).split(", ")]
                out.append((m.group(4), m.group(1), m.group(3), b))
                continue
            m = rx_s.match(ln)
            if m:
                out.append((m.group(3), m.group(1), m.group(2), None))
        _LAYOUT = out
    return _LAYOUT


# ---------------------------------------------------------------- 80-bit floats
def f64_to_ext(x):
    if x != x:
        return struct.pack("<QH", 0xC000000000000000, 0xFFFF)
    sign = 0x8000 if math.copysign(1.0, x) < 0 else 0
    if x == 0:
        return struct.pack("<QH", 0, sign)
    if math.isinf(x):
        return struct.pack("<QH", 1 << 63, sign | 0x7FFF)
    m, e = math.frexp(abs(x))  # m in [0.5, 1), 53 bits
    return struct.pack("<QH", int(m * (1 << 64)), sign | (e + 16382))


def ext_to_f64(b):
    m, se = struct.unpack("<QH", b)
    sign = -1.0 if se & 0x8000 else 1.0
    e = se & 0x7FFF
    if e == 0x7FFF:
        return sign * math.inf if (m << 1) & 0xFFFFFFFFFFFFFFFF == 0 else math.nan
    if m == 0:
        return sign * 0.0
    p = e - 16383 - 63 if e else -16382 - 63
    if -1074 + 64 <= p + 64 <= 1023:  # normal f64 range: float(int) rounds half-even, ldexp exact
        r = math.ldexp(float(m), p)
        if r >= 2.2250738585072014e-308:
            return sign * r
    try:
        return sign * float(Fraction(m) * Fraction(2) ** p)
    except OverflowError:
        return sign * math.inf


# ---------------------------------------------------------------- heap
class Heap:
    """Blocks: [capacity][length] header, user pointer = block + 8 (so ptr-4 is the BSTR length)."""

    def __init__(self, uc):
        self.uc = uc
        self.top = HEAP
        self.free = {}
        self.live = {}

    def alloc(self, n, zero=True):
        cap = (max(n, 1) + 2 + 15) & ~15
        lst = self.free.get(cap)
        if lst:
            blk = lst.pop()
            if zero:
                self.uc.mem_write(blk + 8, b"\0" * cap)
        else:
            blk = self.top
            if blk + 8 + cap > HEAP + HEAP_SIZE:
                raise EmuFault("emulator heap exhausted")
            self.top = blk + 8 + cap
        self.uc.mem_write(blk, struct.pack("<II", cap, n))
        self.live[blk + 8] = cap
        return blk + 8

    def release(self, p):
        cap = self.live.pop(p, None)
        if cap is None:
            return False
        self.free.setdefault(cap, []).append(p - 8)
        return True

    def bstr(self, data):
        if not data:
            return 0
        p = self.alloc(len(data), zero=False)
        self.uc.mem_write(p, bytes(data) + b"\0\0")
        return p


# ---------------------------------------------------------------- virtual filesystem
APP_DIR = os.path.join(ROOT, "extracted", "app")
INVALID_HANDLE = 0xFFFFFFFF
FILE_FUNCS = {"CreateFileA", "ReadFile", "WriteFile", "SetFilePointer", "SetEndOfFile", "CloseHandle",
              "GetFileType", "DeleteFileA", "FindFirstFileA", "FindNextFileA", "FindClose", "SetFileAttributesA"}


def vkey(name):
    """Files are keyed by upper-case base name: directories (DataPath etc.) and case are ignored."""
    if isinstance(name, bytes):
        name = name.decode("latin-1")
    return re.split(r"[\\/:]", name)[-1].upper()


class VFS:
    """In-memory files {key: bytearray}. Names not given explicitly are looked up (lazily, read-only
    copies) in extracted/app/ unless `defaults` is False. `changed` = keys written, created or deleted."""

    def __init__(self, emu, files=None, defaults=True):
        self.emu = emu
        self.files = {}
        self.names = {}
        for k, v in (files or {}).items():
            self.files[vkey(k)] = bytearray(v)
            self.names[vkey(k)] = k
        self.deleted = set()
        self.changed = set()
        self.handles = {}
        self.finds = {}
        self.next_h = 0x5000
        self.app = {}
        if defaults and os.path.isdir(APP_DIR):
            for f in os.listdir(APP_DIR):
                if os.path.isfile(os.path.join(APP_DIR, f)):
                    self.app[f.upper()] = f

    def lookup(self, key):
        if key in self.files:
            return self.files[key]
        if key in self.deleted or key not in self.app:
            return None
        with open(os.path.join(APP_DIR, self.app[key]), "rb") as fh:
            self.files[key] = bytearray(fh.read())
        self.names[key] = self.app[key]
        return self.files[key]

    def all_keys(self):
        return sorted((set(self.files) | set(self.app)) - self.deleted)

    def outputs(self):
        """{name: bytes or None (deleted)} of every file the run wrote, created or deleted."""
        out = {}
        for k in sorted(self.changed):
            n = self.names.get(k, k)
            out[n] = None if k in self.deleted else bytes(self.files[k])
        return out

    def _new_handle(self, obj, table):
        h = self.next_h
        self.next_h += 4
        table[h] = obj
        return h

    def _find_data(self, ptr, key):
        data = self.lookup(key)
        name = self.names.get(key, key).encode("latin-1")[:259]
        b = bytearray(320)
        struct.pack_into("<I", b, 0, 0x20)  # FILE_ATTRIBUTE_ARCHIVE
        struct.pack_into("<II", b, 28, 0, len(data))
        b[44:44 + len(name)] = name
        self.emu.uc.mem_write(ptr, bytes(b))

    def api(self, name, a):
        e = self.emu
        if name == "CreateFileA":
            fname, access, share, sec, disp, flags, tmpl = a
            raw = e.cstr(fname)
            key = vkey(raw)
            data = self.lookup(key)
            if key not in self.names:
                self.names[key] = raw.decode("latin-1")
            exists = data is not None
            if (disp in (3, 5) and not exists) or (disp == 1 and exists):  # OPEN/TRUNCATE_EXISTING, CREATE_NEW
                e.last_error = 2 if not exists else 80
                return INVALID_HANDLE
            if not exists or disp in (2, 5):  # CREATE_ALWAYS / TRUNCATE_EXISTING / new file
                self.files[key] = bytearray()
                self.deleted.discard(key)
                self.changed.add(key)
            e.last_error = 183 if exists and disp in (2, 4) else 0
            return self._new_handle({"key": key, "pos": 0, "write": bool(access & 0x40000000)}, self.handles)
        if name == "CloseHandle":
            if a[0] in self.handles:
                del self.handles[a[0]]
            return 1
        if name == "GetFileType":
            return 1 if a[0] in self.handles else 0
        if name in ("ReadFile", "WriteFile"):
            h, buf, n, pdone, ov = a
            f = self.handles.get(h)
            if f is None:
                e.last_error = 6
                return 0
            data = self.files[f["key"]]
            if name == "ReadFile":
                chunk = bytes(data[f["pos"]:f["pos"] + n])
                e.uc.mem_write(buf, chunk)
            else:
                chunk = e.rd(buf, n)
                end = f["pos"] + n
                if f["pos"] > len(data):
                    data.extend(b"\0" * (f["pos"] - len(data)))
                data[f["pos"]:end] = chunk
                self.changed.add(f["key"])
            f["pos"] += len(chunk)
            if pdone:
                e.w32(pdone, len(chunk))
            return 1
        if name == "SetFilePointer":
            h, lo, phi, method = a
            f = self.handles.get(h)
            if f is None:
                e.last_error = 6
                return INVALID_HANDLE
            off = struct.unpack("<i", struct.pack("<I", lo))[0]
            if phi:
                hi = struct.unpack("<i", e.rd(phi, 4))[0]
                off = (hi << 32) | (lo & 0xFFFFFFFF)
            base = (0, f["pos"], len(self.files[f["key"]]))[method]
            f["pos"] = max(0, base + off)
            if phi:
                e.w32(phi, f["pos"] >> 32)
            return f["pos"] & 0xFFFFFFFF
        if name == "SetEndOfFile":
            f = self.handles.get(a[0])
            if f is None:
                return 0
            del self.files[f["key"]][f["pos"]:]
            self.changed.add(f["key"])
            return 1
        if name == "DeleteFileA":
            key = vkey(e.cstr(a[0]))
            if self.lookup(key) is None:
                e.last_error = 2
                return 0
            self.deleted.add(key)
            self.changed.add(key)
            self.files.pop(key, None)
            return 1
        if name == "SetFileAttributesA":
            return 1 if self.lookup(vkey(e.cstr(a[0]))) is not None else 0
        if name == "FindFirstFileA":
            pat = vkey(e.cstr(a[0]))
            import fnmatch
            hits = [k for k in self.all_keys() if fnmatch.fnmatchcase(k, pat)]
            if not hits:
                e.last_error = 2
                return INVALID_HANDLE
            self._find_data(a[1], hits[0])
            return self._new_handle(hits[1:], self.finds)
        if name == "FindNextFileA":
            rest = self.finds.get(a[0])
            if not rest:
                e.last_error = 18
                return 0
            self._find_data(a[1], rest.pop(0))
            return 1
        if name == "FindClose":
            return 1 if self.finds.pop(a[0], None) is not None else 0
        raise EmuFault("VFS: %s" % name)


# ---------------------------------------------------------------- window manager (PB DDT dialogs)
WM_DESTROY, WM_CLOSE, WM_SETTEXT, WM_GETTEXT, WM_GETTEXTLENGTH = 0x2, 0x10, 0xC, 0xD, 0xE
WM_INITDIALOG, WM_COMMAND = 0x110, 0x111
DIALOG_ACTIONS = ("click:", "close", "sysclose", "end", "select:", "dblclick:", "text:")
MAX_DIALOG_ACTIONS = 5  # actions per dialog before the harness destroys it ("FORCED END")


def is_dialog_action(a):
    return isinstance(a, str) and a.startswith(DIALOG_ACTIONS)


class Win:
    def __init__(self, hwnd, cls, text, parent, cid, style, proc=0):
        self.hwnd, self.cls, self.text, self.parent, self.id, self.style = hwnd, cls, text, parent, cid, style
        self.proc = proc  # dialog procedure (dialogs only)
        self.items, self.data, self.cursel, self.check = [], [], -1, 0
        self.longs = {}
        self.alive, self.shown, self.ended, self.result = True, False, False, 0
        self.actions, self.closed_by = 0, None

    @property
    def is_dialog(self):
        return self.proc != 0

    def model(self, wm):
        d = {"hwnd": self.hwnd, "title": self.text, "alive": self.alive, "closed_by": self.closed_by,
             "controls": []}
        for c in wm.children(self.hwnd):
            d["controls"].append({"id": c.id, "class": c.cls, "text": c.text, "style": c.style,
                                  "items": list(c.items), "cursel": c.cursel, "check": c.check})
        return d


class WindowManager:
    """Just enough of USER32 for PowerBASIC DDT: windows/controls with texts and list items, dialog
    procedures called back into emulated code (API implementations are generators that `yield (proc,
    args)` and get the callback's EAX back), a posted-message queue, and scripted closing of modal
    dialogs from PeekMessageA when the queue is empty."""

    def __init__(self, emu):
        self.e = emu
        self.wins = {}
        self.order = []  # dialogs in creation order
        self.queue = []
        self.next = 0x3000
        self.focus = 0

    def new(self, *a, **k):
        h = self.next
        self.next += 4
        w = Win(h, *a, **k)
        self.wins[h] = w
        if w.is_dialog:
            self.order.append(w)
        return w

    def children(self, h):
        return [w for w in self.wins.values() if w.parent == h and not w.is_dialog]

    def child(self, h, cid):
        for w in self.children(h):
            if w.id == cid and w.alive:
                return w
        return None

    def models(self):
        return [w.model(self) for w in self.order]

    def _log(self, s):
        self.e.log.append(s)

    def _template_title(self, p):
        """Title of a DLGTEMPLATE(EX) (best effort)."""
        e = self.e
        try:
            ex = struct.unpack("<HH", e.rd(p, 4)) == (1, 0xFFFF)
            q = p + (26 if ex else 18)
            for _ in range(2):  # menu, class
                w0 = struct.unpack("<H", e.rd(q, 2))[0]
                if w0 == 0:
                    q += 2
                elif w0 == 0xFFFF:
                    q += 4
                else:
                    while struct.unpack("<H", e.rd(q, 2))[0]:
                        q += 2
                    q += 2
            t = b""
            while True:
                c = struct.unpack("<H", e.rd(q, 2))[0]
                if not c:
                    break
                t += bytes([c & 0xFF])
                q += 2
            return t
        except (UcError, struct.error):
            return b""

    # --- dialog creation / destruction
    def create_dialog(self, tmpl, parent, proc, lparam):
        w = self.new("#32770", self._template_title(tmpl), parent, 0, 0, proc)
        self._log("DIALOG NEW 0x%x %r" % (w.hwnd, w.text.decode("latin-1")))
        yield (proc, [w.hwnd, WM_INITDIALOG, 0, lparam])
        return w

    def destroy(self, w, how):
        if not w.alive:
            return
        if w.is_dialog:
            yield (w.proc, [w.hwnd, WM_DESTROY, 0, 0])
        w.alive = False
        w.closed_by = w.closed_by or how
        for c in self.children(w.hwnd):
            c.alive = False
        if w.is_dialog:
            self._log("DIALOG END 0x%x %r (%s)" % (w.hwnd, w.text.decode("latin-1"), w.closed_by))

    # --- scripted closing
    def next_action(self, w):
        """Messages to post to dialog `w` for the next scripted (or default) action."""
        e = self.e
        w.actions += 1
        if w.actions > MAX_DIALOG_ACTIONS:
            return "end", []
        act = e.answers.pop(0) if e.answers and is_dialog_action(e.answers[0]) else None
        while act and act.startswith("text:"):
            _, cid, val = act.split(":", 2)
            c = self.child(w.hwnd, int(cid))
            if c:
                c.text = _bytes(val)
            self._log("DIALOG 0x%x text %s = %r" % (w.hwnd, cid, val))
            act = e.answers.pop(0) if e.answers and is_dialog_action(e.answers[0]) else None
        if act is None:
            ok = self.child(w.hwnd, 1)
            if ok is None:
                for c in self.children(w.hwnd):
                    if c.alive and c.cls.upper() == b"BUTTON" and c.style & 0xF == 1:
                        ok = c
                        break
            act = "click:%d" % ok.id if ok else "close"
            how = "default " + act
        else:
            how = act
        self._log("DIALOG 0x%x %s" % (w.hwnd, how))
        w.closed_by = how
        if act == "close":
            return how, [(w.hwnd, WM_CLOSE, 0, 0)]
        if act == "sysclose":
            # The title-bar X sends WM_SYSCOMMAND/SC_CLOSE. PowerBASIC DDT handles it in
            # its dialog procedure (0x6baa83), whereas a bare WM_CLOSE can fall through
            # to Windows' default dialog procedure, which this harness does not emulate.
            return how, [(w.hwnd, 0x112, 0xF060, 0)]
        if act == "end":
            return "end", []
        kind, rest = act.split(":", 1)
        if kind == "click":
            cid = int(rest)
            c = self.child(w.hwnd, cid)
            return how, [(w.hwnd, WM_COMMAND, cid & 0xFFFF, c.hwnd if c else 0)]
        cid, idx = (int(x) for x in rest.split(":"))
        c = self.child(w.hwnd, cid)
        if c:
            c.cursel = idx
        code = 1 if kind == "select" else 2  # LBN_SELCHANGE / LBN_DBLCLK
        return how, [(w.hwnd, WM_COMMAND, (code << 16) | (cid & 0xFFFF), c.hwnd if c else 0)]

    def modal_target(self):
        for w in reversed(self.order):
            if w.alive and w.shown:
                return w
        return None

    # --- message plumbing
    def deliver(self, h, msg, wp, lp):
        """Send a message: list/text messages are handled here, the rest goes to the dialog procedure."""
        e = self.e
        w = self.wins.get(h)
        if w is None or not w.alive:
            return 0
        r = self.control_message(w, msg, wp, lp)
        if r is not None:
            return r
        if w.is_dialog:
            return (yield (w.proc, [h, msg, wp, lp]))
        return 0

    def control_message(self, w, msg, wp, lp):
        e = self.e
        sort = (w.cls.upper() == b"LISTBOX" and w.style & 0x2) or (w.cls.upper() == b"COMBOBOX" and w.style & 0x100)
        if msg in (0x180, 0x143):  # LB_ADDSTRING / CB_ADDSTRING
            t = e.cstr(lp, 1 << 16)
            if sort:
                i = 0
                while i < len(w.items) and w.items[i].upper() <= t.upper():
                    i += 1
            else:
                i = len(w.items)
            w.items.insert(i, t)
            w.data.insert(i, 0)
            return i
        if msg in (0x181, 0x14A):  # LB_INSERTSTRING / CB_INSERTSTRING
            i = len(w.items) if wp == 0xFFFFFFFF else wp
            w.items.insert(i, e.cstr(lp, 1 << 16))
            w.data.insert(i, 0)
            return i
        if msg in (0x182, 0x144):  # LB_DELETESTRING / CB_DELETESTRING
            if wp < len(w.items):
                del w.items[wp]
                del w.data[wp]
                return len(w.items)
            return 0xFFFFFFFF
        if msg in (0x184, 0x14B):  # LB_RESETCONTENT / CB_RESETCONTENT
            w.items, w.data, w.cursel = [], [], -1
            return 0
        if msg in (0x18B, 0x146):  # GETCOUNT
            return len(w.items)
        if msg in (0x189, 0x148):  # LB_GETTEXT / CB_GETLBTEXT
            if wp >= len(w.items):
                return 0xFFFFFFFF
            e.uc.mem_write(lp, w.items[wp] + b"\0")
            return len(w.items[wp])
        if msg in (0x18A, 0x149):  # GETTEXTLEN
            return len(w.items[wp]) if wp < len(w.items) else 0xFFFFFFFF
        if msg in (0x186, 0x14E):  # SETCURSEL
            w.cursel = -1 if wp == 0xFFFFFFFF else wp
            return w.cursel & 0xFFFFFFFF
        if msg in (0x188, 0x147):  # GETCURSEL
            return w.cursel & 0xFFFFFFFF
        if msg in (0x19A, 0x151):  # SETITEMDATA
            if wp < len(w.data):
                w.data[wp] = lp
            return 0
        if msg in (0x199, 0x150):  # GETITEMDATA
            return w.data[wp] if wp < len(w.data) else 0xFFFFFFFF
        if msg == WM_SETTEXT:
            w.text = e.cstr(lp, 1 << 16)
            return 1
        if msg == WM_GETTEXT:
            if wp:
                e.uc.mem_write(lp, w.text[:wp - 1] + b"\0")
            return min(len(w.text), max(wp - 1, 0))
        if msg == WM_GETTEXTLENGTH:
            return len(w.text)
        if msg == 0xF1:  # BM_SETCHECK
            w.check = wp
            return 0
        if msg == 0xF0:  # BM_GETCHECK
            return w.check
        if msg in (0x30, 0x31, 0x0B, 0x0F) or 0xB0 <= msg <= 0xDF or 0x400 <= msg < 0x8000:
            return 0  # WM_SETFONT/GETFONT/SETREDRAW/PAINT, EM_*, WM_USER+ (control-specific: ignored)
        if not w.is_dialog:
            return 0
        return None

    # --- API entry points (may be generators)
    def api(self, name, a, ret):
        e = self.e
        if name == "CreateDialogIndirectParamA":
            w = yield from self.create_dialog(a[1], a[2], a[3], a[4])
            return w.hwnd
        if name == "DialogBoxIndirectParamA":
            w = yield from self.create_dialog(a[1], a[2], a[3], a[4])
            w.shown = True
            while w.alive and not w.ended:
                how, msgs = self.next_action(w)
                if how == "end":
                    break
                for m in msgs:
                    yield from self.deliver(*m)
            yield from self.destroy(w, "EndDialog" if w.ended else "FORCED END")
            return w.result
        if name == "CreateWindowExA":
            ex, cls, text, style, x, y, cx, cy, parent, menu, hinst, param = a
            cname = e.cstr(cls) if cls >= 0x10000 else b"#%d" % cls
            w = self.new(cname, e.cstr(text, 1 << 16), parent, menu if style & 0x40000000 else 0, style)
            self._log("CONTROL ADD %s 0x%x id=%d %r" % (cname.decode("latin-1"), parent, w.id,
                                                        w.text.decode("latin-1")))
            return w.hwnd
        if name == "DestroyWindow":
            w = self.wins.get(a[0])
            if w is None:
                return 0
            yield from self.destroy(w, "DestroyWindow")
            return 1
        if name == "EndDialog":
            w = self.wins.get(a[0])
            if w is None:
                return 0
            w.ended, w.result = True, a[1]
            w.closed_by = w.closed_by or "EndDialog"
            return 1
        if name == "IsWindow":
            w = self.wins.get(a[0])
            return 1 if w and w.alive else 0
        if name == "GetDlgItem":
            c = self.child(a[0], a[1] & 0xFFFF)
            return c.hwnd if c else 0
        if name == "GetDlgCtrlID":
            w = self.wins.get(a[0])
            return w.id if w else 0
        if name == "SetWindowTextA":
            w = self.wins.get(a[0])
            if w is None:
                return 0
            w.text = e.cstr(a[1], 1 << 16)
            self._log("SET TEXT 0x%x id=%d %r" % (w.parent, w.id, w.text.decode("latin-1")) if not w.is_dialog
                      else "DIALOG SET TEXT 0x%x %r" % (w.hwnd, w.text.decode("latin-1")))
            return 1
        if name in ("GetWindowTextA", "GetWindowTextLengthA"):
            w = self.wins.get(a[0])
            t = w.text if w else b""
            if name == "GetWindowTextLengthA":
                return len(t)
            if a[2]:
                e.uc.mem_write(a[1], t[:a[2] - 1] + b"\0")
            return min(len(t), max(a[2] - 1, 0))
        if name == "GetWindowLongA":
            w = self.wins.get(a[0])
            if w is None:
                return 0
            idx = struct.unpack("<i", struct.pack("<I", a[1]))[0]
            if idx == -12:
                return w.id
            if idx == -16:
                return w.style
            if idx == -8:
                return w.longs.get(-8, w.parent if w.is_dialog else 0)
            if idx == 4 and w.is_dialog:
                return w.longs.get(4, w.proc)
            return w.longs.get(idx, 0)
        if name == "SetWindowLongA":
            w = self.wins.get(a[0])
            if w is None:
                return 0
            idx = struct.unpack("<i", struct.pack("<I", a[1]))[0]
            if idx == -4 and not w.is_dialog:
                old = w.longs.get(-4, FAKE_WNDPROC)
            else:
                old = w.longs.get(idx, 0)
            w.longs[idx] = a[2]
            return old
        if name == "CallWindowProcA":
            proc, h, msg, wp, lp = a
            w = self.wins.get(h)
            if w is None:
                return 0
            if proc == FAKE_WNDPROC:
                r = self.control_message(w, msg, wp, lp)
                return r or 0
            return (yield (proc, [h, msg, wp, lp]))
        if name == "SendMessageA":
            return (yield from self.deliver(*a))
        if name == "SendDlgItemMessageA":
            c = self.child(a[0], a[1] & 0xFFFF)
            if c is None:
                return 0
            return (yield from self.deliver(c.hwnd, a[2], a[3], a[4]))
        if name == "PostMessageA":
            self.queue.append(tuple(a))
            return 1
        if name == "PeekMessageA":
            pmsg, h, mn, mx, remove = a
            if not self.queue:
                w = self.modal_target()
                if w is None:
                    return 0
                how, msgs = self.next_action(w)
                if how == "end":
                    self._log("DIALOG 0x%x FORCED END" % w.hwnd)
                    yield from self.destroy(w, "FORCED END")
                    # Forced closure bypasses DDT's DIALOG END helper. Wake its modal loop:
                    # it checks IsWindow only after PeekMessage succeeds, and otherwise spins
                    # forever on an empty queue even though the dialog has been destroyed.
                    msgs = [(w.hwnd, 0, 0, 0)]  # WM_NULL, discarded because the window is dead
                self.queue.extend(msgs)
                if not self.queue:
                    return 0
            m = self.queue[0]
            if remove & 1:
                self.queue.pop(0)
            e.uc.mem_write(pmsg, struct.pack("<IIIIIii", m[0], m[1], m[2], m[3], 0, 0, 0))
            return 1
        if name in ("IsDialogMessageA", "DispatchMessageA"):
            pmsg = a[1] if name == "IsDialogMessageA" else a[0]
            h, msg, wp, lp = struct.unpack("<IIII", e.rd(pmsg, 16))
            if name == "IsDialogMessageA":
                tw = self.wins.get(h)
                if not (h == a[0] or (tw and tw.parent == a[0])):
                    return 0
            yield from self.deliver(h, msg, wp, lp)
            return 1
        if name == "ShowWindow":
            w = self.wins.get(a[0])
            if w:
                was = w.shown
                w.shown = a[1] != 0
                if w.is_dialog and w.shown and not was:
                    self._log("DIALOG SHOW 0x%x %r" % (w.hwnd, w.text.decode("latin-1")))
                return 1 if was else 0
            return 0
        if name == "SetFocus":
            old, self.focus = self.focus, a[0]
            return old
        if name in ("EnableWindow", "SetForegroundWindow", "UpdateWindow", "InvalidateRect", "RedrawWindow",
                    "SetWindowPos", "TranslateMessage", "CheckRadioButton"):
            if name == "CheckRadioButton":
                for c in self.children(a[0]):
                    if a[1] <= c.id <= a[2]:
                        c.check = 1 if c.id == a[3] else 0
            return 0 if name == "TranslateMessage" else 1
        raise EmuFault("WindowManager: %s" % name)


WM_FUNCS = {"CreateDialogIndirectParamA", "DialogBoxIndirectParamA", "CreateWindowExA", "DestroyWindow",
            "EndDialog", "IsWindow", "GetDlgItem", "GetDlgCtrlID", "SetWindowTextA", "GetWindowTextA",
            "GetWindowTextLengthA", "GetWindowLongA", "SetWindowLongA", "CallWindowProcA", "SendMessageA",
            "SendDlgItemMessageA", "PostMessageA", "PeekMessageA", "IsDialogMessageA", "DispatchMessageA",
            "ShowWindow", "SetFocus", "EnableWindow", "SetForegroundWindow", "UpdateWindow", "InvalidateRect",
            "RedrawWindow", "SetWindowPos", "TranslateMessage", "CheckRadioButton"}


# ---------------------------------------------------------------- emulator
class Emu:
    def __init__(self, exe=DEFAULT_EXE, answers=(), trace=False, budget=0, timeout=60_000_000, hooks=(),
                 files=None, default_files=True, skips=()):
        self.pe = pefile.PE(exe, fast_load=True)
        self.pe.parse_data_directories(directories=[pefile.DIRECTORY_ENTRY["IMAGE_DIRECTORY_ENTRY_IMPORT"]])
        uc = Uc(UC_ARCH_X86, UC_MODE_32)
        self.uc = uc
        pe = self.pe
        size = (pe.OPTIONAL_HEADER.SizeOfImage + 0xFFFF) & ~0xFFFF
        uc.mem_map(BASE, size)
        uc.mem_write(BASE, pe.header)
        for s in pe.sections:
            uc.mem_write(BASE + s.VirtualAddress, s.get_data())
        for base, sz in ((STUB, 0x10000), (HEAP, HEAP_SIZE), (STACK, STACK_SIZE), (TRAMP, 0x1000)):
            uc.mem_map(base, sz)
        self.heap = Heap(uc)
        self.stubs = {}
        addr = STUB
        for entry in pe.DIRECTORY_ENTRY_IMPORT:
            for imp in entry.imports:
                name = imp.name.decode() if imp.name else "ord%d" % imp.ordinal
                uc.mem_write(imp.address, struct.pack("<I", addr))
                uc.mem_write(addr, b"\xc3")
                self.stubs[addr] = name
                addr += 4
        self._install_frndint_pe()
        uc.mem_write(CBRET, b"\xc3")
        uc.mem_write(FAKE_WNDPROC, b"\xc3")
        uc.hook_add(UC_HOOK_CODE, self._stub_hook, begin=STUB, end=STUB + 0xFFFF)
        for a in ERR_ENTRIES:
            uc.hook_add(UC_HOOK_CODE, self._err_hook, begin=a, end=a)
        self.trace = [] if trace else None
        if trace:
            uc.hook_add(UC_HOOK_BLOCK, self._block_hook)
        uc.hook_add(UC_HOOK_MEM_INVALID, self._mem_invalid)
        self.hooks = {}
        for h in hooks:
            uc.hook_add(UC_HOOK_CODE, self._call_hook, begin=h, end=h)
            self.hooks[h] = func_names().get(h, "sub_%x" % h)
        self.skips = {}
        for addr, n in skips:
            uc.hook_add(UC_HOOK_CODE, self._skip_hook, begin=addr, end=addr)
            self.skips[addr] = (n, func_names().get(addr, "sub_%x" % addr))
        self.last_error = 0
        self.answers = list(answers)
        # SYSTEMTIME; campaign runs advance this between calls without rebuilding the emulator.
        self.local_time = (2000, 1, 1, 3, 10, 0, 0, 0)
        self.log = []
        self.tls = {}
        self.budget = budget
        self.timeout = timeout
        self.bad_access = None
        self.stub_error = None
        self.warnings = []
        self.booted = False
        self.vfs = VFS(self, files, default_files)
        self.wm = WindowManager(self)
        self.pending = []  # API generators waiting for a callback to return
        self.cmdline = self.heap.bstr(b"WSR.EXE")
        self.empty_sz = self.heap.alloc(4)
        self.boot()

    def boot(self):
        """Run the real startup: PB runtime init from the entry point 0x6b71be (.data relocation, BSS and
        runtime-state zeroing, TLS error block, FPU control word 0x133f), then WinMain/PBMAIN 0x40148f up
        to BOOT_STOP, i.e. through its block of global DIMs (arrays get their original bounds) and
        InitCommonControlsEx, before the first game initialisation routine."""
        uc = self.uc
        esp = STACK + STACK_SIZE - 0x1000
        self.w32(esp, SENTINEL)
        uc.reg_write(UC_X86_REG_ESP, esp)
        try:
            uc.emu_start(BASE + self.pe.OPTIONAL_HEADER.AddressOfEntryPoint, BOOT_STOP, count=50_000_000)
        except UcError as e:
            raise EmuFault("boot fault at 0x%x: %s" % (uc.reg_read(UC_X86_REG_EIP), e))
        if uc.reg_read(UC_X86_REG_EIP) != BOOT_STOP or self.stub_error:
            raise EmuFault("boot did not reach 0x%x (eip 0x%x, %s)" % (
                BOOT_STOP, uc.reg_read(UC_X86_REG_EIP), self.stub_error))
        d = self.rd(0x6C51AC, 8)  # HoldStockId(0 TO 15, 0 TO 1600) AS INTEGER, DIMmed by PBMAIN
        if d[4] & 4 == 0 or d[6] != 0x08 or d[7] != 2:
            raise EmuFault("boot: unexpected HoldStockId descriptor %s" % d.hex())
        self.boot_log = self.log
        self.log = []
        self.booted = True

    # -- memory helpers
    def rd(self, a, n):
        return bytes(self.uc.mem_read(a, n))

    def u32(self, a):
        return struct.unpack("<I", self.uc.mem_read(a, 4))[0]

    def w32(self, a, v):
        self.uc.mem_write(a, struct.pack("<I", v & 0xFFFFFFFF))

    def cstr(self, p, limit=4096):
        if not p:
            return b""
        try:
            b = self.rd(p, limit)
        except UcError:
            b = b""
            for i in range(limit):
                try:
                    c = self.rd(p + i, 1)
                except UcError:
                    break
                if c == b"\0":
                    break
                b += c
        return b.split(b"\0", 1)[0]

    def read_bstr(self, p, what=None):
        if not p:
            return b""
        if p not in self.heap.live:
            self.warnings.append("%s: string pointer 0x%x is not a heap BSTR, read as \"\"" % (what, p))
            return b""
        return self.rd(p, self.u32(p - 4))

    # -- hooks
    def _block_hook(self, uc, address, size, _):
        self.trace.append((address, size))
        if len(self.trace) > 64:
            del self.trace[:32]

    def _call_hook(self, uc, address, size, _):
        """Log a call of a hooked game routine: its first 4 stack args (BYREF LONG values dereferenced)."""
        if not self.booted:
            return
        esp = uc.reg_read(UC_X86_REG_ESP)
        parts = []
        for i in range(4):
            v = self.u32(esp + 4 + 4 * i)
            try:
                parts.append("0x%x->%d" % (v, struct.unpack("<i", self.rd(v, 4))[0]))
            except UcError:
                parts.append("0x%x" % v)
        self.log.append("CALL %s(%s) [from 0x%x]" % (self.hooks[address], ", ".join(parts), self.u32(esp)))

    def _install_frndint_pe(self):
        """Unicorn's x87 never sets the precision-exception flag (PE, FPSW bit 5). PB_Pow (0x6bcaf8:
        fnclex; frndint; fnstsw ax; test al, 0x20) uses it to detect a non-integer exponent — the only
        place in WSR.EXE that reads PE (all 12.7k fnstsw sites checked). Emulate it for every frndint:
        remember ST0 before, set PE after if the value changed."""
        from capstone import CS_ARCH_X86, CS_MODE_32, Cs
        t = self.pe.sections[0]
        data = t.get_data()
        base = BASE + t.VirtualAddress
        md = Cs(CS_ARCH_X86, CS_MODE_32)
        md.skipdata = True
        self._frndint_pre = None
        sites = []
        k = data.find(b"\xd9\xfc")
        while k >= 0:
            a = base + k
            ins = next(md.disasm(data[k:k + 2], a), None)
            if ins is not None and ins.mnemonic == "frndint":
                sites.append(a)
            k = data.find(b"\xd9\xfc", k + 1)
        for a in sites:
            self.uc.hook_add(UC_HOOK_CODE, self._frndint_before, begin=a, end=a)
            self.uc.hook_add(UC_HOOK_CODE, self._frndint_after, begin=a + 2, end=a + 2)

    def _st0(self):
        top = (self.uc.reg_read(UC_X86_REG_FPSW) >> 11) & 7
        return self.uc.reg_read(UC_X86_REG_FP0 + top)

    def _frndint_before(self, uc, address, size, _):
        self._frndint_pre = (address, self._st0())

    def _frndint_after(self, uc, address, size, _):
        pre = self._frndint_pre
        self._frndint_pre = None
        if pre is not None and pre[0] == address - 2 and pre[1] != self._st0():
            uc.reg_write(UC_X86_REG_FPSW, uc.reg_read(UC_X86_REG_FPSW) | 0x20)

    def _skip_hook(self, uc, address, size, _):
        """Skipped routine: return 0 at once, popping its stdcall args."""
        if not self.booted:
            return
        n, name = self.skips[address]
        esp = uc.reg_read(UC_X86_REG_ESP)
        ret = self.u32(esp)
        self.log.append("SKIP %s [from 0x%x]" % (name, ret))
        uc.reg_write(UC_X86_REG_EAX, 0)
        uc.reg_write(UC_X86_REG_ESP, esp + 4 + 4 * n)
        uc.reg_write(UC_X86_REG_EIP, ret)

    def _mem_invalid(self, uc, access, address, size, value, _):
        self.bad_access = (access, address, size)
        return False

    def _err_hook(self, uc, address, size, _):
        reg = ERR_ENTRIES[address]
        code = uc.reg_read(UC_X86_REG_EDX if reg == "dl" else UC_X86_REG_EAX) & 0xFF
        ebp = uc.reg_read(UC_X86_REG_EBP)
        try:
            fn = self.u32(ebp - 0x74)
        except UcError:
            fn = 0
        try:
            at = self.u32(uc.reg_read(UC_X86_REG_ESP))
        except UcError:
            at = 0
        self.log.append("PBERR %d in 0x%x near 0x%x" % (code, fn, at))

    def _stub_hook(self, uc, address, size, _):
        if address == CBRET:
            fr = self.pending[-1]
            if uc.reg_read(UC_X86_REG_ESP) != fr["esp"]:
                self.warnings.append("callback from %s returned with esp 0x%x, expected 0x%x" % (
                    fr["name"], uc.reg_read(UC_X86_REG_ESP), fr["esp"]))
            self._step(fr, uc.reg_read(UC_X86_REG_EAX))
            return
        name = self.stubs.get(address, "?")
        esp = uc.reg_read(UC_X86_REG_ESP)
        ret = self.u32(esp)
        n = NARGS.get(name)
        if n is None:
            self.stub_error = "import %s has no argument count (add it to NARGS)" % name
            uc.emu_stop()
            return
        args = struct.unpack("<%dI" % n, uc.mem_read(esp + 4, 4 * n)) if n else ()
        try:
            rv = self.api(name, args, ret)
        except EmuFault as e:
            self.stub_error = str(e)
            uc.emu_stop()
            return
        if isinstance(rv, types.GeneratorType):
            fr = {"gen": rv, "esp": esp, "n": n, "ret": ret, "name": name, "args": args}
            self.pending.append(fr)
            self._step(fr, None)
            return
        uc.reg_write(UC_X86_REG_EAX, (rv or 0) & 0xFFFFFFFF)
        uc.reg_write(UC_X86_REG_ESP, esp + 4 * n)
        self.w32(esp + 4 * n, ret)

    def _step(self, fr, val):
        """Advance an API generator: either start the callback it asks for (by making the stub's `ret`
        jump to it, with CBRET as its return address) or return its result to the API's caller."""
        uc = self.uc
        try:
            req = fr["gen"].send(val)
        except StopIteration as stop:
            self.pending.pop()
            rv = stop.value or 0
            if fr["name"] not in QUIET and fr["name"] not in WM_QUIET:
                self._logcall(fr["name"], fr["args"], fr["ret"], rv)
            esp, n = fr["esp"], fr["n"]
            uc.reg_write(UC_X86_REG_EAX, rv & 0xFFFFFFFF)
            uc.reg_write(UC_X86_REG_ESP, esp + 4 * n)
            self.w32(esp + 4 * n, fr["ret"])
            return
        except EmuFault as e:
            self.stub_error = str(e)
            uc.emu_stop()
            return
        proc, cargs = req
        sp = fr["esp"] - 4 * len(cargs) - 8
        self.uc.mem_write(sp, struct.pack("<II%dI" % len(cargs), proc, CBRET, *[x & 0xFFFFFFFF for x in cargs]))
        uc.reg_write(UC_X86_REG_ESP, sp)

    def _logcall(self, name, args, ret, rv, extra=""):
        strs = STR_ARGS.get(name, ())
        parts = []
        for i, a in enumerate(args):
            if i in strs and a:
                parts.append(repr(self.cstr(a).decode("latin-1")))
            else:
                parts.append("0x%x" % a)
        self.log.append("API %s(%s) -> %d%s [from 0x%x]" % (name, ", ".join(parts), rv, extra, ret))

    def api(self, name, a, ret):
        h = self.heap
        uc = self.uc
        rv = 0
        if name in WM_FUNCS and not (name == "DialogBoxIndirectParamA" and a[3] == INPUTBOX_DLGPROC):
            return self.wm.api(name, a, ret)
        if name in FILE_FUNCS:
            rv = self.vfs.api(name, a)
            if name not in QUIET:
                self._logcall(name, a, ret, rv)
            return rv
        if name == "SysAllocStringByteLen":
            psz, n = a
            if n >= 0x10000000:
                rv = 0
            else:
                rv = h.alloc(n, zero=psz == 0)
                if psz:
                    uc.mem_write(rv, self.rd(psz, n))
                uc.mem_write(rv + n, b"\0\0")
        elif name == "SysFreeString":
            if a[0]:
                h.release(a[0])
        elif name == "SysStringByteLen":
            rv = self.u32(a[0] - 4) if a[0] else 0
        elif name == "GlobalAlloc":
            flags, n = a
            rv = h.alloc(n, zero=True)
        elif name == "GlobalReAlloc":
            p, n, flags = a
            old = h.live.get(p)
            if old is None:
                rv = 0
            else:
                oldn = self.u32(p - 4)
                rv = h.alloc(n, zero=True)
                uc.mem_write(rv, self.rd(p, min(oldn, n)))
                h.release(p)
        elif name == "GlobalFree":
            rv = 0 if (not a[0] or h.release(a[0])) else a[0]
        elif name == "GlobalLock":
            rv = a[0]
        elif name == "GlobalUnlock":
            rv = 1
        elif name == "TlsAlloc":
            rv = len(self.tls) + 1
            self.tls[rv] = 0
        elif name == "TlsGetValue":
            rv = self.tls.get(a[0], 0)
        elif name == "TlsSetValue":
            self.tls[a[0]] = a[1]
            rv = 1
        elif name == "TlsFree":
            rv = 1
        elif name in ("CharUpperBuffA", "CharLowerBuffA"):
            p, n = a
            up = name == "CharUpperBuffA"
            uc.mem_write(p, bytes(_case1252(b, up) for b in self.rd(p, n)))
            rv = n
        elif name == "CharNextA":
            p = a[0]
            rv = p + 1 if self.rd(p, 1)[0] else p
        elif name == "GetStringTypeA":
            lcid, typ, src, n, out = a
            import unicodedata
            vals = []
            for b in self.rd(src, n):
                c = bytes([b]).decode("cp1252", errors="replace")
                vals.append(0x100 if unicodedata.category(c).startswith("L") else 0)
            uc.mem_write(out, struct.pack("<%dH" % n, *vals))
            rv = 1
        elif name == "MultiByteToWideChar":
            cp, fl, src, n, dst, dn = a
            s = self.rd(src, n) if n != 0xFFFFFFFF else self.cstr(src) + b"\0"
            if dn:
                uc.mem_write(dst, s.decode("latin-1").encode("utf-16-le")[: 2 * dn])
            rv = len(s)
        elif name == "WideCharToMultiByte":
            cp, fl, src, n, dst, dn, d1, d2 = a
            if n == 0xFFFFFFFF:
                raw = b""
                while True:
                    c = self.rd(src + len(raw), 2)
                    raw += c
                    if c == b"\0\0":
                        break
            else:
                raw = self.rd(src, 2 * n)
            s = raw.decode("utf-16-le").encode("latin-1", errors="replace")
            if dn:
                uc.mem_write(dst, s[:dn])
            rv = len(s)
        elif name == "GetLastError":
            rv = self.last_error
        elif name == "SetLastError":
            self.last_error = a[0]
        elif name == "GetLocalTime":
            uc.mem_write(a[0], struct.pack("<8H", *self.local_time))
        elif name == "GetModuleHandleA":
            rv = BASE
        elif name == "GetCommandLineA":
            rv = self.cmdline
        elif name == "GetSysColor":
            rv = 0xFFFFFF
        elif name == "MessageBoxA":
            hwnd, text, cap, typ = a
            kind = typ & 0xF
            if self.answers and not is_dialog_action(self.answers[0]):
                ans = self.answers.pop(0)
                rv = ANSWERS[ans] if ans in ANSWERS else int(ans)
            else:
                rv = 6 if kind in (3, 4) else 1
            self.log.append("MSGBOX %r %r type=0x%x -> %d [from 0x%x]" % (
                self.cstr(cap).decode("latin-1"), self.cstr(text).decode("latin-1"), typ, rv, ret))
            return rv
        elif name == "DialogBoxIndirectParamA" and a[3] == INPUTBOX_DLGPROC:
            # INPUTBOX$: the dialog proc ends the dialog with a new BSTR of the edit text (OK) or 0 (Cancel)
            ans = self.answers.pop(0) if self.answers and not is_dialog_action(self.answers[0]) else ""
            rv = self.heap.bstr(_bytes(ans))
            self.log.append("INPUTBOX -> %r [from 0x%x]" % (ans, ret))
            return rv
        elif name in ("CreateMenu", "CreatePopupMenu"):
            rv = 0x2800
        elif name in ("GetDC", "CreateCompatibleDC", "CreateSolidBrush", "CreatePen", "CreateFontIndirectA",
                      "GetStockObject", "SelectObject"):
            rv = 0x2000
        elif name in ("ReleaseDC", "DeleteObject", "DeleteDC", "SetWindowTextA", "ShowWindow", "UpdateWindow",
                      "InvalidateRect", "EnableWindow", "DestroyWindow", "EndDialog", "PostMessageA",
                      "SetForegroundWindow", "TextOutA", "MoveToEx", "LineTo", "FillRect",
                      "SetWindowPos", "RedrawWindow", "DrawMenuBar", "SetMenuItemInfoA", "InsertMenuItemA",
                      "GetClientRect", "GetWindowRect", "SystemParametersInfoA", "GetVersionExA"):
            rv = 1
        elif name == "RemarksData":
            rv = self.empty_sz
        elif name == "ExitProcess":
            raise EmuFault("ExitProcess(%d) called" % a[0])
        if name not in QUIET and name not in WM_QUIET:
            self._logcall(name, a, ret, rv)
        return rv

    # -- state in/out
    def write_state(self, snap):
        uc = self.uc
        h = self.heap
        for key, name, typ, bounds in layout():
            if key not in snap:
                continue
            v = snap[key]
            addr = int(key, 16)
            size, fmt = TYPES[typ]
            if bounds is None:
                uc.mem_write(addr, self._enc(fmt, v))
                continue
            counts = [hi - lo + 1 for lo, hi in bounds]
            total = 1
            for c in counts:
                total *= c
            if len(v) != total:
                raise EmuFault("%s (%s): %d elements, expected %d" % (name, key, len(v), total))
            tmpl = self.rd(addr, 0x74)
            tcode, tnd, tsz = tmpl[6], tmpl[7], struct.unpack_from("<I", tmpl, 0x14)[0]
            if tnd != len(bounds) or tsz != size:
                self.warnings.append("%s: template ndims %d size %d vs %d/%d" % (name, tnd, tsz, len(bounds), size))
            if tcode == 0:
                tcode = TYPE_CODE.get(typ, 0)
            if tmpl[4] & 4:
                cur = [struct.unpack_from("<ii", tmpl, 0xC + 12 * (i + 1)) for i in range(tnd)]
                if cur != [tuple(b) for b in bounds]:
                    self.warnings.append("%s: PBMAIN DIMs it as %s, state.rs says %s" % (name, cur, bounds))
            if fmt == "str":
                ptrs = [h.bstr(_bytes(s)) for s in v]
                blob = struct.pack("<%dI" % total, *ptrs)
            elif fmt == "ext":
                blob = b"".join(encode_ext(x) for x in v)
            elif fmt in "fd":
                blob = struct.pack("<%d%s" % (total, fmt), *[_flt(x) for x in v])
            else:
                blob = struct.pack("<%d%s" % (total, fmt), *[int(x) for x in v])
            data = h.alloc(len(blob), zero=False)
            uc.mem_write(data, blob)
            d = bytearray(0x74)
            struct.pack_into("<I", d, 0, data)
            d[4], d[5], d[6], d[7] = 4, 2, tcode, len(bounds)
            struct.pack_into("<III", d, 8, total, len(blob), 0)
            struct.pack_into("<I", d, 0x14, size)
            prod = 1
            for i, (lo, hi) in enumerate(bounds):
                dd = i + 1
                struct.pack_into("<ii", d, 0xC + 12 * dd, lo, hi)
                if dd >= 2:
                    struct.pack_into("<I", d, 0x14 + 12 * (dd - 1), prod)
                prod *= hi - lo + 1
            uc.mem_write(addr, bytes(d))

    def _enc(self, fmt, v):
        if fmt == "str":
            return struct.pack("<I", self.heap.bstr(_bytes(v)))
        if fmt == "ext":
            return encode_ext(v)
        if fmt in "fd":
            return struct.pack("<" + fmt, _flt(v))
        return struct.pack("<" + fmt, int(v))

    def _dec(self, fmt, b, what=None):
        if fmt == "ext":
            return decode_ext(b)
        if fmt == "str":
            return self.read_bstr(struct.unpack("<I", b)[0], what).decode("latin-1")
        x = struct.unpack("<" + fmt, b)[0]
        return _jflt(x) if fmt in "fd" else x

    def fingerprint(self):
        """Raw bytes of every global (array data blocks, string contents), keyed like the snapshot: cheap
        to compute, used to find which fields a run changed without decoding the whole state."""
        fp = {}
        for key, name, typ, bounds in layout():
            addr = int(key, 16)
            size, fmt = TYPES[typ]
            if bounds is None:
                b = self.rd(addr, size)
                if fmt == "str":
                    b += self.read_bstr(struct.unpack("<I", b)[0], name)
                fp[key] = b
                continue
            d = self.rd(addr, 0x74)
            total = struct.unpack_from("<I", d, 8)[0] if d[4] & 4 else 0
            esz = struct.unpack_from("<I", d, 0x14)[0] or size
            data = struct.unpack_from("<I", d, 0)[0]
            blob = self.rd(data, total * esz) if total else b""
            if fmt == "str" and total:
                blob += b"\0".join(self.read_bstr(q, name) for q in struct.unpack("<%dI" % total, blob))
            fp[key] = d + blob
        return fp

    def read_state(self, keys=None):
        out = {}
        for key, name, typ, bounds in layout():
            if keys is not None and key not in keys:
                continue
            addr = int(key, 16)
            size, fmt = TYPES[typ]
            if bounds is None:
                out[key] = self._dec(fmt, self.rd(addr, size), name)
                continue
            counts = [hi - lo + 1 for lo, hi in bounds]
            total = 1
            for c in counts:
                total *= c
            d = self.rd(addr, 0x74)
            dflt = "" if fmt == "str" else (0.0 if fmt in ("ext", "f", "d") else 0)
            if not d[4] & 4:
                if d[0:4] != b"\0\0\0\0":
                    self.warnings.append("%s: descriptor not dimensioned" % name)
                out[key] = [dflt] * total
                continue
            nd = d[7]
            cur = [struct.unpack_from("<ii", d, 0xC + 12 * (i + 1)) for i in range(nd)]
            data = struct.unpack_from("<I", d, 0)[0]
            esz = struct.unpack_from("<I", d, 0x14)[0]
            if cur == [tuple(b) for b in bounds] and esz == size:
                blob = self.rd(data, total * size)
                out[key] = self._decode_list(fmt, blob, total, size)
            else:
                self.warnings.append("%s: bounds changed to %s" % (name, cur))
                vals = []
                ccounts = [hi - lo + 1 for lo, hi in cur]
                for flat in range(total):
                    idx, r = [], flat
                    for (lo, hi), c in zip(bounds, counts):
                        idx.append(lo + r % c)
                        r //= c
                    off, mul, ok = 0, 1, len(idx) == nd
                    for (lo, hi), c, i in zip(cur, ccounts, idx):
                        ok = ok and lo <= i <= hi
                        off += (i - lo) * mul
                        mul *= c
                    vals.append(self._dec(fmt, self.rd(data + off * esz, size)) if ok else dflt)
                out[key] = vals
        return out

    def _decode_list(self, fmt, blob, total, size):
        if fmt == "ext":
            return [decode_ext(blob[i:i + 10]) for i in range(0, total * 10, 10)]
        if fmt == "str":
            return [self.read_bstr(p, "array element").decode("latin-1") for p in struct.unpack("<%dI" % total, blob)]
        vals = list(struct.unpack("<%d%s" % (total, fmt), blob))
        return [_jflt(x) for x in vals] if fmt in "fd" else vals

    # -- call
    def make_args(self, args):
        """Returns (stack dwords, [(kind, cell address)] for reading BYREF results back)."""
        vals, cells = [], []
        h = self.heap
        for x in args:
            kind, v = ("l", x) if isinstance(x, int) else x
            if kind == "v":
                vals.append(int(v) & 0xFFFFFFFF)
                cells.append(None)
                continue
            if kind == "vs":
                vals.append(h.bstr(_bytes(v)))
                cells.append(None)
                continue
            fmt = {"l": "i", "i": "h", "q": "q", "e": "ext", "d": "d", "f": "f", "s": "str"}[kind]
            cell = h.alloc(16)
            self.uc.mem_write(cell, self._enc(fmt, v))
            vals.append(cell)
            cells.append((fmt, cell))
        return vals, cells

    def call(self, addr, args):
        uc = self.uc
        vals, cells = self.make_args(args)
        # trampoline: fninit; fldcw [CW_ADDR]; mov eax, addr; jmp eax
        code = b"\xdb\xe3" + b"\xd9\x2d" + struct.pack("<I", CW_ADDR) + b"\xb8" + struct.pack("<I", addr) + b"\xff\xe0"
        uc.mem_write(TRAMP, code)
        # return sentinel: fnstsw [SW_ADDR]; fstp tword [ST0_ADDR]; hlt  (captures an ST0 return value)
        sent = b"\xdd\x3d" + struct.pack("<I", SW_ADDR) + b"\xdb\x3d" + struct.pack("<I", ST0_ADDR)
        uc.mem_write(SENTINEL, sent + b"\x90" * (SENTINEL_END - SENTINEL - len(sent)) + b"\xf4")
        # Persistent campaign calls can target different routines. Unicorn retains translated
        # blocks across mem_write(), so invalidate the rewritten trampoline before executing it.
        uc.ctl_remove_cache(TRAMP, TRAMP + 0x1000)
        uc.mem_write(CW_ADDR, struct.pack("<H", FPU_CW))
        esp = STACK + STACK_SIZE - 0x1000
        for v in reversed(vals):
            esp -= 4
            self.w32(esp, v)
        esp -= 4
        self.w32(esp, SENTINEL)
        uc.reg_write(UC_X86_REG_ESP, esp)
        uc.reg_write(UC_X86_REG_EBP, STACK + STACK_SIZE - 0x100)
        for r in ("eax", "ebx", "ecx", "edx", "esi", "edi"):
            uc.reg_write(REGS[r], 0)
        uc.reg_write(UC_X86_REG_EFLAGS, 0x202)
        err = None
        t0 = time.time()
        try:
            uc.emu_start(TRAMP, SENTINEL_END, timeout=self.timeout, count=self.budget)
        except UcError as e:
            err = "fault: %s" % e
            if self.bad_access:
                acc, a, n = self.bad_access
                err += " (access type %d at 0x%x size %d)" % (acc, a, n)
        self.elapsed = time.time() - t0
        eip = uc.reg_read(UC_X86_REG_EIP)
        if err is None and self.stub_error:
            err = self.stub_error
        if err is None and eip != SENTINEL_END:
            err = "stopped at 0x%x (instruction budget %d / timeout %.1fs exhausted)" % (
                eip, self.budget, self.timeout / 1e6)
        regs = {k: uc.reg_read(r) for k, r in REGS.items()}
        # ST0 at return (if the FPU stack was not empty: TOP != 0 after fninit) and EAX as a BSTR
        sw = struct.unpack("<H", self.rd(SW_ADDR, 2))[0]
        has_st0 = err is None and (sw >> 11) & 7
        self.ret_float = ext_to_f64(self.rd(ST0_ADDR, 10)) if has_st0 else None
        self.ret_float_raw = self.rd(ST0_ADDR, 10)[::-1].hex() if has_st0 else None  # 20 hex digits, big-endian
        self.ret_str = self.read_bstr(regs["eax"]) if regs["eax"] in self.heap.live else None
        args_out = [self._dec(c[0], self.rd(c[1], 10 if c[0] == "ext" else TYPES_BY_FMT[c[0]])) if c else None
                    for c in cells]
        return err, regs, args_out


TYPES_BY_FMT = {"i": 4, "h": 2, "q": 8, "d": 8, "f": 4, "str": 4}


def _case1252(b, up):
    """CharUpperBuffA/CharLowerBuffA on code page 1252 (as tools/emu/pbstring.py)."""
    if b in (0x81, 0x8D, 0x8F, 0x90, 0x9D):
        return b
    c = bytes([b]).decode("cp1252")
    u = c.upper() if up else c.lower()
    if len(u) != 1:
        return b
    try:
        e = u.encode("cp1252")
    except UnicodeEncodeError:
        return b
    return e[0] if len(e) == 1 else b


def _bytes(s):
    if isinstance(s, (bytes, bytearray)):
        return bytes(s)
    return s.encode("latin-1")


def _flt(x):
    return ext_to_f64(encode_ext(x)) if isinstance(x, str) and x.startswith("x87:") else math.nan if x is None else float(x)


def encode_ext(value):
    """Lossless x87 snapshot item, or a legacy JSON DOUBLE value."""
    if isinstance(value, str) and value.startswith("x87:"):
        if len(value) != 24:
            raise ValueError("Invalid x87 snapshot item")
        return struct.pack("<QH", int(value[8:], 16), int(value[4:8], 16))
    return f64_to_ext(_flt(value))


def decode_ext(raw):
    mant, se = struct.unpack("<QH", raw)
    exponent = se & 0x7fff
    if mant == 0 and exponent == 0 or 15361 <= exponent <= 17406 and mant & 0x7ff == 0:
        return ext_to_f64(raw)
    return f"x87:{se:04x}{mant:016x}"


def _jflt(x):
    return None if (x != x or math.isinf(x)) else x


def disasm_trace(emu, trace, last_eip):
    from capstone import CS_ARCH_X86, CS_MODE_32, Cs
    md = Cs(CS_ARCH_X86, CS_MODE_32)
    names = {v: k for k, v in emu.stubs.items()}
    lines = []
    for a, n in trace[-12:]:
        try:
            code = emu.rd(a, n)
        except UcError:
            lines.append("  %08x <unreadable>" % a)
            continue
        if STUB <= a < STUB + 0x10000:
            lines.append("  %08x <import %s>" % (a, emu.stubs.get(a, "?")))
            continue
        for ins in md.disasm(code, a):
            mark = " <==" if ins.address == last_eip else ""
            lines.append("  %08x %s %s%s" % (ins.address, ins.mnemonic, ins.op_str, mark))
    return "\n".join(lines)


# ---------------------------------------------------------------- API
# A booted emulator to reuse (set by `serve` in the forked child; see serve()) and its decoded state.
_PREBOOT = None
_BOOT_STATE = {}
_UNDIMMED = set()  # array globals whose descriptor is not dimensioned after the boot


def _configure(emu, answers, budget, timeout, hooks, files, default_files, skips):
    """Apply per-call options to an already booted Emu (same effect as passing them to Emu()))."""
    emu.answers = list(answers)
    emu.budget = budget
    emu.timeout = timeout
    for h in hooks:
        emu.uc.hook_add(UC_HOOK_CODE, emu._call_hook, begin=h, end=h)
        emu.hooks[h] = func_names().get(h, "sub_%x" % h)
    for addr, n in skips:
        emu.uc.hook_add(UC_HOOK_CODE, emu._skip_hook, begin=addr, end=addr)
        emu.skips[addr] = (n, func_names().get(addr, "sub_%x" % addr))
    emu.vfs = VFS(emu, files, default_files)


def run(addr, args=(), state_json=None, answers=(), exe=DEFAULT_EXE, budget=0, timeout=60_000_000,
        hooks=(), rng=None, files=None, default_files=True, skips=(), _trace=False):
    """Run the original routine at `addr` (stdcall) on `state_json` (str, dict or None = EXE initial
    state). Returns dict(state_json, state, ui_log, return_value, err, args_out, regs, elapsed, warnings)."""
    if isinstance(state_json, str):
        snap = json.loads(state_json)
    else:
        snap = state_json or {}
    delta = _PREBOOT is not None and not _trace and exe == DEFAULT_EXE
    if delta:
        emu = _PREBOOT
        _configure(emu, answers, budget, timeout, hooks, files, default_files, skips)
        # the booted memory already holds _BOOT_STATE: write only what differs
        # (arrays the boot did not DIM must still get a dimensioned descriptor)
        snap = {k: v for k, v in snap.items() if k in _UNDIMMED or _BOOT_STATE.get(k) != v}
    else:
        emu = Emu(exe, answers, trace=_trace, budget=budget, timeout=timeout, hooks=hooks, files=files,
                  default_files=default_files, skips=skips)
    emu.write_state(snap)
    fp0 = emu.fingerprint() if delta else None
    if rng is not None:
        emu.w32(RNG_STATE, rng)
    err, regs, args_out = emu.call(addr, list(args))
    if err and not _trace and not err.startswith("stopped"):  # repeat with a block trace for the report
        t = run(addr, args, snap, answers, exe, budget, timeout, hooks, rng, files, default_files, skips,
                _trace=True)
        return t
    if err and not _trace:
        err += "\nregs: " + " ".join("%s=%08x" % (k, v) for k, v in regs.items())
        err += "\nat:\n" + disasm_trace(emu, [(regs["eip"], 32)], regs["eip"])
    elif err and _trace:
        err += "\nregs: " + " ".join("%s=%08x" % (k, v) for k, v in regs.items())
        err += "\nlast basic blocks:\n" + disasm_trace(emu, emu.trace or [], regs["eip"])
        err += "\nlast log lines:\n  " + "\n  ".join(emu.log[-10:])
    if delta:
        # only fields whose bytes changed during the run (callers keep their input for the rest)
        fp1 = emu.fingerprint()
        st = emu.read_state({k for k, b in fp1.items() if fp0.get(k) != b})
    else:
        st = emu.read_state()
    return {
        "state": st,
        "state_json": dump_snapshot(st),
        "ui_log": emu.log,
        "return_value": regs["eax"],
        "err": err,
        "args_out": args_out,
        "regs": regs,
        "elapsed": emu.elapsed,
        "warnings": emu.warnings,
        "rng": emu.u32(RNG_STATE),
        "files": emu.vfs.outputs(),
        "return_float": _jflt(emu.ret_float) if emu.ret_float is not None else None,
        "return_float_raw": emu.ret_float_raw,
        "return_str": None if emu.ret_str is None else emu.ret_str.decode("latin-1"),
        "dialogs": emu.wm.models(),
    }


def _jstr(s):
    # same escaping as wsr_core::snapshot (its reader knows only \" \\ \n \r \t \u, not \b \f)
    out = ['"']
    for c in s:
        o = ord(c)
        if c == '"':
            out.append('\\"')
        elif c == "\\":
            out.append("\\\\")
        elif 0x20 <= o <= 0x7E:
            out.append(c)
        else:
            out.append("\\u%04x" % o)
    out.append('"')
    return "".join(out)


def _jval(v):
    if isinstance(v, str):
        return _jstr(v)
    if v is None:
        return "null"
    if isinstance(v, float):
        return repr(v)
    return str(v)


def dump_snapshot(st):
    """Snapshot JSON in the format of `Globals::to_json` (key order = state.rs order)."""
    parts = []
    for k, v in st.items():
        if isinstance(v, list):
            parts.append('"%s":[%s]' % (k, ",".join(map(_jval, v))))
        else:
            parts.append('"%s":%s' % (k, _jval(v)))
    return "{" + ",".join(parts) + "}"


def initial_state(exe=DEFAULT_EXE):
    emu = Emu(exe)
    return emu.read_state()


def diff_states(a, b):
    """[(name, index text, old, new)] of changed fields."""
    out = []
    for key, name, typ, bounds in layout():
        x, y = a.get(key), b.get(key)
        if bounds is None:
            if x != y:
                out.append((name, "", x, y))
            continue
        counts = [hi - lo + 1 for lo, hi in bounds]
        for flat, (p, q) in enumerate(zip(x or [], y or [])):
            if p != q:
                idx, r = [], flat
                for (lo, hi), c in zip(bounds, counts):
                    idx.append(lo + r % c)
                    r //= c
                out.append((name, "(%s)" % ", ".join(map(str, idx)), p, q))
    return out


def parse_arg(s):
    if ":" in s:
        k, v = s.split(":", 1)
        if k in ("v", "i", "q", "l"):
            return (k, int(v, 0))
        if k == "e" and v.startswith("x87:"):
            encode_ext(v)  # Validate the raw representation before allocating a BYREF cell.
            return (k, v)
        if k in ("e", "d", "f"):
            return (k, float(v))
        if k in ("s", "vs"):
            return (k, v)
        raise SystemExit("bad arg spec %r" % s)
    return int(s, 0)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("cmd", choices=["run", "diff", "dump"])
    ap.add_argument("addr", nargs="?")
    ap.add_argument("state_pos", nargs="?")
    ap.add_argument("--state")
    ap.add_argument("--arg", action="append", default=[])
    ap.add_argument("--answer", action="append", default=[])
    ap.add_argument("--out")
    ap.add_argument("--log")
    ap.add_argument("--result")
    ap.add_argument("--budget", type=int, default=0,
                    help="instruction budget (0 = unlimited; counting slows emulation ~4x)")
    ap.add_argument("--timeout", type=float, default=60, help="seconds (0 = none)")
    ap.add_argument("--exe", default=DEFAULT_EXE)
    ap.add_argument("--hook", action="append", default=[], help="log calls of this routine (hex address)")
    ap.add_argument("--rng", type=lambda x: int(x, 0), help="set the RND state dword 0x6c3280 first")
    ap.add_argument("--file", action="append", default=[], help="NAME=PATH: virtual file NAME with PATH's bytes")
    ap.add_argument("--files", help="JSON file {name: base64} of virtual files")
    ap.add_argument("--no-default-files", action="store_true", help="don't fall back to extracted/app/ files")
    ap.add_argument("--files-out", help="directory to write the files the run wrote/created into")
    ap.add_argument("--skip", action="append", default=[],
                    help="ADDR[:NARGS]: make this routine return 0 at once (stdcall, NARGS dword args)")
    o = ap.parse_args(argv)
    if o.cmd == "dump":
        st = initial_state(o.exe)
        s = dump_snapshot(st)
        if o.out:
            open(o.out, "w").write(s)
        else:
            print(s)
        return 0
    addr = int(o.addr, 16)
    sp = o.state or o.state_pos
    state = json.load(open(sp, encoding="utf-8")) if sp else None
    args = [parse_arg(a) for a in o.arg]
    files = {}
    if o.files:
        files.update({k: base64.b64decode(v) for k, v in json.load(open(o.files)).items()})
    for f in o.file:
        k, path = f.split("=", 1)
        files[k] = open(path, "rb").read()
    skips = []
    for sk in o.skip:
        a, _, n = sk.partition(":")
        skips.append((int(a, 16), int(n or 0)))
    r = run(addr, args, state, o.answer, exe=o.exe, budget=o.budget, timeout=int(o.timeout * 1e6),
            hooks=[int(h, 16) for h in o.hook], rng=o.rng, files=files, default_files=not o.no_default_files,
            skips=skips)
    if o.files_out:
        os.makedirs(o.files_out, exist_ok=True)
        for k, v in r["files"].items():
            if v is not None:
                open(os.path.join(o.files_out, vkey(k)), "wb").write(v)
    if o.cmd == "diff":
        before = state if state is not None else initial_state(o.exe)
        full = dict(initial_state(o.exe)) if state is not None else before
        full.update(before)
        for name, idx, p, q in diff_states(full, r["state"]):
            print("%s%s: %r -> %r" % (name, idx, p, q))
        for ln in r["ui_log"]:
            print("#", ln)
        for k, v in r["files"].items():
            print("# file %s: %s" % (k, "deleted" if v is None else "%d bytes" % len(v)))
        for d in r["dialogs"]:
            print("# dialog 0x%x %r closed by %s" % (d["hwnd"], d["title"].decode("latin-1"), d["closed_by"]))
            for c in d["controls"]:
                print("#   %s id=%d %r%s" % (c["class"].decode("latin-1"), c["id"], c["text"].decode("latin-1"),
                                          " items:" if c["items"] else ""))
                for x in c["items"]:
                    print("#     %r" % x.decode("latin-1"))
        if r["return_str"] is not None or r["return_float"] is not None:
            print("# return_str %r, return_float %r" % (r["return_str"], r["return_float"]))
        for w in r["warnings"]:
            print("# warning:", w)
        print("# return 0x%x, args %r, rng 0x%x, %.2fs" % (r["return_value"], r["args_out"], r["rng"], r["elapsed"]))
        if r["err"]:
            print("ERROR:", r["err"], file=sys.stderr)
            return 1
        return 0
    if o.out:
        open(o.out, "w", encoding="utf-8").write(r["state_json"])
    if o.log:
        with open(o.log, "w", encoding="utf-8") as f:
            for ln in r["ui_log"]:
                f.write(ln.replace("\n", "\\n") + "\n")
    meta = {k: r[k] for k in ("return_value", "err", "args_out", "elapsed", "warnings", "rng")}
    meta["files"] = {k: (None if v is None else base64.b64encode(v).decode()) for k, v in r["files"].items()}
    for k in ("return_float", "return_float_raw", "return_str"):
        meta[k] = r[k]
    # dialogs, flat (for simple readers): dialogs = n; dlg<i>.title/.closed_by/.ids; dlg<i>.c<id>.class/.text/.items
    meta["dialogs"] = len(r["dialogs"])
    for i, d in enumerate(r["dialogs"]):
        meta["dlg%d.title" % i] = d["title"].decode("latin-1")
        meta["dlg%d.closed_by" % i] = d["closed_by"] or ""
        meta["dlg%d.ids" % i] = [c["id"] for c in d["controls"]]
        for c in d["controls"]:
            pre = "dlg%d.c%d." % (i, c["id"])
            meta[pre + "class"] = c["class"].decode("latin-1")
            meta[pre + "text"] = c["text"].decode("latin-1")
            meta[pre + "items"] = [x.decode("latin-1") for x in c["items"]]
            meta[pre + "cursel"] = c["cursel"]
    meta["files_written"] = [vkey(k) for k, v in r["files"].items() if v is not None]
    meta["files_deleted"] = [vkey(k) for k, v in r["files"].items() if v is None]
    if o.result:
        json.dump(meta, open(o.result, "w"))
    if not o.out:
        print(json.dumps(dict(meta, state=r["state"], ui_log=r["ui_log"])))
    if r["err"]:
        print(r["err"], file=sys.stderr)
        return 1
    return 0


def serve():
    """Request server for the test harness: boots the original once, then for every request line (a JSON
    list of `run` CLI arguments) forks a child that reuses the booted emulator (copy-on-write) and runs
    main() with those arguments. Replies one line per request: the child's exit code. Saves the Python
    start-up and the boot per call; the child also writes only fields that differ from the booted state
    and returns only fields the run changed (`--out` is then a partial snapshot, which
    `Globals::from_json` applies on top of the input state)."""
    global _PREBOOT, _BOOT_STATE, _UNDIMMED
    _PREBOOT = Emu(DEFAULT_EXE)
    _BOOT_STATE = _PREBOOT.read_state()
    _UNDIMMED = {key for key, name, typ, bounds in layout()
                 if bounds is not None and not _PREBOOT.rd(int(key, 16), 0x74)[4] & 4}
    func_names()  # warm caches shared by the children
    layout()
    out = sys.stdout
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        argv = json.loads(line)
        pid = os.fork()
        if pid == 0:
            code = 1
            try:
                code = main(argv) or 0
            except BaseException as e:  # report and exit; never return into the server loop
                print("game.py child error: %r" % e, file=sys.stderr)
            finally:
                sys.stdout.flush()
                sys.stderr.flush()
                os._exit(code)
        _, status = os.waitpid(pid, 0)
        out.write("%d\n" % os.waitstatus_to_exitcode(status))
        out.flush()


if __name__ == "__main__":
    if sys.argv[1:2] == ["serve"]:
        serve()
        sys.exit(0)
    sys.exit(main())
