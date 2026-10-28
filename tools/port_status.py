#!/usr/bin/env python3
"""Porting progress: which WSR.EXE routines have a Rust port (doc comment `Port of <Name> (0xADDR)`).

Counts game routines (SUB/FUNCTION + GOSUB blocks, not the PB runtime) from decompiled/functions.tsv /
tools/names.tsv, weighted by machine-code size, and lists PORT-TODO markers.

Usage: tools/port_status.py [--todo] [--missing]
"""
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def main():
    names = {}
    for ln in open(os.path.join(ROOT, 'tools', 'names.tsv')):
        f = ln.rstrip('\n').split('\t')
        if len(f) > 1 and re.match(r'^[0-9a-f]{6}$', f[0]) and not f[0].startswith('6c'):
            names[int(f[0], 16)] = f[1]
    size = {}
    for ln in open(os.path.join(ROOT, 'decompiled', 'functions.tsv')):
        f = ln.split('\t')
        if re.match(r'^[0-9a-f]{8}$', f[0]):
            size[int(f[0], 16)] = int(f[2])
    ported, todos = {}, []
    src = os.path.join(ROOT, 'engine', 'wsr-core', 'src')
    for dp, _, fs in os.walk(src):
        for fn in fs:
            if not fn.endswith('.rs'):
                continue
            p = os.path.join(dp, fn)
            for k, ln in enumerate(open(p), 1):
                # doc lines that only name a routine as missing don't count
                if ln.lstrip().startswith('///') and not re.search(r'PORT-TODO|not ported|unported|not yet ported', ln, re.I):
                    # `Port of Name (0xADDR)` or any doc line naming a routine together with its address
                    for m in re.finditer(r'\b0x([0-9a-f]{5,6})\b', ln):
                        a = int(m.group(1), 16)
                        if a in names and re.search(r'\b%s\b' % re.escape(names[a]), ln):
                            ported[a] = (names[a], os.path.relpath(p, ROOT))
                for m in re.finditer(r'PORT-TODO\(([^)]+)\)', ln):
                    todos.append((m.group(1), os.path.relpath(p, ROOT), k))
    game = {a: n for a, n in names.items() if a < 0x6b71be}
    tot = sum(size.get(a, 0) for a in game)
    done = sum(size.get(a, 0) for a in game if a in ported)
    print('ported routines: %d / %d  (%.1f%% of game code by size)' % (
        sum(1 for a in game if a in ported), len(game), 100.0 * done / max(tot, 1)))
    print('PORT-TODO markers: %d (%d distinct)' % (len(todos), len(set(t[0] for t in todos))))
    if '--todo' in sys.argv:
        for t in sorted(set(t[0] for t in todos)):
            a = int(t, 16) if t.startswith('0x') else None
            print('  %-10s %s' % (t, game.get(a, '') if a else ''))
    if '--missing' in sys.argv:
        for a in sorted(game, key=lambda a: -size.get(a, 0)):
            if a not in ported:
                print('  %06x %6d %s' % (a, size.get(a, 0), game[a]))


if __name__ == '__main__':
    main()
