//! PowerBASIC 8 string functions, reimplemented from the WSR.EXE runtime.
//!
//! Every function mirrors one runtime helper (address given in its doc comment, names as in
//! `tools/ghidra/PBAnnotate.java`). Strings are byte strings (`&[u8]` in, [`PbStr`] out); positions are
//! 1-based like in BASIC. The edge cases (zero/negative/out-of-range positions and counts, empty
//! arguments) follow the original register arithmetic exactly; they are pinned down by
//! `tests/string.rs` against vectors produced by running the original code under emulation
//! (`tools/emu/pbstring.py`).
//!
//! Conventions shared by most routines:
//! - A negative position or count is usually taken relative to the length (`len + x`, sometimes
//!   `len + x + 1`), using 32-bit wrapping arithmetic, and then compared *unsigned*: a value that is
//!   still negative behaves like "huge". The individual doc comments spell out each case.
//! - Substring search ([`find`]) is the plain left-to-right first occurrence (`0x6bd823`); repeated
//!   searches (PARSE$, REPLACE, REMOVE$, PARSECOUNT) continue after the end of the previous match, so
//!   matches never overlap.
//! - Runtime errors: allocating a negative length raises error 7 (SPACE$, STRING$ with a negative
//!   count); REPLACE ANY with sets of different length raises error 5. In both cases the original pushes
//!   `""`, which is what these functions return; the error flag itself is not modelled.
//! - The original distinguishes a null `""` from a non-null zero-length string in a few places
//!   (`PARSECOUNT ANY`, `PARSE$ ANY`, `REPLACE`, ...: they test the pointer, not the length). The
//!   runtime never creates non-null empty strings (the allocator returns null for length 0), so all
//!   `""` here are the null kind.

use crate::pb::pbstr::PbStr;
use std::cmp::Ordering;

/// Which case mapping UCASE$/LCASE$/MCASE$ use (EAX of the helper).
///
/// The game always passes 0 ([`CaseMode::Ansi`]): every UCASE$/LCASE$/MCASE$ call site in WSR.EXE
/// has `xor eax, eax`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseMode {
    /// EAX = 0: Windows `CharUpperBuffA`/`CharLowerBuffA` (and `GetStringTypeA` C1_ALPHA for MCASE$)
    /// on the ANSI code page. Modelled for code page 1252, see [`upper_ansi`].
    Ansi,
    /// EAX != 0: the runtime's own table, ASCII plus the code page 437 letters ÄÖÜÉÑÅÇ / äöüéñåç.
    Oem,
}

/// First occurrence of `needle` (non-empty) in `hay`; `0x6bd823`.
fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Number of non-overlapping occurrences of `d` (non-empty), scanning left to right; `0x6bd7fe`.
fn count_occ(s: &[u8], d: &[u8]) -> i32 {
    let mut n = 0i32;
    let mut pos = 0;
    while let Some(m) = find(&s[pos..], d) {
        n += 1;
        pos += m + d.len();
    }
    n
}

/// Number of bytes of `s` contained in `set`; `0x6bd813`.
fn count_any(s: &[u8], set: &[u8]) -> i32 {
    s.iter().filter(|b| set.contains(b)).count() as i32
}

/// Adjust a negative PARSE$ field number: `n + separators + 2` (so -1 is the last field), wrapping.
fn field_index(n: i32, seps: i32) -> i32 {
    if n < 0 {
        n.wrapping_add(seps).wrapping_add(2)
    } else {
        n
    }
}

/// `MID$(s, start, count)`; `PB_Mid` 0x6ba1c9 (start in ECX, count in EAX).
///
/// - `start == 0`: `""`. `start < 0`: counted from the end, `start = len + start + 1`; if `len + start`
///   is negative: `""`. A start beyond the end gives `""`.
/// - `count < 0`: `count = len + count`, then compared unsigned, so e.g. `MID$("abc", 1, -1)` = `"ab"`,
///   `MID$("abc", 1, -3)` = `""` and `MID$("abc", 1, -4)` = `"abc"` (wraps to huge).
pub fn mid(s: &[u8], start: i32, count: i32) -> PbStr {
    let len = s.len() as i32;
    let count = if count < 0 { count.wrapping_add(len) } else { count } as u32;
    if start == 0 {
        return PbStr::new();
    }
    let mut st = start;
    if st < 0 {
        st = st.wrapping_add(len);
        if st < 0 {
            return PbStr::new();
        }
        st += 1;
    }
    let st = (st - 1) as u32;
    let avail = (len as u32).saturating_sub(st);
    let n = avail.min(count) as usize;
    if n == 0 {
        return PbStr::new();
    }
    PbStr(s[st as usize..st as usize + n].to_vec())
}

