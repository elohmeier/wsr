#!/usr/bin/env python3
"""Field list of WSR save files, read off SaveGame in the BASIC view.

Prints the WRITE # statements of SaveGame (0x4857af) with their enclosing FOR/IF structure, and the
description of each named global from tools/names.tsv. Output: decompiled/savegame-format.txt

Usage: tools/pbsave.py [decompiled/bas]
"""
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def main():
    src = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, 'decompiled', 'bas')
    desc = {}
    for ln in open(os.path.join(ROOT, 'tools', 'names.tsv')):
        f = ln.rstrip('\n').split('\t')
        if len(f) >= 4 and re.match(r'^6c[0-9a-f]{4}$', f[0]):
            desc[f[1]] = f[3]
    body = None
    for fn in sorted(os.listdir(src)):
        m = re.search(r'(?ms)^// ==== 004857af .*?(?=^// ==== )', open(os.path.join(src, fn)).read())
        if m:
            body = m.group(0)
    if body is None:
        sys.exit('SaveGame (004857af) not found')
    out, line = [], 0
    for ln in body.split('\n'):
        st = ln.strip()
        if st.startswith(('FOR ', '} NEXT', 'if ', 'else', 'SELECT', 'CASE', '} END SELECT', 'while', 'do {', '} while')) \
                or st.startswith('OPEN ') or st.startswith('CLOSE '):
            out.append(ln.rstrip())
        elif st.startswith('WRITE #'):
            items = [i.strip() for i in st[len('WRITE #'):].rstrip(';').split(',')[1:]]
            line += 1
            notes = [desc[re.match(r'[\w]+', i.lstrip('(int)').strip()).group(0)] for i in items
                     if re.match(r'[\w]+', i.lstrip('(int)').strip()) and re.match(r'[\w]+', i.lstrip('(int)').strip()).group(0) in desc]
            out.append('%s%-60s  # line %d%s' % (re.match(r'^\s*', ln).group(0), ', '.join(items), line,
                                                 ('  ' + notes[0][:90]) if notes else ''))
    p = os.path.join(ROOT, 'decompiled', 'savegame-format.txt')
    open(p, 'w').write('# GAMExx.DAT field order (plain text before obfuscation), from SaveGame 0x4857af\n'
                       '# "line N" counts WRITE statements in program order; lines inside FOR repeat per iteration.\n'
                       + '\n'.join(out) + '\n')
    print('%d WRITE statements -> %s' % (line, p))


if __name__ == '__main__':
    main()
