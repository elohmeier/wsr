#!/usr/bin/env python3
"""Rewrite Ghidra's decompiled WSR.EXE C into BASIC-flavoured pseudocode.

PowerBASIC evaluates string expressions on a per-frame stack (PB_StrPush*, PB_StrConcat, ...,
PB_StrPopAssign). Ghidra shows that as stack-machine code. This pass replays the stack symbolically
per function and folds each sequence back into one statement:

    PB_StrPushVar(a); PB_StrPushLit("x"); PB_StrConcat(); PB_StrPopAssign(&b);   ->   b = a + "x";

It also inlines PB_ArrayElem temporaries as arr(i), renames GOSUB accesses to the caller's frame
(`*(int *)(unaff_EBP + -0x90)` -> `local_94`), drops PB_ErrCheck noise and applies names from
tools/names.tsv. Any sequence it cannot prove balanced is left verbatim.

Usage: tools/pbbasic.py [decompiled/c] [decompiled/bas] [--no-names]
"""
import os
import re
import sys
from collections import Counter

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# name -> (string pops, string pushes, template). {0}.. = popped strings in push order, {aN} = C args,
# {AN} = C arg turned into an lvalue. A template can also be a function (popped, args) -> text.
# Ops with pushes=0 produce an expression (or a statement when the call is the whole line).
CONTROL_KINDS = {1: 'BUTTON', 2: 'CHECK3STATE', 3: 'CHECKBOX', 4: 'FRAME', 5: 'LABEL', 6: 'LINE', 7: 'OPTION',
                 8: 'SCROLLBAR', 9: 'TEXTBOX', 10: 'IMAGE', 11: 'IMAGEX', 12: 'IMGBUTTON', 13: 'IMGBUTTONX',
                 14: 'GRAPHIC', 15: 'COMBOBOX', 16: 'LISTBOX'}
OPEN_MODES = {'0x1a1': 'INPUT', '0x261': 'OUTPUT'}


def num(a):
    try:
        return int(a, 0)
    except ValueError:
        return None


def control_add(p, a):
    kind = num(a[0]) if a else None
    if kind == 0:
        kind = 'BUTTON' if p[0] == '"Button"' else p[0]
    else:
        kind = CONTROL_KINDS.get(kind, a[0] if a else '?')
    rest = [a[1], a[2], p[1]] + a[4:8] if len(a) >= 8 else a[1:3] + [p[1]]
    if len(a) >= 10 and (a[8] != '0' or a[9] != '0'):
        rest += a[8:10]
    cb = ' CALL %s' % a[10] if len(a) >= 11 and a[10] not in ('0', '0x0') else ''
    return 'CONTROL ADD %s, %s%s' % (kind, ', '.join(rest), cb)


def chr_(p, a, pending):
    codes = pending[:] or ['?']
    pending.clear()
    return 'CHR$(%s)' % ', '.join(codes)


