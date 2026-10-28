#!/usr/bin/env python3
"""Infer the PowerBASIC type of every scalar global in WSR.EXE from how the code accesses it.

x87 loads/stores give EXT (tword), DOUBLE/QUAD/CUR (qword), SINGLE/LONG (dword), INTEGER (word);
an address passed in EBX to string helpers (PB_StrPopAssign …) or loaded into EDX for PB_StrPushVar marks
a dynamic STRING; EBX passed to PB_CurLoad/Store marks CURRENCY. Writes decompiled/types.tsv.

Usage: tools/pbtypes.py [path/to/WSR.EXE]
"""
import collections
import os
import re
import sys

import capstone
import pefile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LO, HI = 0x6c3000, 0x6c8724          # runtime globals (below the literal pool)
USER_END = 0x6b71be
STR_EBX = {0x6be7b9, 0x6be82c, 0x6be86b, 0x6bef89, 0x6bf16f}   # PopAssign, PopTo, PopToArg, AsciizAssign, VarPtrLen
STR_EDX = {0x6bf0ac}                                             # PushVar
CUR_EBX = {0x6bcc93: 'CUR', 0x6bcca5: 'CUR', 0x6bcc9c: 'CUX', 0x6bccae: 'CUX'}


def main():
    exe = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, 'extracted', 'app', 'WSR.EXE')
    pe = pefile.PE(exe)
    text = pe.sections[0]
    base = pe.OPTIONAL_HEADER.ImageBase + text.VirtualAddress
    code = text.get_data()[:USER_END - base]
    md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_32)
    md.detail = True
    md.skipdata = True
    votes = collections.defaultdict(collections.Counter)
    ebx = edx = None
    for ins in md.disasm(code, base):
        if ins.id == 0:
            continue
        mn = ins.mnemonic
        if mn == 'call' and ins.operands and ins.operands[0].type == capstone.x86.X86_OP_IMM:
            t = ins.operands[0].imm
            if t in STR_EBX and ebx:
                votes[ebx]['STRING'] += 3
            if t in STR_EDX and edx:
                votes[edx]['STRING'] += 3
            if t in CUR_EBX and ebx:
                votes[ebx][CUR_EBX[t]] += 5
            ebx = edx = None
            continue
        for op in ins.operands:
            if op.type == capstone.x86.X86_OP_MEM and op.mem.base == 0 and op.mem.index == 0 and LO <= op.mem.disp < HI:
                a, sz = op.mem.disp, op.size
                if mn.startswith('fld') or mn.startswith('fst') or mn.startswith('fcom') or mn in ('fadd', 'fsub', 'fmul', 'fdiv', 'fsubr', 'fdivr'):
                    kind = {10: 'EXT', 8: 'DOUBLE', 4: 'SINGLE'}.get(sz, '?')
                elif mn.startswith('fi'):
                    kind = {8: 'QUAD', 4: 'LONG', 2: 'INTEGER'}.get(sz, '?')
                else:
                    kind = {4: 'LONG', 2: 'INTEGER', 1: 'BYTE'}.get(sz, '?')
                votes[a][kind] += 1
                if mn == 'mov' and ins.operands[0].type == capstone.x86.X86_OP_REG:
                    r = ins.reg_name(ins.operands[0].reg)
                    if r == 'edx':
                        edx = a
            elif op.type == capstone.x86.X86_OP_IMM and LO <= op.imm < HI and mn == 'mov':
                r = ins.reg_name(ins.operands[0].reg) if ins.operands[0].type == capstone.x86.X86_OP_REG else ''
                if r == 'ebx':
                    ebx = op.imm
    out = os.path.join(ROOT, 'decompiled', 'types.tsv')
    with open(out, 'w') as o:
        o.write('addr\ttype\tvotes\n')
        for a in sorted(votes):
            v = votes[a]
            if 'STRING' in v:
                t = 'STRING'
            elif 'CUR' in v or 'CUX' in v:
                t = 'CUR' if v['CUR'] >= v['CUX'] else 'CUX'
            elif 'EXT' in v:
                t = 'EXT'
            elif 'DOUBLE' in v:
                t = 'QUAD' if v['QUAD'] > v['DOUBLE'] else 'DOUBLE'
            elif 'QUAD' in v:
                t = 'QUAD'
            elif 'SINGLE' in v:
                t = 'SINGLE'
            else:
                t = v.most_common(1)[0][0]
            o.write('%x\t%s\t%s\n' % (a, t, ' '.join('%s:%d' % kv for kv in v.most_common())))
    print('%d globals typed -> %s' % (len(votes), out))


if __name__ == '__main__':
    main()
