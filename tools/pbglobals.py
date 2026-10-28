#!/usr/bin/env python3
"""Inventory of WSR.EXE globals from the BASIC-flavoured output (tools/pbbasic.py).

Writes decompiled/globals.tsv: address, current name, uses, functions, DIM, save-file slot,
and a few string literals seen on the same lines (a hint for naming).
With --dossier DIR it also writes per-batch text files with sample lines for each global.

Usage: tools/pbglobals.py [decompiled/bas] [--dossier DIR] [--batch N]
"""
import os
import re
import sys
from collections import defaultdict

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
G = re.compile(r'\b_?DAT_00(6c[3-8][0-9a-f]{3})\b')
SAVE_WRITERS = ('FUN_004857af', 'SaveGame')


def main():
    args = sys.argv[1:]
    src = args[0] if args and not args[0].startswith('--') else os.path.join(ROOT, 'decompiled', 'bas')
    dossier = args[args.index('--dossier') + 1] if '--dossier' in args else None
    batch = int(args[args.index('--batch') + 1]) if '--batch' in args else 150
    uses, funcs, dims, lines, save = defaultdict(int), defaultdict(set), {}, defaultdict(list), {}
    # the BASIC view may already carry names from tools/names.tsv: map them back to addresses
    global G
    rev = {}
    np = os.path.join(ROOT, 'tools', 'names.tsv')
    if os.path.exists(np):
        for ln in open(np):
            f = ln.rstrip('\n').split('\t')
            if len(f) > 1 and re.match(r'^6c[3-8][0-9a-f]{3}$', f[0]):
                rev[f[1]] = f[0]
    if rev:
        alt = '|'.join(sorted(map(re.escape, rev), key=len, reverse=True))
        pat = re.compile(r'\b_?DAT_00(6c[3-8][0-9a-f]{3})\b|\b(%s)\b' % alt)

        class _G:
            @staticmethod
            def findall(s):
                return [m.group(1) or rev[m.group(2)] for m in pat.finditer(s)]
        G = _G
    for fn in sorted(os.listdir(src)):
        if not fn.endswith('.c'):
            continue
        for part in re.split(r'(?m)^(?=// ==== )', open(os.path.join(src, fn)).read()):
            m = re.match(r'// ==== \w+ (\w+)', part)
            if not m or m.group(1).startswith(('pbrt_', 'PB_')):
                continue
            f = m.group(1)
            slot = 0
            for ln in part.split('\n'):
                st = ln.strip()
                if f in SAVE_WRITERS and st.startswith('WRITE #'):
                    for item in st.split(',')[1:]:
                        slot += 1
                        for a in G.findall(item):
                            save.setdefault(a, slot)
                d = re.match(r'DIM (\w+)\((.*)\) AS (\w+);', st)
                if d and G.findall(d.group(1)):
                    dims.setdefault(G.findall(d.group(1))[0], '(%s) AS %s' % (d.group(2), d.group(3)))
                for a in set(G.findall(ln)):
                    uses[a] += 1
                    funcs[a].add(f)
                    if len(lines[a]) < 400:
                        lines[a].append((f, st))
    out = os.path.join(ROOT, 'decompiled', 'globals.tsv')
    order = sorted(uses, key=lambda a: -uses[a])
    with open(out, 'w') as o:
        o.write('addr\tuses\tfuncs\tdim\tsave_slot\tliterals\n')
        for a in sorted(uses):
            lits = []
            for _, st in lines[a]:
                for lit in re.findall(r'"([^"]{3,60})"', st):
                    if lit not in lits:
                        lits.append(lit)
            o.write('%s\t%d\t%d\t%s\t%s\t%s\n' % (a, uses[a], len(funcs[a]), dims.get(a, ''), save.get(a, ''),
                                                 ' | '.join(lits[:6])))
    print('%d globals -> %s' % (len(uses), out))
    if dossier:
        os.makedirs(dossier, exist_ok=True)
        for b in range(0, len(order), batch):
            with open(os.path.join(dossier, 'batch%02d.txt' % (b // batch)), 'w') as o:
                for a in order[b:b + batch]:
                    o.write('### 0x%s  uses=%d funcs=%d dim=%s save_slot=%s\n' % (a, uses[a], len(funcs[a]),
                            dims.get(a, '-'), save.get(a, '-')))
                    # prefer lines with string literals, then diverse functions
                    ls = sorted(lines[a], key=lambda x: ('"' not in x[1], len(x[1]) > 220))
                    seen, picked = set(), []
                    for f, st in ls:
                        if st in seen or len(picked) >= 10:
                            continue
                        seen.add(st)
                        picked.append('  [%s] %s' % (f, st[:300]))
                    o.write('\n'.join(picked) + '\n\n')
        print('dossier batches: %d' % ((len(order) + batch - 1) // batch))


if __name__ == '__main__':
    main()