/// `LEFT$(s, n)`. The compiler emits no helper of its own for it: LEFT$ is compiled as
/// `PB_Mid` with start 1 (call sites `mov ecx, 1; mov eax, n; call 0x6ba1c9`), so this is
/// `MID$(s, 1, n)`, including the negative-count behaviour of [`mid`] (`LEFT$("abc", -1)` = `"ab"`).
pub fn left(s: &[u8], n: i32) -> PbStr {
    mid(s, 1, n)
}

/// `RIGHT$(s, n)`; `PB_Right` 0x6ba227 (n in EAX).
///
/// `n < 0` becomes `len + n`; then if `n >= len` (unsigned, so a still negative value counts as huge)
/// the whole string is returned, else the last `n` bytes. `RIGHT$("abc", -1)` = `"bc"`,
/// `RIGHT$("abc", -3)` = `""`, `RIGHT$("abc", -4)` = `"abc"`.
pub fn right(s: &[u8], n: i32) -> PbStr {
    let len = s.len() as i32;
    let n = if n < 0 { n.wrapping_add(len) } else { n } as u32;
    if n >= len as u32 {
        return PbStr(s.to_vec());
    }
    PbStr(s[s.len() - n as usize..].to_vec())
}

/// `MID$(target, start, count) = repl`; `PB_MidAssign` 0x6ba15a (EBX/EDX target pointer/length, ECX
/// start, EAX count). Overwrites in place, never changes the length of `target`.
///
/// - `start == 0`: no-op; `start < 0`: `len + start + 1` (no-op if `len + start < 0`); a start beyond
///   `len + 1` is a no-op.
/// - `count < 0`: `len + count` (no-op if that is negative).
/// - Copies `min(len(repl), len - start + 1, count)` bytes.
pub fn mid_assign(target: &mut PbStr, start: i32, count: i32, repl: &[u8]) {
    let len = target.0.len() as i32;
    if start == 0 {
        return;
    }
    let mut st = start;
    if st < 0 {
        st = st.wrapping_add(len);
        if st < 0 {
            return;
        }
        st += 1;
    }
    let st = (st - 1) as u32;
    let mut cnt = count;
    if cnt < 0 {
        cnt = cnt.wrapping_add(len);
        if cnt < 0 {
            return;
        }
    }
    let Some(avail) = (len as u32).checked_sub(st) else {
        return;
    };
    let n = (repl.len() as u32).min(avail).min(cnt as u32) as usize;
    let st = st as usize;
    target.0[st..st + n].copy_from_slice(&repl[..n]);
}

/// `LTRIM$(s)`; `PB_LTrim` 0x6bd16f. Removes leading spaces (0x20 only; no tabs). The game never uses
/// the `LTRIM$(s, [ANY] chars)` forms (0x6bd1e8 / 0x6bd27b have no call sites).
pub fn ltrim(s: &[u8]) -> PbStr {
    let k = s.iter().position(|&b| b != b' ').unwrap_or(s.len());
    PbStr(s[k..].to_vec())
}

/// `RTRIM$(s)`; `PB_RTrim` 0x6bd1c0. Removes trailing spaces (0x20 only). The `RTRIM$(s, [ANY] chars)`
/// forms (0x6bd253 / 0x6bd29f) are unused by the game.
pub fn rtrim(s: &[u8]) -> PbStr {
    let k = s.iter().rposition(|&b| b != b' ').map_or(0, |k| k + 1);
    PbStr(s[..k].to_vec())
}

/// `CharUpperBuffA` on Windows code page 1252: a-z, à-þ (not ÷) to upper case, š→Š, œ→Œ, ž→Ž, ÿ→Ÿ.
/// ß, µ and ƒ stay (their upper case is not in the code page).
pub fn upper_ansi(b: u8) -> u8 {
    match b {
        b'a'..=b'z' | 0xe0..=0xf6 | 0xf8..=0xfe => b - 0x20,
        0x9a | 0x9c | 0x9e => b - 0x10,
        0xff => 0x9f,
        _ => b,
    }
}