OPS = {
    'PB_StrPushLit': (0, 1, '{a0}'),
    'PB_StrPushVar': (0, 1, '{a0}'),
    'PB_StrPushTemp': (0, 1, '{a0}'),
    'PB_StrPushEmpty': (0, 1, '""'),
    'PB_StrConcat': (2, 1, '{0} + {1}'),
    'PB_StrPopAssign': (1, 0, '{A0} = {0}'),
    'PB_StrPopTo': (1, 0, '{A0} = {0}'),
    'PB_StrPopToArg': (1, 0, '{A0} = {0}'),
    'PB_StrPopFree': (1, 0, '{0}'),
    'PB_AsciizAssign': (1, 0, '{a0} = {0}'),
    'PB_StrEqual': (2, 0, '({0} == {1})'),
    'PB_StrLessEq': (2, 0, '({0} <= {1})'),
    'PB_StrGreaterEq': (2, 0, '({0} >= {1})'),
    'PB_StrLess': (2, 0, '({0} < {1})'),
    'PB_StrGreater': (2, 0, '({0} > {1})'),
    'PB_Len': (1, 0, 'LEN({0})'),
    'PB_Asc': (1, 0, 'ASC({0})'),
    'PB_Val': (1, 0, 'VAL({0})'),
    'PB_StrNum': (0, 1, 'STR$({a0})'),
    'PB_StrNum7': (0, 1, 'STR$({a0})'),
    'PB_StrNumDigits': (0, 1, 'STR$({a0}, {a1})'),
    'PB_FormatNum': (0, 1, 'FORMAT$({a0})'),
    'PB_Format': (1, 1, 'FORMAT$({a0}, {0})'),
    'PB_LTrim': (1, 1, 'LTRIM$({0})'),
    'PB_RTrim': (1, 1, 'RTRIM$({0})'),
    'PB_Right': (1, 1, 'RIGHT$({0}, {a0})'),
    'PB_Mid': (1, 1, 'MID$({0}, {a0}, {a1})'),
    'PB_MidAssign': (1, 0, 'MID$({a0}, {a2}, {a3}) = {0}'),
    'PB_Parse': (2, 1, 'PARSE$({0}, {1}, {a0})'),
    'PB_InstrAny': (2, 0, 'INSTR({a0}, {0}, ANY {1})'),
    'PB_RetainAny': (2, 1, 'RETAIN$({0}, ANY {1})'),
    'PB_ParseArray': (2, 0, 'PARSE {0}, {1}, {a0}()'),
    'PB_Remain': (2, 1, 'REMAIN$({a0}, {0}, {1})'),
    'PB_Extract': (2, 1, 'EXTRACT$({a0}, {0}, {1})'),
    'PB_StrPushAsciizPtr': (0, 1, '@{a0}'),
    'PB_ComboBoxGetText': (0, 1, 'COMBOBOX_GET_TEXT$({a0}, {a1})'),
    'PB_ListBoxGetText': (0, 1, 'LISTBOX_GET_TEXT$({a0}, {a1})'),
    'PB_XPrintFont': (1, 0, 'XPRINT FONT {0}, {a0}, {a1}'),
    'PB_XPrintAttach': (2, 0, 'XPRINT ATTACH {a0}, {0}, {1}'),
    'PB_XPrintTextSize': (1, 0, 'XPRINT_TEXT_SIZE({0})'),
    'PB_XPrintName': (0, 1, 'XPRINT$'),
    'PB_ParseAny': (2, 1, 'PARSE$({0}, ANY {1}, {a0})'),
    'PB_Command': (0, 1, 'COMMAND$'),
    'PB_Dir': (1, 1, 'DIR$({0}, {a0})'),
    'PB_ChDir': (1, 0, 'CHDIR {0}'),
    'PB_ChDrive': (1, 0, 'CHDRIVE {0}'),
    'PB_SetAttr': (1, 0, 'SETATTR {0}, {a0}'),
    'PB_ListBoxAdd': (1, 0, 'LISTBOX ADD {a0}, {a1}, {0}'),
    'PB_ComboBoxAdd': (1, 0, 'COMBOBOX ADD {a0}, {a1}, {0}'),
    'PB_ParseCount': (2, 0, 'PARSECOUNT({0}, {1})'),
    'PB_ParseCountAny': (2, 0, 'PARSECOUNT({0}, ANY {1})'),
    'PB_Instr': (2, 0, 'INSTR({a0}, {0}, {1})'),
    'PB_Replace': (3, 1, 'REPLACE$({2}, {0}, {1})'),
    'PB_ReplaceAny': (3, 1, 'REPLACE$({2}, ANY {0}, {1})'),
    'PB_Remove': (2, 1, 'REMOVE$({0}, {1})'),
    'PB_RemoveAny': (2, 1, 'REMOVE$({0}, ANY {1})'),
    'PB_StringFill': (1, 1, 'STRING$({a0}, {0})'),
    'PB_StringChr': (0, 1, 'STRING$({a0}, {a1})'),
    'PB_Space': (0, 1, 'SPACE$({a0})'),
    'PB_UCase': (1, 1, 'UCASE$({0})'),
    'PB_LCase': (1, 1, 'LCASE$({0})'),
    'PB_MCase': (1, 1, 'MCASE$({0})'),
    'PB_Chr': (0, 1, chr_),
    'PB_Time': (0, 1, 'TIME$'),
    'PB_Date': (0, 1, 'DATE$'),
    'PB_FuncName': (0, 1, 'FUNCNAME$'),
    'PB_ReadStr': (0, 1, 'READ$({a0})'),
    'PB_MsgBox': (2, 0, 'MSGBOX({0}, {a0}, {1})'),
    'PB_InputBox': (3, 1, 'INPUTBOX$({0}, {1}, {2})'),
    'PB_ControlGetText': (0, 1, 'CONTROL_GET_TEXT$({a0})'),
    'PB_ControlSetText': (1, 0, 'CONTROL SET TEXT {a0}, {0}'),
    'PB_ControlAdd': (2, 0, control_add),
    'PB_DialogNew': (1, 0, 'DIALOG_NEW({a1}, {0}, {a2}, {a3}, {a4}, {a5}, {a6}, {a7})'),
    'PB_MenuAddItem': (1, 0, 'MENU ADD STRING, {a0}, {0}, {a1}, {a2}'),
    'PB_MenuSetText': (1, 0, 'MENU SET TEXT {a0}, {a1}, {0}'),
    'PB_Shell': (1, 0, 'SHELL({0}, {a0})'),
    'PB_ArraySort': (1, 0, 'ARRAY SORT {a0}'),
    'PB_XPrintStr': (1, 0, 'XPRINT {0};'),
    'PB_PrintStr': (1, 0, 'PRINT #fh, {0};'),
    'PB_WriteStr': (1, 0, 'WRITE #fh, {0}'),
    'PB_LineInput': (0, 1, 'LINE_INPUT$(#fh)'),
    'PB_InputItem': (0, 1, 'INPUT$(#fh)'),
    'PB_Open': (1, 0, lambda p, a: 'OPEN %s FOR %s AS #fh' % (p[0], OPEN_MODES.get(a[0], a[0]) if a else '?')),
    'PB_Kill': (1, 0, 'KILL {0}'),
}
# Value-only wrappers: rewrite in place everywhere (not string-stack ops).
VALUE_REWRITES = [
    (re.compile(r'\bPB_(?:Int|Uint|Short)ToFloat\(([^()]*)\)'), r'\1'),
    (re.compile(r'\bPB_IntToFloat\(([^()]*)\)'), r'\1'),
    (re.compile(r'\bPB_UIntToFloat\(([^()]*)\)'), r'\1'),
    (re.compile(r'\bPB_ShortToFloat\(([^()]*)\)'), r'\1'),
]
NOISE = re.compile(r'^\s*(PB_ErrCheck|PB_ErrCheckKeepFlags|PB_FpuReset|PB_FrameRelease)\(\);\s*$')
CALL = re.compile(r'\b(PB_\w+|pbrt_[0-9a-f]+|FUN_[0-9a-f]+|GOSUB_[0-9a-f]+)\(')
CONTROL = re.compile(r'^\s*(}|{|if\b|else\b|while\b|for\b|do\b|switch\b|case\b|default:|goto\b|return\b|LAB_\w+:|break;|continue;)')


def load_names():
    names = {}
    p = os.path.join(ROOT, 'tools', 'names.tsv')
    if os.path.exists(p):
        for line in open(p):
            if line.strip() and not line.startswith('#'):
                addr, name = line.split('\t')[:2]
                names[int(addr, 16)] = name.strip()
    return names


def scan_call(s, i):
    """s[i] is '(' of a call; return (args, end) where end is the index after ')'."""
    depth, args, start, j = 0, [], i + 1, i
    while j < len(s):
        c = s[j]
        if c == '"':
            j += 1
            while j < len(s) and s[j] != '"':
                j += 2 if s[j] == '\\' else 1
        elif c == "'":
            j += 1
            while j < len(s) and s[j] != "'":
                j += 2 if s[j] == '\\' else 1
        elif c == '(':
            depth += 1
        elif c == ')':
            depth -= 1
            if depth == 0:
                a = s[start:j].strip()
                if a or args:
                    args.append(a)
                return args, j + 1
        elif c == ',' and depth == 1:
            args.append(s[start:j].strip())
            start = j + 1
        j += 1
    return None, None


def calls(s):
    """All calls in s as (start, end, name, args), innermost/leftmost first by end index."""
    out = []
    for m in CALL.finditer(s):
        args, end = scan_call(s, m.end() - 1)
        if args is not None:
            out.append((m.start(), end, m.group(1), args))
    return sorted(out, key=lambda c: c[1])


def strip_cast(e):
    return re.sub(r'^\((?:char|undefined\d?|void|int|uint|short) \*+\)\s*', '', e.strip())


