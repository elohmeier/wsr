#!/usr/bin/env python3
"""Per-function dossiers from the BASIC-flavoured output, for naming SUB/FUNCTIONs.

For each user function: size, callers, callees, string literals, globals used and the first lines
of the body. Writes batches to DIR (default: decompiled/fdossier).

Usage: tools/pbfuncs.py [decompiled/bas] [--out DIR] [--batch N]
"""
import os
import re
import sys
from collections import defaultdict

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def main():
    args = sys.argv[1:]
    src = args[0] if args and not args[0].startswith('--') else os.path.join(ROOT, 'decompiled', 'bas')
    out = args[args.index('--out') + 1] if '--out' in args else os.path.join(ROOT, 'decompiled', 'fdossier')
    batch = int(args[args.index('--batch') + 1]) if '--batch' in args else 170
    funcs = {}
    for fn in sorted(os.listdir(src)):
        if not fn.endswith('.c'):
            continue
        for part in re.split(r'(?m)^(?=// ==== )', open(os.path.join(src, fn)).read()):
            m = re.match(r'// ==== (\w+) (\w+) \(size (\d+)\)', part)
            if m and not m.group(2).startswith(('pbrt_', 'PB_', 'entry')):
                funcs[m.group(2)] = (m.group(1), int(m.group(3)), part)
    callers = defaultdict(set)
    callees = {}
    for name, (_, _, body) in funcs.items():
        cs = set(re.findall(r'\b((?:FUN|GOSUB)_[0-9a-f]+)\(', body)) - {name}
        callees[name] = cs
        for c in cs:
            callers[c].add(name)
    os.makedirs(out, exist_ok=True)
    order = sorted(funcs, key=lambda n: funcs[n][0])
    for b in range(0, len(order), batch):
        with open(os.path.join(out, 'batch%02d.txt' % (b // batch)), 'w') as o:
            for name in order[b:b + batch]:
                addr, size, body = funcs[name]
                lits = []
                for lit in re.findall(r'"([^"]{3,80})"', body):
                    if lit not in lits:
                        lits.append(lit)
                gl = sorted(set(re.findall(r'\b(?:g_)?[A-Z][A-Za-z0-9]+(?=\(|\b)', body)) -
                            {'DIM', 'AS', 'TO', 'LEN', 'STR', 'VAL', 'MSGBOX', 'WRITE', 'PRINT', 'INPUT', 'OPEN',
                             'FOR', 'CLOSE', 'CONTROL', 'ADD', 'SET', 'TEXT', 'LABEL', 'DIALOG', 'END', 'RND',
                             'ROUND', 'ABS', 'INT', 'FIX', 'ERR', 'CALL', 'ON', 'ERROR', 'GOTO', 'ANY'})
                lines = [ln for ln in body.split('\n')[1:] if ln.strip() and not re.match(r'^\s+[\w ]+\**\w+(\[\w*\])?;$', ln)]
                o.write('### %s %s size=%d callers=%s callees=%s\n' % (
                    addr, name, size, ','.join(sorted(callers[name])[:8]) + ('…' if len(callers[name]) > 8 else ''),
                    ','.join(sorted(callees[name])[:12]) + ('…' if len(callees[name]) > 12 else '')))
                o.write('  strings: %s\n' % ' | '.join(lits[:14]))
                o.write('  names used: %s\n' % ' '.join(gl[:30]))
                o.write('\n'.join('  ' + ln.strip()[:200] for ln in lines[3:28]) + '\n\n')
    print('%d functions -> %s (%d batches)' % (len(order), out, (len(order) + batch - 1) // batch))


if __name__ == '__main__':
    main()