/// `CharLowerBuffA` on code page 1252: A-Z, À-Þ (not ×) to lower case, Š→š, Œ→œ, Ž→ž, Ÿ→ÿ.
pub fn lower_ansi(b: u8) -> u8 {
    match b {
        b'A'..=b'Z' | 0xc0..=0xd6 | 0xd8..=0xde => b + 0x20,
        0x8a | 0x8c | 0x8e => b + 0x10,
        0x9f => 0xff,
        _ => b,
    }
}

/// `GetStringTypeA(CT_CTYPE1)` has C1_ALPHA, code page 1252: the bytes that decode to Unicode letters
/// (A-Z, a-z, ƒ ˆ ŠŒŽšœžŸ, ª µ º, À-ÿ except × and ÷).
pub fn is_alpha_ansi(b: u8) -> bool {
    matches!(b,
        b'A'..=b'Z' | b'a'..=b'z' | 0x83 | 0x88 | 0x8a | 0x8c | 0x8e | 0x9a | 0x9c | 0x9e | 0x9f
        | 0xaa | 0xb5 | 0xba | 0xc0..=0xd6 | 0xd8..=0xf6 | 0xf8..=0xff)
}

/// OEM-mode upper case (0x6b9fe8): a-z plus the code page 437 pairs ä→Ä (84→8E), ö→Ö (94→99),
/// ü→Ü (81→9A), é→É (82→90), ñ→Ñ (A4→A5), å→Å (86→8F), ç→Ç (87→80).
pub fn upper_oem(b: u8) -> u8 {
    match b {
        b'a'..=b'z' => b ^ 0x20,
        0x84 => 0x8e,
        0x94 => 0x99,
        0x81 => 0x9a,
        0x82 => 0x90,
        0xa4 => 0xa5,
        0x86 => 0x8f,
        0x87 => 0x80,
        _ => b,
    }
}

/// OEM-mode lower case (0x6b9fac), the inverse of [`upper_oem`].
pub fn lower_oem(b: u8) -> u8 {
    match b {
        b'A'..=b'Z' => b ^ 0x20,
        0x8e => 0x84,
        0x99 => 0x94,
        0x9a => 0x81,
        0x90 => 0x82,
        0xa5 => 0xa4,
        0x8f => 0x86,
        0x80 => 0x87,
        _ => b,
    }
}

/// OEM-mode letter test used by MCASE$ (0x6ba024): A-Z, a-z, 80-82, 84, 86, 87, 8E-90, 94, 99, 9A,
/// A4, A5.
pub fn is_alpha_oem(b: u8) -> bool {
    matches!(b,
        b'A'..=b'Z' | b'a'..=b'z' | 0x80..=0x82 | 0x84 | 0x86 | 0x87 | 0x8e..=0x90 | 0x94 | 0x99 | 0x9a
        | 0xa4 | 0xa5)
}

/// `UCASE$(s)`; `PB_UCase` 0x6b9ea0 (mode in EAX).
pub fn ucase(s: &[u8], mode: CaseMode) -> PbStr {
    let f = match mode {
        CaseMode::Ansi => upper_ansi,
        CaseMode::Oem => upper_oem,
    };
    PbStr(s.iter().map(|&b| f(b)).collect())
}

/// `LCASE$(s)`; `PB_LCase` 0x6b9e5c (mode in EAX).
pub fn lcase(s: &[u8], mode: CaseMode) -> PbStr {
    let f = match mode {
        CaseMode::Ansi => lower_ansi,
        CaseMode::Oem => lower_oem,
    };
    PbStr(s.iter().map(|&b| f(b)).collect())
}

/// `MCASE$(s)` (mixed/proper case); `PB_MCase` 0x6b9eef (mode in EAX).
///
/// Walks the bytes with an "upper next" flag that starts set: each byte is upper-cased if the flag is
/// set, else lower-cased; then the flag is set again iff the (converted) byte is not a letter. So every
/// letter after a non-letter (space, digit, apostrophe, hyphen, ...) is capitalised:
/// `"o'neil mcdonald-smith"` → `"O'Neil Mcdonald-Smith"`, `"2nd"` → `"2Nd"`. In ANSI mode letters are
/// [`is_alpha_ansi`] (the original asks `GetStringTypeA` per character, stepping with `CharNextA`, which
/// is one byte on code page 1252), in OEM mode [`is_alpha_oem`].
pub fn mcase(s: &[u8], mode: CaseMode) -> PbStr {
    type Tables = (fn(u8) -> u8, fn(u8) -> u8, fn(u8) -> bool);
    let (up, low, alpha): Tables = match mode {
        CaseMode::Ansi => (upper_ansi, lower_ansi, is_alpha_ansi),
        CaseMode::Oem => (upper_oem, lower_oem, is_alpha_oem),
    };
    let mut upper_next = true;
    PbStr(
        s.iter()
            .map(|&b| {
                let c = if upper_next { up(b) } else { low(b) };
                upper_next = !alpha(c);
                c
            })
            .collect(),
    )
}