def unescape(lit):
    return lit.replace("\\'", "'")


def deref(e):
    """Turn a C address expression into an lvalue: &x -> x, else *e."""
    e = strip_cast(e)
    if e.startswith('&') and re.match(r'^&[\w.]+(\([^()]*(\([^()]*\)[^()]*)*\))?$', e):
        return e[1:]
    if re.match(r'^\w+\([^()]*\)$', e) and not e.startswith(('FUN_', 'PB_', 'pbrt_')):
        return e  # inlined array element
    return '*' + e


# ---------------------------------------------------------------------------------------------
# GOSUB frame accesses

def gosub_frame(body):
    def local(off):
        return 'local_%x' % (off + 4)

    def param(off):
        return 'param_%d' % ((off - 8) // 4 + 1)

    def name(sign, off):
        off = int(off, 16)
        return local(off) if sign == '-' else param(off) if off >= 8 else None

    def deref_sub(m):
        n = name(m.group(2), m.group(3))
        return n if n else m.group(0)

    def addr_sub(m):
        n = name(m.group(1), m.group(2))
        return '&' + n if n else m.group(0)
    body = re.sub(r'\*\(([\w ]+\*?) \*\)\(unaff_EBP \+ (-?)0x([0-9a-f]+)\)', lambda m: deref_sub(m) if True else '', body)
    body = re.sub(r'\([\w ]+\*+\)\(unaff_EBP \+ (-?)0x([0-9a-f]+)\)', addr_sub, body)
    body = re.sub(r'(?<![\w)])\(unaff_EBP \+ (-?)0x([0-9a-f]+)\)', addr_sub, body)
    body = re.sub(r'\bunaff_EBP \+ (-?)0x([0-9a-f]+)\b', addr_sub, body)
    return body


# ---------------------------------------------------------------------------------------------
# PB_ArrayElem temporaries -> arr(i)

ELEM_ASSIGN = re.compile(r'^(\s*)(\w+) = PB_ArrayElem\((.*)\);\s*$')


def elem_text(args):
    desc = strip_cast(args[0])
    desc = desc[1:] if desc.startswith('&') else desc
    idx = ', '.join(strip_cast(a) if a.startswith('(int)') is False else a for a in args[2:])
    return '%s(%s)' % (desc, idx)


def subst_elem(s, v, txt):
    e = re.escape(v)
    s = re.sub(r'\*\((?:[\w ]+?) \*+\)\s*%s\b' % e, txt, s)
    s = re.sub(r'\((?:[\w ]+?) \*+\)\s*\*%s\b' % e, txt, s)
    s = re.sub(r'\*%s\b' % e, txt, s)
    s = re.sub(r'(PB_StrPopAssign|PB_StrPopTo|PB_StrPushVar)\(\s*(?:\([\w *]+\))?\s*%s\)' % e,
               lambda mm: '%s(&%s)' % (mm.group(1), txt), s)
    return re.sub(r'\b%s\b' % e, '&' + txt, s)


def inline_elems(lines):
    """Replace `v = PB_ArrayElem(a,n,i); ... *v ...` by a(i) when safe."""
    depth_at = []
    d = 0
    for ln in lines:
        depth_at.append(d)
        d += ln.count('{') - ln.count('}')
    out = list(lines)
    n = len(out)
    for k in range(n):
        ln = out[k]
        m = ELEM_ASSIGN.match(ln) if ln is not None else None
        if not m:
            continue
        v = m.group(2)
        args, _ = scan_call(ln, ln.index('PB_ArrayElem(') + len('PB_ArrayElem'))
        if not args or len(args) < 3:
            continue
        txt = elem_text(args)
        vre = re.compile(r'\b%s\b' % re.escape(v))
        uses, ok, d0, rhs_use = [], True, depth_at[k], None
        for j in range(k + 1, n):
            lj = out[j]
            if lj is None:
                continue
            mj = ELEM_ASSIGN.match(lj)
            if mj and mj.group(2) == v:
                if vre.search(lj[mj.end(2):]):
                    rhs_use = j  # v = PB_ArrayElem(..., *v): substitute in the RHS only
                break
            if re.search(r'(?<![*\w])%s = ' % re.escape(v), lj):
                ok = False
                break
            if vre.search(lj):
                if depth_at[j] < d0 or (depth_at[j] == d0 and lj.lstrip().startswith('}')):
                    ok = False
                    break
                uses.append(j)
            elif depth_at[j] < d0:
                # left the block: any later use belongs to a different path
                rest = [x for x in range(j, n) if out[x] is not None and vre.search(out[x])]
                nxt = next((x for x in range(j, n) if out[x] is not None and ELEM_ASSIGN.match(out[x])
                            and ELEM_ASSIGN.match(out[x]).group(2) == v), n)
                if any(x < nxt for x in rest):
                    ok = False
                break
        if not ok or not (uses or rhs_use is not None):
            continue
        for j in uses:
            out[j] = subst_elem(out[j], v, txt)
        if rhs_use is not None:
            lj = out[rhs_use]
            cut = lj.index('PB_ArrayElem(')
            out[rhs_use] = lj[:cut] + subst_elem(lj[cut:], v, txt)
        out[k] = None
    return [x for x in out if x is not None]


# ---------------------------------------------------------------------------------------------
# String stack replay

class Fail(Exception):
    pass


def apply_op(name, args, stack, pending):
    """Apply one string-stack op; return None for a push, else the expression/statement text."""
    if name == 'PB_VarArgCount':
        pending.append(args[0] if args else '?')
        return None
    pops, pushes, tmpl = OPS[name]
    if len(stack) < pops:
        raise Fail('underflow')
    popped = stack[len(stack) - pops:] if pops else []
    del stack[len(stack) - pops:]
    fmt = {('a%d' % i): strip_cast(a) for i, a in enumerate(args)}
    fmt.update({('A%d' % i): deref(a) for i, a in enumerate(args)})
    if name == 'PB_StrPushLit':
        fmt['a0'] = unescape(args[0]) if args else '""'
    if name == 'PB_StrPushVar':
        fmt['a0'] = deref(args[0]) if args else '?'
        if fmt['a0'].startswith('*'):
            fmt['a0'] = strip_cast(args[0])
    try:
        if name == 'PB_Chr':
            text = chr_(popped, args, pending)
        elif callable(tmpl):
            text = tmpl(popped, [fmt['a%d' % k] for k in range(len(args))])
        else:
            text = tmpl.format(*popped, **fmt)
    except (IndexError, KeyError):
        text = '%s(%s)' % (name, ', '.join(popped + args))
    if pushes:
        stack.append(text)
        return None
    return text


def replay(lines, stats):
    out, buf, raw, stack, pending = [], [], [], [], []

    def flush_raw(why='end'):
        out.extend(raw)
        if buf or stack:
            stats['fallback'] += 1
            stats['why:' + why] += 1
        buf.clear(), raw.clear(), stack.clear()

    for ln in lines:
        cs = [c for c in calls(ln) if c[2] in OPS or c[2] == 'PB_VarArgCount' or (c[2].startswith('pbrt_') and stack)]
        if not cs:
            if stack:
                if CONTROL.match(ln):
                    flush_raw('control')
                    out.append(ln)
                else:
                    buf.append(ln)
                    raw.append(ln)
            else:
                out.append(ln)
            continue
        if any(c[2].startswith('pbrt_') for c in cs):
            flush_raw('pbrt:' + next(c[2] for c in cs if c[2].startswith('pbrt_')))
            out.append(ln)
            continue
        indent = re.match(r'^\s*', ln).group(0)
        bare = len(cs) == 1 and re.match(r'^\s*%s\(.*\);\s*$' % re.escape(cs[0][2]), ln) and ln.count(cs[0][2] + '(') == 1
        try:
            snap = list(stack)
            new, shift = ln, 0
            for start, end, name, args in cs:
                text = apply_op(name, args, stack, pending)
                if text is None:  # push
                    if not bare:
                        new = new[:start + shift] + new[end + shift:]
                        shift -= end - start
                    continue
                stats['folded'] += 1
                if bare:
                    new = indent + text + (';' if text else '')
                else:
                    new = new[:start + shift] + text + new[end + shift:]
                    shift += len(text) - (end - start)
        except Fail:
            stack[:] = snap
            flush_raw('underflow')
            out.append(ln)
            continue
        raw.append(ln)
        if bare and cs[0][2] == 'PB_VarArgCount':
            continue
        if not stack:
            out.extend(buf)
            if new.strip():
                out.append(new)
            buf.clear(), raw.clear()
        elif not (bare and (cs[0][2] == 'PB_VarArgCount' or OPS[cs[0][2]][1])):
            if new.strip() and new.strip() != ';':
                buf.append(new)
    flush_raw()
    return out


# ---------------------------------------------------------------------------------------------

def tidy(lines):
    body = '\n'.join(lines)
    return [ln for ln in body.split('\n')]


def drop_unused_decls(lines):
    out = []
    text = '\n'.join(lines)
    in_decl = False
    for k, ln in enumerate(lines):
        if ln == '{':
            in_decl = True
            out.append(ln)
            continue
        if in_decl:
            m = re.match(r'^  [\w ]+?\**(\w+)(\[[^\]]*\])?;$', ln)
            if m:
                if len(re.findall(r'\b%s\b' % m.group(1), text)) <= 1:
                    continue
            elif ln.strip() == '':
                in_decl = False
        out.append(ln)
    return out


# ---------------------------------------------------------------------------------------------
# Polish: non-string helpers, file numbers, DIM, constants

DIM_TYPES = {0x08: 'INTEGER', 0x0a: 'LONG', 0x10: 'QUAD', 0x14: 'SINGLE', 0x16: 'DOUBLE', 0x18: 'EXT',
             0x1a: 'CUR', 0x2e: 'STRING', 0x30: 'STRING', 0x22: 'VARIANT', 0x24: 'OBJECT'}
FUNCS = {'PB_Rnd': 'RND', 'PB_RndInt': 'RND', 'PB_Int': 'INT', 'PB_Fix': 'FIX', 'PB_Ceil': 'CEIL',
         'PB_Frac': 'FRAC', 'PB_Sqr': 'SQR', 'PB_Timer': 'TIMER', 'PB_Err': 'ERR', 'PB_Eof': 'EOF',
         'PB_FreeFile': 'FREEFILE', 'PB_Rgb': 'RGB', 'PB_Pow': 'POW', 'PB_GetDlgItem': 'GetDlgItem',
         'PB_SendMessage': 'SendMessage', 'PB_UBound': 'UBOUND', 'PB_DialogGetSize': 'DIALOG_GET_SIZE',
         'PB_DialogPixelsToUnits': 'DIALOG_PIXELS_TO_UNITS', 'PB_XPrintGetPpi': 'XPRINT_GET_PPI',
         'PB_XPrintGetMargin': 'XPRINT_GET_MARGIN', 'PB_XPrintGetClient': 'XPRINT_GET_CLIENT'}
STMTS = {
    'PB_DialogEnd': 'DIALOG END {0}, {1}',
    'PB_DialogShowModal': 'DIALOG SHOW MODAL {0} CALL {1}',
    'PB_DialogShowState': 'DIALOG SHOW STATE {0}, {1}',
    'PB_ControlSetCheck': 'CONTROL SET CHECK {0}, {1}, {2}',
    'PB_ControlSetOption': 'CONTROL SET OPTION {0}, {1}, {2}, {3}',
    'PB_ControlSetFocus': 'CONTROL SET FOCUS {0}, {1}',
    'PB_ControlKill': 'CONTROL KILL {0}, {1}',
    'PB_Sleep': 'SLEEP {0}',
    'PB_Randomize': 'RANDOMIZE {0}',
    'PB_Error': 'ERROR {0}',
    'PB_Resume': 'RESUME {0}',
    'PB_XPrintEnd': 'XPRINT',
    'PB_XPrintFormFeed': 'XPRINT FORMFEED',
    'PB_ComboBoxReset': 'COMBOBOX RESET {0}, {1}',
    'PB_ComboBoxSelect': 'COMBOBOX SELECT {0}, {1}, {2}',
    'PB_ListBoxReset': 'LISTBOX RESET {0}, {1}',
    'PB_SwapMem': 'SWAP {0}, {1}',
    'PB_ErrClear': 'ERRCLEAR',
    'PB_Beep': 'BEEP',
    'PB_DialogDoEvents': 'DIALOG DOEVENTS {0}',
    'PB_MenuAttach': 'MENU ATTACH {0}, {1}',
    'PB_DialogShowModeless': 'DIALOG SHOW MODELESS {0} CALL {1}',
    'PB_DialogSetSize': 'DIALOG SET SIZE {0}, {2}, {3}',
    'PB_XPrintCancel': 'XPRINT CANCEL',
}
DROP = re.compile(r'^\s*(PB_ChkRef\(.*\)|PB_StrFree\(.*\)|pcStack_78 = \w+|PB_FpuSave\(.*\)|PB_FpuRestore\(.*\)'
                  r'|\w+ = (?:\([\w *]+\))?\(?shim_\w+ \+ \d+\)?);\s*$')
FH_SET = re.compile(r'^\s*local_3a = (?:\(undefined2\))?(.+);\s*$')


def replace_calls(line, names, fn):
    """Replace calls to `names` (innermost first) by fn(name, args) unless it returns None."""
    for _ in range(20):
        cs = [c for c in calls_any(line) if c[2] in names]
        done = False
        for start, end, name, args in cs:
            r = fn(name, args)
            if r is not None:
                line = line[:start] + r + line[end:]
                done = True
                break
        if not done:
            return line
    return line


def calls_any(s):
    out = []
    for m in re.finditer(r'\b(PB_\w+)\(', s):
        args, end = scan_call(s, m.end() - 1)
        if args is not None:
            out.append((m.start(), end, m.group(1), args))
    return sorted(out, key=lambda c: c[1])


def dim_text(args):
    if len(args) < 6:
        return None
    tw = num(args[2])
    if tw is None:
        return None
    nd, ty = tw >> 24, (tw >> 16) & 0xff
    b = [strip_cast(a) for a in args[4:]]
    if len(b) != 2 * nd:
        return None
    dims = ['%s TO %s' % (b[2 * (nd - 1 - k) + 1], b[2 * (nd - 1 - k)]) for k in range(nd)]
    d = strip_cast(args[0])
    return 'DIM %s(%s) AS %s' % (d[1:] if d.startswith('&') else d, ', '.join(dims), DIM_TYPES.get(ty, hex(ty)))


def polish_call(name, a):
    a = [strip_cast(x) for x in a]
    if name == 'PB_ArrayElem' and len(a) >= 3:
        d = a[0][1:] if a[0].startswith('&') else a[0]
        return '&%s(%s)' % (d, ', '.join(a[2:]))
    if name in ('PB_CurLoad', 'PB_CuxLoad') and a:
        return deref(a[0])
    if name in ('PB_CurStore', 'PB_CuxStore') and len(a) == 2:
        return '%s = %s' % (deref(a[1]), a[0])
    if name == 'PB_StrCmpVar' and len(a) == 2:
        return '(%s == %s)' % (a[0], a[1])
    if name == 'PB_IDiv' and len(a) == 2:
        return '(%s \\ %s)' % (a[0], a[1])
    if name == 'PB_And' and len(a) == 2:
        return '(%s AND %s)' % (a[0], a[1])
    if name == 'PB_XPrintSetPos' and len(a) >= 2:
        return 'XPRINT SET POS (%s, %s)' % (a[0], a[1])
    if name == 'PB_FMod' and len(a) == 2:
        return '(%s MOD %s)' % (a[0], a[1])
    if name in ('PB_IntToFloat', 'PB_UIntToFloat', 'PB_ShortToFloat') and a:
        return a[0]
    if name == 'PB_DimArray':
        return dim_text(a)
    if name == 'PB_ArrayErase' and len(a) == 2:
        return '%s %s()' % ('ERASE' if a[1] == '2' else 'RESET', a[0][1:] if a[0].startswith('&') else a[0])
    if name == 'PB_ControlEnable' and len(a) == 3:
        return 'CONTROL %s %s, %s' % ('DISABLE' if a[2] == '0' else 'ENABLE', a[0], a[1])
    if name == 'PB_MenuNew' and a:
        return 'MENU_NEW(%s)' % ('POPUP' if a[0] == '1' else 'BAR')
    if name == 'PB_Close' and a:
        return 'CLOSE #%s' % a[0]
    if name == 'PB_OnErrorSet' and a:
        return 'ON ERROR GOTO %s' % (a[0][1:] if a[0].startswith('&') else a[0])
    if name in STMTS:
        try:
            return STMTS[name].format(*a)
        except IndexError:
            return None
    if name in FUNCS:
        return '%s(%s)' % (FUNCS[name], ', '.join(a)) if a else FUNCS[name]
    return None


POLISH_NAMES = set(FUNCS) | set(STMTS) | {'PB_ArrayElem', 'PB_StrCmpVar', 'PB_IDiv', 'PB_And', 'PB_XPrintSetPos', 'PB_CurLoad', 'PB_CuxLoad', 'PB_CurStore', 'PB_CuxStore',
                                          'PB_FMod', 'PB_IntToFloat', 'PB_UIntToFloat', 'PB_ShortToFloat', 'PB_DimArray',
                                          'PB_ArrayErase', 'PB_ControlEnable', 'PB_MenuNew', 'PB_Close', 'PB_OnErrorSet'}
HEX = re.compile(r'(?<![\w.])0x([0-9a-f]{1,6})\b')
STRLIT = re.compile(r'"(?:[^"\\]|\\.)*"')


def decimal(line):
    """Hex constants -> decimal (outside string literals)."""
    parts = STRLIT.split(line)
    lits = STRLIT.findall(line)
    parts = [HEX.sub(lambda m: str(int(m.group(1), 16)), p) for p in parts]
    out = parts[0]
    for lit, p in zip(lits, parts[1:]):
        out += lit + p
    return out


COMMA_ELEM = re.compile(r'(\w+) = (&\w+\((?:[^()]|\([^()]*\))*\)), ')


def inline_comma_elems(lines):
    """`(p = &a(i), *p < 10)` -> `(a(i) < 10)` when p is not read again before being reassigned."""
    for k, ln in enumerate(lines):
        for _ in range(10):
            m = COMMA_ELEM.search(ln)
            if not m:
                break
            v, elem = m.group(1), m.group(2)
            rest = ln[m.end():]
            nxt = re.search(r'\b%s = ' % re.escape(v), rest)
            tail = rest[nxt.start():] if nxt else ''
            head = rest[:nxt.start()] if nxt else rest
            later_read = False
            if not nxt:
                for ln2 in lines[k + 1:]:
                    if re.search(r'\b%s = ' % re.escape(v), ln2) and not re.search(r'[^=!<>]=\s*[^=]*\b%s\b' % re.escape(v), ln2.split(v + ' = ')[0]):
                        break
                    if re.search(r'\b%s\b' % re.escape(v), ln2):
                        later_read = True
                        break
            if later_read or re.search(r'\b%s\b' % re.escape(v), re.sub(r'\*\s*%s\b' % re.escape(v), '', head)):
                break
            head = re.sub(r'\*\s*%s\b' % re.escape(v), elem[1:], head)
            ln = ln[:m.start()] + head + tail
        lines[k] = ln
    return lines


def polish(lines):
    out, fh, items, kind = [], 'fh', [], None

    def close_items():
        nonlocal items, kind
        if items:
            ind = re.match(r'^\s*', items[0][0]).group(0)
            out.append('%s%s #%s, %s;' % (ind, kind, fh, ', '.join(i[1].rstrip(';').strip() for i in items)))
            items, kind = [], None

    for ln in lines:
        if DROP.match(ln):
            continue
        m = FH_SET.match(ln)
        if m:
            fh = re.sub(r'^\((?:short|int|undefined\d?)\)', '', strip_cast(m.group(1)))
            continue
        ln = replace_calls(ln, POLISH_NAMES, polish_call)
        ln = re.sub(r'\*&(\w+\b)(?!\s*\[)', r'\1', ln)
        ln = re.sub(r'\*\([\w ]+ \*\)&(\w+\()', r'\1', ln)
        ln = ln.replace('(float10)', '').replace('PB_ErrCheckKeepFlags(), ', '').replace('PB_ErrCheck(), ', '')
        ln = re.sub(r'\((\w+) = ([^(),]+(?:\([^()]*\))?[^(),]*), \1\)', r'(\2)', ln)
        ln = decimal(ln)
        st = ln.strip()
        w = re.match(r'^WRITE #fh, (.*?);?$', st) or re.match(r'^PB_WriteNum\((.*)\);$', st)
        pr = re.match(r'^PRINT #fh, (.*?);;?$', st)
        if w or pr:
            if kind and kind != ('WRITE' if w else 'PRINT'):
                close_items()
            kind = 'WRITE' if w else 'PRINT'
            items.append((ln, (w or pr).group(1)))
            continue
        if st in ('PB_WriteSep();',):
            continue
        if st == 'PB_PrintEnd();':
            if items:
                close_items()
            else:
                out.append(ln.replace('PB_PrintEnd();', 'PRINT #%s,' % fh))
            continue
        close_items()
        ln = ln.replace('#fh', '#' + fh).replace('PB_InputNum()', 'INPUT_NUM(#%s)' % fh)
        out.append(ln)
    close_items()
    return out


TEMP_ASSIGN = re.compile(r'^(\s*)([a-z]+Var\d+(?:_\d+)?) = (.+);\s*$')
LOOP_OPEN = re.compile(r'^\s*(while|for|do)\b|^\s*}\s*while\b')


def fold_temps(lines):
    """`t = E; X(t);` -> `X(E);` for Ghidra temporaries used once on the very next line, when the value
    cannot be read anywhere else (no later read before reassignment, not loop-carried, no gotos)."""
    labels = {m.group(1): i for i, ln in enumerate(lines) for m in [re.match(r'^(LAB_\w+):', ln)] if m}
    spans = [(labels[g], i) for i, ln in enumerate(lines) for g in re.findall(r'goto (LAB_\w+);', ln)
             if g in labels and labels[g] < i]  # backward gotos form loops
    out = list(lines)
    k = 0
    while k < len(out) - 1:
        if any(a <= k <= b for a, b in spans):
            k += 1
            continue
        m = TEMP_ASSIGN.match(out[k] or '')
        nxt = out[k + 1]
        if not m or nxt is None or CONTROL.match(nxt) and not nxt.lstrip().startswith(('if', 'else if', 'while', 'return')):
            k += 1
            continue
        v, e = m.group(2), m.group(3)
        vre = re.compile(r'\b%s\b' % re.escape(v))
        if len(vre.findall(nxt)) != 1 or re.search(r'(?<![*\w])%s = ' % re.escape(v), nxt) or '/*' in nxt \
                or vre.search(e) or nxt.lstrip().startswith('while'):
            k += 1
            continue
        # forward scan: next event for v must be a reassignment (or nothing), and no enclosing loop may close
        ok, depth, stack = True, 0, []
        for ln in out[k + 2:]:
            if ln is None:
                continue
            if re.match(r'^\s*%s = ' % re.escape(v), ln) and not vre.search(ln.split('=', 1)[1]) and not stack:
                break  # unconditional reassignment (not inside a block opened after the temp)
            if vre.search(ln):
                ok = False
                break
            opens, closes = ln.count('{'), ln.count('}')
            for _ in range(closes):
                if stack:
                    stack.pop()
                else:
                    depth -= 1
            if depth < 0 and LOOP_OPEN.search(ln) and ln.lstrip().startswith('}'):
                ok = False  # do { ... } while (...) enclosing the assignment
                break
            for _ in range(opens):
                stack.append(ln)
        if ok:
            # an enclosing while/for/do loop? look backwards for unmatched loop openers
            d = 0
            for ln in reversed(out[:k]):
                if ln is None:
                    continue
                d += ln.count('}') - ln.count('{')
                if d < 0:
                    if LOOP_OPEN.search(ln):
                        # the value could flow around the loop only if v is read before k inside it
                        start = out.index(ln) if ln in out else 0
                        if any(x is not None and vre.search(x) and not re.match(r'^\s*%s = ' % re.escape(v), x)
                               for x in out[start:k]):
                            ok = False
                        break
                    d = 0
        if not ok:
            k += 1
            continue
        wrapped = e.startswith('(') and e.endswith(')') and paren_balance(e[1:-1]) == 0 and '(' not in e[1:-1].split(')')[0][:0]
        if wrapped and '(%s)' % v in nxt:
            out[k + 1] = nxt.replace('(%s)' % v, e, 1)
        else:
            rep = e if wrapped or re.match(r'^[\w.$]+(\(.*\))?$', e) or e.startswith('"') else '(%s)' % e
            out[k + 1] = vre.sub(lambda _: rep, nxt, count=1)
        out[k] = None
        k += 1
    return [x for x in out if x is not None]


# ---------------------------------------------------------------------------------------------
# Structure: FOR loops and SELECT CASE

INC = re.compile(r'^\s*(\w+(?:\[0\])?) = \1 \+ (-?\d+);\s*$')


def block_end(lines, k):
    """Index of the line closing the block opened at the end of lines[k]."""
    d = 0
    for j in range(k, len(lines)):
        ln = lines[j]
        if '"' in ln:
            ln = STRLIT.sub('""', ln)
        d += ln.count('{') - ln.count('}')
        if d <= 0 and j > k:
            return j
        if d == 0 and j == k:
            return None
    return None


def const(e):
    try:
        return int(e)
    except ValueError:
        return None


def for_loops(lines):
    out = list(lines)
    for k, ln in enumerate(lines):
        mw = re.match(r'^(\s*)while \((\w+(?:\[0\])?) (<=|<|>=|>) (.+)\) \{$', ln)
        md = re.match(r'^(\s*)do \{$', ln)
        if not (mw or md):
            continue
        end = block_end(out, k)
        if end is None:
            continue
        ind = (mw or md).group(1)
        if md:
            mc = re.match(r'^\s*\} while \((\w+(?:\[0\])?) (<=|<|>=|>) (-?\d+)\);$', out[end])
            if not mc:
                continue
            v, op, lim = mc.group(1), mc.group(2), mc.group(3)
        else:
            if out[end].strip() != '}':
                continue
            v, op, lim = mw.group(2), mw.group(3), mw.group(4)
        # last body statement must be the increment
        last = end - 1
        while last > k and (out[last] is None or not out[last].strip()):
            last -= 1
        mi = INC.match(out[last] or '')
        if not mi or mi.group(1) != v:
            continue
        step = int(mi.group(2))
        if (step > 0) != (op in ('<', '<=')):
            continue
        ev = re.escape(v)
        body = [x for x in out[k + 1:last] if x is not None]
        if any(re.search(r'(?<![*\w.])%s = ' % ev, x) for x in body) or any('continue;' in x for x in body):
            continue
        # limit must not change inside the loop
        if any(re.search(r'(?<![*\w.])%s = ' % re.escape(w), x) for w in re.findall(r'[A-Za-z_]\w*', lim) for x in body):
            continue
        # find the initialisation V = A among the few lines before
        init = None
        for j in range(k - 1, max(-1, k - 6), -1):
            if out[j] is None:
                continue
            m0 = re.match(r'^\s*%s = (.+);$' % ev, out[j])
            if m0:
                init = j
                break
            if re.search(r'\b%s\b' % ev, out[j]) or CONTROL.match(out[j]) or out[j].rstrip().endswith('{'):
                break
        if init is None:
            continue
        a = re.match(r'^\s*%s = (.+);$' % ev, out[init]).group(1)
        if md:
            ca, cl = const(a), const(lim)
            if ca is None or cl is None:
                continue
            to = cl - 1 if op == '<' else cl + 1 if op == '>' else cl
            if (step > 0 and ca > to) or (step < 0 and ca < to):
                continue
            to = str(to)
        else:
            to = lim if op in ('<=', '>=') else ('%s - 1' % lim if op == '<' else '%s + 1' % lim)
            to = str(const(to.split(' ')[0]) - 1) if op == '<' and const(lim) is not None else (
                str(const(lim) + 1) if op == '>' and const(lim) is not None else to)
        st = '' if step == 1 else ' STEP %d' % step
        # PB evaluates the limit once into a temporary: fold `T = E; ... FOR v = a TO T` when T has no other use
        if re.match(r'^[A-Za-z_]\w*$', to):
            for j in range(init - 1, max(-1, init - 3), -1):
                mt = out[j] and re.match(r'^\s*%s = (.+);$' % re.escape(to), out[j])
                if mt:
                    uses = sum(len(re.findall(r'\b%s\b' % re.escape(to), x)) for x in out if x is not None)
                    decl = sum(1 for x in out if x is not None and re.match(r'^  [\w ]+\**%s;$' % re.escape(to), x))
                    if uses - decl == 2 and not re.search(r'\b%s\b' % ev, mt.group(1)):
                        to = mt.group(1)
                        out[j] = None
                    break
                if out[j] is not None:
                    break
        out[init] = None
        out[k] = '%sFOR %s = %s TO %s%s {' % (ind, v, a, to, st)
        out[last] = None
        out[end] = '%s} NEXT %s' % (ind, v)
    return [x for x in out if x is not None]


def case_values(cond, x=None):
    """`(a == 1) || ((a == 2 || a == 3))` -> ('a', ['1', '2', '3']) or None."""
    parts = [p.strip().strip('()').strip() for p in re.split(r'\|\|', cond.replace('(', ' ').replace(')', ' '))]
    vals, var = [], x
    for p in parts:
        m = re.match(r'^(\w+(?:\[0\])?) == (-?\d+|"[^"]*")$', p)
        if not m or (var is not None and m.group(1) != var):
            return None
        var = m.group(1)
        vals.append(m.group(2))
    return (var, vals) if vals else None


def select_case(lines):
    """if/else-if chains comparing one variable with constants (>= 3 arms) -> SELECT CASE."""
    out = list(lines)
    k = 0
    while k < len(out):
        m = re.match(r'^(\s*)if \((.*)\) \{$', out[k] or '')
        cv = case_values(m.group(2)) if m and '"' not in m.group(2) else None
        if not cv:
            k += 1
            continue
        ind, x = m.group(1), cv[0]
        arms, j, elses, last, ok = [(k, cv[1])], k, None, None, True
        while True:
            e = block_end(out, j)
            if e is None or out[e].strip() != '}' or e + 1 >= len(out):
                ok = e is not None and out[e].strip() == '}'
                last = e
                break
            nx = out[e + 1]
            mn = re.match(r'^\s*else if \((.*)\) \{$', nx)
            cn = case_values(mn.group(1), x) if mn else None
            if cn:
                arms.append((e, cn[1]))
                j = e + 1
                continue
            if nx.strip() == 'else {':
                e2 = block_end(out, e + 1)
                if e2 is None:
                    ok = False
                    break
                elses = e
                last = e2
                break
            last = e
            break
        if not ok or len(arms) < 3:
            k += 1
            continue
        out[k] = '%sSELECT CASE %s {\n%sCASE %s:' % (ind, x, ind, ', '.join(arms[0][1]))
        for a, vals in arms[1:]:
            out[a] = '\x00'
            out[a + 1] = '%sCASE %s:' % (ind, ', '.join(vals))
        if elses is not None:
            out[elses] = '\x00'
            out[elses + 1] = '%sCASE ELSE:' % ind
        out[last] = '%s} END SELECT' % ind
        k += 1
    return '\n'.join(x for x in out if x != '\x00').split('\n')


def paren_balance(s):
    d, j = 0, 0
    while j < len(s):
        c = s[j]
        if c in '"\'':
            q, j = c, j + 1
            while j < len(s) and s[j] != q:
                j += 2 if s[j] == '\\' else 1
        elif c == '(':
            d += 1
        elif c == ')':
            d -= 1
        j += 1
    return d


def join_continuations(lines):
    """Ghidra wraps long statements; join lines until parentheses balance (comments excluded)."""
    out, cur = [], None
    for ln in lines:
        if cur is not None:
            cur += ' ' + ln.strip() if not cur.endswith('(') else ln.strip()
            if paren_balance(cur) <= 0 and not cur.endswith(' ='):
                out.append(cur)
                cur = None
            continue
        if not ln.lstrip().startswith(('/*', '//')) and (paren_balance(ln) > 0 or ln.endswith(' =')):
            cur = ln
        else:
            out.append(ln)
    if cur is not None:
        out.append(cur)
    return out


def load_locals():
    loc = {}
    p = os.path.join(ROOT, 'tools', 'locals.tsv')
    if os.path.exists(p):
        for line in open(p):
            f = line.rstrip('\n').split('\t')
            if len(f) >= 3 and not line.startswith('#'):
                loc.setdefault(int(f[0], 16), {})[f[1]] = f[2]
    return loc


LOCALS = {}
FUNC_ADDR = {}


def apply_locals(text, chain):
    """chain: frame owner addresses from the root SUB down to this routine (GOSUBs share the root's frame)."""
    m = {}
    for a in chain:
        m.update(LOCALS.get(a, {}))
    if not m:
        return text
    return re.sub(r'\*?\b(local_[0-9a-f]+|param_\d+)\b(\[0\])?',
                  lambda x: m.get(x.group(1), x.group(0)) if x.group(1) in m else x.group(0), text)


def process(func, names, stats, callers=None):
    head, _, body = func.partition('\n{\n')
    if not _:
        return func
    if re.match(r'// ==== \w+ (pbrt_|PB_)', head):
        return func  # PB runtime: leave as decompiled
    is_gosub = re.search(r'GOSUB_[0-9a-f]+ \(size', head) is not None
    if is_gosub:
        body = gosub_frame(body)
        g = re.search(r'(GOSUB_[0-9a-f]+)', head).group(1)
        head = head.replace('/* PB GOSUB target', '/* GOSUB from %s; its locals and params are the caller\'s.\n   PB GOSUB target' %
                            ', '.join(sorted(callers.get(g, ['?']))) if callers is not None else '/* PB GOSUB target', 1)
    lines = [ln for ln in join_continuations(body.split('\n')) if not NOISE.match(ln)]
    lines = inline_elems(lines)
    lines = replay(lines, stats)
    lines = polish(lines)
    lines = inline_comma_elems(lines)
    lines = fold_temps(lines)
    lines = for_loops(lines)
    lines = select_case(lines)
    lines = drop_unused_decls(['{'] + lines)[1:]
    text = head + '\n{\n' + '\n'.join(lines)
    own = int(re.match(r'// ==== (\w+)', head).group(1), 16)
    chain = [own]
    if is_gosub and callers:
        cur = g
        for _ in range(8):  # walk GOSUB -> caller up to the owning SUB/FUNCTION
            cs = sorted(callers.get(cur, []))
            if not cs or FUNC_ADDR.get(cs[0]) in chain:
                break
            chain.insert(0, FUNC_ADDR.get(cs[0], own))
            if not cs[0].startswith('GOSUB_'):
                break
            cur = cs[0]
    text = apply_locals(text, chain)
    if names:
        text = re.sub(r'\b(_?DAT_00([0-9a-f]{6})|FUN_00([0-9a-f]{6})|GOSUB_([0-9a-f]{6}))\b',
                      lambda m: names.get(int(m.group(2) or m.group(3) or m.group(4), 16), m.group(1)), text)
    return text


def main():
    argv = [a for a in sys.argv[1:] if not a.startswith('--')]
    src = argv[0] if len(argv) > 0 else os.path.join(ROOT, 'decompiled', 'c')
    dst = argv[1] if len(argv) > 1 else os.path.join(ROOT, 'decompiled', 'bas')
    os.makedirs(dst, exist_ok=True)
    names = {} if '--no-names' in sys.argv else load_names()
    if '--no-names' not in sys.argv:
        LOCALS.update(load_locals())
    stats = Counter()
    callers = {}
    for fn in sorted(os.listdir(src)):
        if fn.endswith('.c'):
            for part in re.split(r'(?m)^(?=// ==== )', open(os.path.join(src, fn)).read()):
                m = re.match(r'// ==== (\w+) (\w+)', part)
                if m:
                    FUNC_ADDR[m.group(2)] = int(m.group(1), 16)
                    m = re.match(r'// ==== \w+ (\w+)', part)
                    for g in set(re.findall(r'\b(GOSUB_[0-9a-f]+)\(', part)):
                        if g != m.group(1):
                            callers.setdefault(g, set()).add(m.group(1))
    for fn in sorted(os.listdir(src)):
        if not fn.endswith('.c'):
            continue
        text = open(os.path.join(src, fn)).read()
        parts = re.split(r'(?m)^(?=// ==== )', text)
        out = [process(p, names, stats, callers) if p.startswith('// ==== ') else p for p in parts]
        open(os.path.join(dst, fn), 'w').write(''.join(out))
    left = Counter()
    for fn in os.listdir(dst):
        for m in re.finditer(r'\b(PB_Str\w+)\(', open(os.path.join(dst, fn)).read()):
            left[m.group(1)] += 1
    print('folded %d string ops, %d fallbacks; left: %s' % (stats['folded'], stats['fallback'],
          ', '.join('%s %d' % kv for kv in left.most_common(8))))
    if os.environ.get('PBBASIC_DEBUG'):
        print(sorted(((v, k) for k, v in stats.items() if k.startswith('why:')), reverse=True)[:25])


if __name__ == '__main__':
    main()