/// `INSTR(start, s, find)`; `PB_Instr` 0x6b8be4 (start in EAX). Returns the 1-based position or 0.
///
/// - `s == ""`: 0 (even for an empty `find`).
/// - `1 <= start <= len`: first occurrence starting at or after `start`; an empty `find` returns
///   `start`. `start == 0` or `start > len`: 0.
/// - `start < 0`: reverse search. `p = len + start + 1` (so -1 = the whole string); `p <= 0` gives 0;
///   else the last occurrence that lies entirely within the first `p` bytes. An empty `find` returns `p`.
pub fn instr(start: i32, s: &[u8], find_: &[u8]) -> i32 {
    let len = s.len() as i32;
    if s.is_empty() {
        return 0;
    }
    if start >= 0 {
        let off = start.wrapping_sub(1) as u32;
        if off >= len as u32 {
            return 0;
        }
        if find_.is_empty() {
            return start;
        }
        let off = off as usize;
        find(&s[off..], find_).map_or(0, |m| (off + m + 1) as i32)
    } else {
        let p = len + start + 1;
        if p <= 0 {
            return 0;
        }
        if find_.is_empty() {
            return p;
        }
        let w = &s[..p as usize];
        if find_.len() > w.len() {
            return 0;
        }
        w.windows(find_.len()).rposition(|x| x == find_).map_or(0, |m| m as i32 + 1)
    }
}

/// `INSTR(start, s, ANY chars)`; `PB_InstrAny` 0x6b8c54 (start in EAX). Position of the first byte of
/// `s` (at or after `start`) that is contained in `chars`, or 0. Ranges as in [`instr`]: empty `s` gives
/// 0; empty `chars` gives `start` (or `p` when `start < 0`); `start < 0` searches backwards from
/// position `p = len + start + 1`.
pub fn instr_any(start: i32, s: &[u8], chars: &[u8]) -> i32 {
    let len = s.len() as i32;
    if s.is_empty() {
        return 0;
    }
    if start >= 0 {
        let off = start.wrapping_sub(1) as u32;
        if off >= len as u32 {
            return 0;
        }
        if chars.is_empty() {
            return start;
        }
        let off = off as usize;
        s[off..].iter().position(|b| chars.contains(b)).map_or(0, |m| (off + m + 1) as i32)
    } else {
        let p = len + start + 1;
        if p <= 0 {
            return 0;
        }
        if chars.is_empty() {
            return p;
        }
        s[..p as usize].iter().rposition(|b| chars.contains(b)).map_or(0, |m| m as i32 + 1)
    }
}

// ------------------------------------------------------------------ comma-delimited (empty delimiter)

/// Number of separating commas for PARSECOUNT/PARSE$ with an empty delimiter; 0x6bd969.
///
/// Fields are scanned like this: leading spaces are skipped; a field whose first byte is `"` extends to
/// the closing quote, after which everything up to the next comma is ignored; any other field extends
/// to the next comma. Quotes are only special at the start of a field. A field that runs into the end
/// of the string (also an unterminated quote) does not count a separator.
fn csv_count(s: &[u8]) -> i32 {
    let mut n = 0;
    let mut i = 0;
    let comma_from = |i: usize| s[i..].iter().position(|&b| b == b',').map(|k| i + k);
    loop {
        if i >= s.len() {
            return n;
        }
        let c = s[i];
        i += 1;
        if c == b' ' {
            continue;
        }
        if c == b'"' {
            match s[i..].iter().position(|&b| b == b'"') {
                None => return n,
                Some(k) => i += k + 1,
            }
            match comma_from(i) {
                None => return n,
                Some(j) => i = j + 1,
            }
        } else if c != b',' {
            match comma_from(i) {
                None => return n,
                Some(j) => i = j + 1,
            }
        }
        n += 1;
    }
}

/// Field `k` (1-based) of a comma-delimited string; 0x6bda62. `k <= 0` or beyond the last field: `""`.
///
/// Leading spaces are skipped. A quoted field yields the bytes between the quotes (up to the end if
/// unterminated; nothing after the closing quote); an unquoted field yields the bytes up to the next
/// comma with trailing spaces removed (only 0x20, not tabs).
fn csv_field(s: &[u8], mut k: i32) -> &[u8] {
    let mut i = 0;
    let comma_from = |i: usize| s[i..].iter().position(|&b| b == b',').map(|j| i + j);
    loop {
        if i >= s.len() {
            return &[];
        }
        let c = s[i];
        i += 1;
        if c == b' ' {
            continue;
        }
        k = k.wrapping_sub(1);
        if k == 0 {
            return csv_take(s, i - 1).0;
        }
        if c == b'"' {
            match s[i..].iter().position(|&b| b == b'"') {
                None => return &[],
                Some(j) => i += j + 1,
            }
            match comma_from(i) {
                None => return &[],
                Some(j) => i = j + 1,
            }
        } else if c != b',' {
            match comma_from(i) {
                None => return &[],
                Some(j) => i = j + 1,
            }
        }
    }
}

/// The field starting at byte `start` (a non-space), and the position after its comma.
fn csv_take(s: &[u8], start: usize) -> (&[u8], usize) {
    if s[start] == b'"' {
        let body = start + 1;
        match s[body..].iter().position(|&b| b == b'"') {
            None => (&s[body..], s.len()),
            Some(j) => {
                let close = body + j;
                let next = s[close + 1..].iter().position(|&b| b == b',').map_or(s.len(), |k| close + 1 + k + 1);
                (&s[body..close], next)
            }
        }
    } else {
        let (mut end, next) = match s[start..].iter().position(|&b| b == b',') {
            Some(k) => (start + k, start + k + 1),
            None => (s.len(), s.len()),
        };
        while end > start && s[end - 1] == b' ' {
            end -= 1;
        }
        (&s[start..end], next)
    }
}

/// Next field of a comma-delimited string for `PARSE s, "", arr()`; 0x6bd8b8. Same field rules as
/// [`csv_field`].
fn csv_next(s: &[u8], mut i: usize) -> (&[u8], usize) {
    while i < s.len() && s[i] == b' ' {
        i += 1;
    }
    if i >= s.len() {
        return (&[], s.len());
    }
    csv_take(s, i)
}

// ------------------------------------------------------------------ PARSE family

/// `PARSECOUNT(s, delim)`; `PB_ParseCount` 0x6bd90c. Non-overlapping occurrences of `delim` plus one
/// (so `PARSECOUNT("", ",")` = 1). An empty `delim` means comma-delimited with quote handling (see
/// [`csv_count`]).
pub fn parse_count(s: &[u8], delim: &[u8]) -> i32 {
    if delim.is_empty() {
        csv_count(s) + 1
    } else {
        count_occ(s, delim) + 1
    }
}

/// `PARSECOUNT(s, ANY chars)`; `PB_ParseCountAny` 0x6bd935. Number of bytes of `s` in `chars`, plus
/// one; 1 if `chars` is empty.
pub fn parse_count_any(s: &[u8], chars: &[u8]) -> i32 {
    if chars.is_empty() {
        1
    } else {
        count_any(s, chars) + 1
    }
}

/// `PARSE$(s, delim, n)`; `PB_Parse` 0x6bd993 (n in EAX).
///
/// Field `n` (1-based) of `s` split at non-overlapping occurrences of `delim`; negative `n` counts from
/// the end (`n + PARSECOUNT + 1`, -1 = last field). `n == 0` or out of range: `""`. An empty `delim`
/// means comma-delimited with quotes and space trimming, see [`csv_field`].
pub fn parse(s: &[u8], delim: &[u8], n: i32) -> PbStr {
    if delim.is_empty() {
        let k = field_index(n, csv_count(s));
        return PbStr(csv_field(s, k).to_vec());
    }
    let mut k = field_index(n, count_occ(s, delim));
    let mut pos = 0;
    loop {
        k = k.wrapping_sub(1);
        match find(&s[pos..], delim) {
            Some(m) => {
                if k == 0 {
                    return PbStr(s[pos..pos + m].to_vec());
                }
                pos += m + delim.len();
            }
            None => {
                return if k == 0 { PbStr(s[pos..].to_vec()) } else { PbStr::new() };
            }
        }
    }
}

/// `PARSE$(s, ANY chars, n)`; `PB_ParseAny` 0x6bd9e6 (n in EAX). Like [`parse`] with every byte of
/// `chars` acting as a one-byte delimiter. Quirk: with an empty `chars` the whole of `s` is returned
/// for any `n`.
pub fn parse_any(s: &[u8], chars: &[u8], n: i32) -> PbStr {
    if chars.is_empty() {
        return PbStr(s.to_vec());
    }
    let mut k = field_index(n, count_any(s, chars));
    let mut pos = 0;
    loop {
        k = k.wrapping_sub(1);
        match s[pos..].iter().position(|b| chars.contains(b)) {
            Some(m) => {
                if k == 0 {
                    return PbStr(s[pos..pos + m].to_vec());
                }
                pos += m + 1;
            }
            None => {
                return if k == 0 { PbStr(s[pos..].to_vec()) } else { PbStr::new() };
            }
        }
    }
}

/// `PARSE s, delim, arr()` for a dynamic string array of `n` elements; `PB_ParseArray` 0x6bddc5.
///
/// Element `i` receives field `i + 1` (missing fields give `""`; fields beyond `n` are dropped, the last
/// element does *not* get the remainder), i.e. the same as `PARSE$(s, delim, i + 1)`, including the
/// comma-delimited rules for an empty `delim`.
pub fn parse_array(s: &[u8], delim: &[u8], n: usize) -> Vec<PbStr> {
    let mut out = Vec::with_capacity(n);
    let mut pos = 0;
    for _ in 0..n {
        if pos >= s.len() && !delim.is_empty() {
            out.push(PbStr::new());
            continue;
        }
        let field = if delim.is_empty() {
            let (f, next) = csv_next(s, pos);
            pos = next;
            f
        } else {
            match find(&s[pos..], delim) {
                Some(m) => {
                    let f = &s[pos..pos + m];
                    pos += m + delim.len();
                    f
                }
                None => {
                    let f = &s[pos..];
                    pos = s.len();
                    f
                }
            }
        };
        out.push(PbStr(field.to_vec()));
    }
    out
}

// ------------------------------------------------------------------ REPLACE / REMOVE$ / RETAIN$

/// `REPLACE find WITH with IN s`; `PB_Replace` 0x6bd2ee. Replaces all non-overlapping occurrences,
/// left to right. An empty `find` leaves `s` unchanged.
pub fn replace(s: &[u8], find_: &[u8], with: &[u8]) -> PbStr {
    if find_.is_empty() {
        return PbStr(s.to_vec());
    }
    let mut out = Vec::with_capacity(s.len());
    let mut pos = 0;
    while let Some(m) = find(&s[pos..], find_) {
        out.extend_from_slice(&s[pos..pos + m]);
        out.extend_from_slice(with);
        pos += m + find_.len();
    }
    out.extend_from_slice(&s[pos..]);
    PbStr(out)
}

/// `REPLACE ANY chars WITH with IN s`; `PB_ReplaceAny` 0x6bd44b. Each byte of `s` that occurs in
/// `chars` is replaced by the byte of `with` at the index of its *first* occurrence in `chars`.
/// `chars` and `with` must have the same length, else the original raises error 5 and yields `""`
/// (returned here as `""`). Empty `chars` (and `with`): `s` unchanged.
pub fn replace_any(s: &[u8], chars: &[u8], with: &[u8]) -> PbStr {
    if chars.len() != with.len() {
        return PbStr::new();
    }
    PbStr(
        s.iter()
            .map(|&b| chars.iter().position(|&c| c == b).map_or(b, |k| with[k]))
            .collect(),
    )
}

/// `REMOVE$(s, sub)`; `PB_Remove` 0x6be36a. Removes all non-overlapping occurrences of `sub`.
pub fn remove(s: &[u8], sub: &[u8]) -> PbStr {
    replace(s, sub, b"")
}

/// `REMOVE$(s, ANY chars)`; `PB_RemoveAny` 0x6be447. Drops every byte contained in `chars`.
pub fn remove_any(s: &[u8], chars: &[u8]) -> PbStr {
    PbStr(s.iter().copied().filter(|b| !chars.contains(b)).collect())
}

/// `RETAIN$(s, ANY chars)`; `PB_RetainAny` 0x6be55c. Keeps only the bytes contained in `chars`
/// (empty `chars` gives `""`).
pub fn retain_any(s: &[u8], chars: &[u8]) -> PbStr {
    PbStr(s.iter().copied().filter(|b| chars.contains(b)).collect())
}

/// Validate a 1-based EXTRACT$/REMAIN$ start: `Some(start - 1)` iff `1 <= start <= len`.
fn start_offset(start: i32, len: usize) -> Option<usize> {
    let off = start.wrapping_sub(1) as u32;
    if off >= len as u32 {
        None
    } else {
        Some(off as usize)
    }
}

/// `EXTRACT$(start, s, m)`; `PB_Extract` 0x6bcdcf (start in EAX). The part of `s` from `start` up to
/// (excluding) the first occurrence of `m` at or after `start`; the rest of `s` if there is none (also
/// for an empty `m`). `start <= 0` or `start > len`: `""` (no negative positions).
pub fn extract(start: i32, s: &[u8], m: &[u8]) -> PbStr {
    let Some(off) = start_offset(start, s.len()) else {
        return PbStr::new();
    };
    let rest = &s[off..];
    let end = find(rest, m).unwrap_or(rest.len());
    PbStr(rest[..end].to_vec())
}

/// `REMAIN$(start, s, m)`; `PB_Remain` 0x6bce7a (start in EAX). The part of `s` after the first
/// occurrence of `m` at or after `start`; `""` if there is none, if `m` is empty, or if `start` is out
/// of range (`start <= 0` or `start > len`).
pub fn remain(start: i32, s: &[u8], m: &[u8]) -> PbStr {
    let Some(off) = start_offset(start, s.len()) else {
        return PbStr::new();
    };
    match find(&s[off..], m) {
        Some(k) => PbStr(s[off + k + m.len()..].to_vec()),
        None => PbStr::new(),
    }
}

// ------------------------------------------------------------------ constructors, ASC, LEN, compare

/// `STRING$(n, s)`; `PB_StringFill` 0x6bc210 (n in EBX). `n` copies of the first byte of `s` (NUL if
/// `s` is empty). `n < 0` raises error 7 in the original and yields `""`.
pub fn string_fill(n: i32, s: &[u8]) -> PbStr {
    string_chr(n, s.first().copied().unwrap_or(0) as i32)
}

/// `STRING$(n, code)`; `PB_StringChr` 0x6bc1e5 (n in EBX, code in EAX). `n` copies of the low byte of
/// `code`. `n < 0`: error 7, `""`.
pub fn string_chr(n: i32, code: i32) -> PbStr {
    if n <= 0 {
        return PbStr::new();
    }
    PbStr(vec![code as u8; n as usize])
}

/// `SPACE$(n)`; `PB_Space` 0x6bc054 (n in EAX). `n < 0`: error 7, `""`.
pub fn space(n: i32) -> PbStr {
    string_chr(n, 0x20)
}

/// `CHR$(a, b, ...)`; `PB_Chr` 0x6b76ff (arguments on the CPU stack). One byte per argument, the low
/// byte of the value (`CHR$(256)` = NUL, `CHR$(-2)` = 0xFE); arguments equal to -1 are skipped.
pub fn chr(codes: &[i32]) -> PbStr {
    PbStr(codes.iter().filter(|&&c| c != -1).map(|&c| c as u8).collect())
}

/// `ASC(s)`; `PB_Asc` 0x6bd709. The first byte, or -1 for `""`.
pub fn asc(s: &[u8]) -> i32 {
    s.first().map_or(-1, |&b| b as i32)
}

/// `LEN(s)`; `PB_Len` 0x6bf094.
pub fn len(s: &[u8]) -> i32 {
    s.len() as i32
}

/// String comparison; `PB_StrEqual` 0x6be966 (the relational operators test its ZF/CF).
///
/// Byte-wise unsigned (`repe cmpsb` over the common length); if the common prefix is equal the shorter
/// string is less. This is the same as the `Ord` of `PbStr` / `[u8]`.
pub fn compare(a: &[u8], b: &[u8]) -> Ordering {
    a.cmp(b)
}
