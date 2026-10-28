//! Number <-> text conversion of the PowerBASIC 8 runtime in WSR.EXE: `STR$`, `FORMAT$(x)` without
//! a mask, `WRITE #`, `VAL` and the `INPUT #` / `LINE INPUT #` tokenizers.
//!
//! Everything here is a transliteration of the original x86 code, verified bit-for-bit against the
//! original running under CPU emulation (`tools/emu/numfmt.py`, fixture `tests/fixtures/numfmt.txt`).
//!
//! The runtime does all arithmetic on the x87 in 80-bit extended precision (control word 0x133f:
//! 64-bit mantissa, round to nearest even). To be exact, the conversions below therefore run on a
//! software model of the x87 extended format ([`Ext80`], `pb::ext80`).
//!
//! # Number -> text (`STR$`, `FORMAT$(x)`, `WRITE #`)
//!
//! All entry points funnel into 0x6bd52d (`digits` in ECX, keep-sign-column flag in EDX):
//!
//! | original              | Rust                      | digits | leading space for x >= 0 |
//! |-----------------------|---------------------------|--------|--------------------------|
//! | 0x6bd503 PB_StrNum    | `str_num(x, 15)`          | 15     | yes                      |
//! | 0x6bd4ef PB_StrNum7   | `str_num(x, 7)`           | 7      | yes                      |
//! | 0x6bd517 StrNumDigits | `str_num(x, n)`           | EAX    | yes                      |
//! | 0x6bd4d0 PB_FormatNum | `format_num(x, 15)`       | 15     | no                       |
//! | 0x6bd4bf              | `format_num(x, 7)`        | 7      | no                       |
//! | 0x6bd4e1              | `format_num(x, n)`        | EAX    | no                       |
//! | 0x6b7d0b PB_WriteNum  | `write_num(x)`            | 15     | no                       |
//!
//! A digit count of 0 or >= 18 (unsigned) means 18.
//!
//! Digit generation (0x6bf404, [`digits_core`]): for |x| with binary exponent `e`, the decimal
//! exponent is estimated as `d = (e * 0x4d10) >> 16` (0x4d10/65536 ~ log10 2), then |x| is scaled by
//! `10^(17-d)` (0x6bf530, see [`scale10`]), rounded to an integer (FRNDINT); if that is >= 1e18 it is
//! divided by 10 (and `d` incremented). The integer (18 digits) is converted with FISTP and its
//! decimal digits (followed by two '0' sentinels) are taken `digits + 1` at a time. The last digit
//! is then used to round the first `digits` digits: > '5' rounds up, == '5' rounds up only if the
//! previous digit is odd (round-half-even on *one* digit, ignoring the digits after it, so this is
//! a double rounding: first to 18 significant digits, then to `digits`). A carry out of the first
//! digit yields "100..." and increments the exponent.
//!
//! Layout (0x6bf260 / 0x6bf294): trailing zeros of the digit string are dropped (`sig` digits
//! remain, decimal exponent `exp` = number of integer digits). With `n` = digits:
//! * integer (`exp >= sig`): plain digits padded with zeros if `exp <= n`, else `d.dddE+xx`;
//! * fraction (`exp < sig`), `f = sig - exp` fraction digits: if `f <= n` the number is printed
//!   positionally with *no* leading "0" before the point (".5", "-.001234", "12.25"), else
//!   `d.dddE-xx`;
//! * the exponent has no leading zeros and 1-4 digits ("1E+15", "3.33333333333333E-3").
//!
//! The result is `<sign column><text>` where the sign column is '-' or ' '; FORMAT$ and WRITE #
//! drop the ' '. Zero prints as "0" (" 0" for STR$); -0 prints like 0.
//!
//! # WRITE #
//!
//! A numeric item is exactly `format_num(x, 15)` (0x6b7d0b calls 0x6bf28f = 15 digits and strips the
//! sign column). A string item (0x6b7d2f) is `"` + bytes + `"` with no escaping of embedded quotes.
//! The compiler emits PB_WriteSep (0x6b7d5d, writes ",") between items and PB_PrintEnd (0x6b8fe8 ->
//! 0x6bee67, writes "\r\n") at the end of the statement; nothing else is written (no trailing
//! separator, no spaces).
//!
//! # Text -> number (`VAL`, `INPUT #`)
//!
//! 0x6bea23 ([`parse_number_ext`]) parses a byte string: `&` prefix -> radix integer (0x6bea3b),
//! otherwise decimal (0x6bea96). VAL (0x6bd6bb) first skips leading spaces, TABs and LFs (0x6bd797;
//! not CR). The decimal parser accepts an optional single sign, digits with at most one '.', and an
//! exponent introduced by E/e/D/d with an optional sign; parsing stops silently at the first
//! character that does not fit. At most 18 significant digits are accumulated (exactly, in an i64);
//! further digits are truncated (only counted). The value is `int * 10^k`, computed as one
//! multiplication or division by `10^|k|` (built by [`scale10`] from 1.0), so results like VAL("0.1")
//! are correctly rounded *80-bit* values which are then rounded once more to f64 by [`val`].
//! Exponents are clamped to 9999 (16-bit arithmetic, see [`parse_dec`] for the wrap-around quirk);
//! a decimal exponent (point position + exponent) >= 4933 gives 1.7014118e38 (the f32 constant
//! 0x7f000000), <= -4933 gives 0. A lone sign or "-0" gives -0.0.
//!
//! Radix integers: `&H` (hex), `&O`/`&Q` (octal), `&B` (binary), or `&` followed directly by digits
//! (octal). Up to 64 bits; overflow or no digits gives 0. Optional type suffixes `%`, `&`, `&&`,
//! `?`, `??`, `???`. Values whose leading digit is not 0 and that fit in 16 bits are sign-extended
//! from 16 bits (`&HFFFF` = -1, `&H8000` = -32768) unless followed by `&`; values that need 17..32
//! bits are sign-extended from 32 bits unless followed by `&&`; a leading 0 digit (`&H0FFFF`)
//! prevents sign extension. With a `?` suffix negative 64-bit values are an error (0).
//!
//! # INPUT # / LINE INPUT # (see [`InputReader`])
//!
//! Mirrors 0x6ba3ea (numeric item), 0x6b85e3 (string item), 0x6b84bb (LINE INPUT), 0x6b7b17 (EOF)
//! and the shared helpers 0x6b86dc (skip spaces) and 0x6b8702 (consume the item terminator) for a
//! buffered disk file. The buffer size of the original does not influence the results (checked in
//! the fixture with buffer sizes 1..4096).

use std::cmp::Ordering;

pub use crate::pb::ext80::Ext80;

/// Constants of the runtime power table at 0x6be5f1 (pointer at 0x6b71a8): 10^(8*2^i), i = 0..9,
/// as stored (rounded) 80-bit values, followed by 1e18 (+0x84).
const POW_TABLE: [Ext80; 10] = [
    Ext80 {
        se: 0x4019,
        mant: 0xbebc200000000000,
    },
    Ext80 {
        se: 0x4034,
        mant: 0x8e1bc9bf04000000,
    },
    Ext80 {
        se: 0x4069,
        mant: 0x9dc5ada82b70b59e,
    },
    Ext80 {
        se: 0x40d3,
        mant: 0xc2781f49ffcfa6d5,
    },
    Ext80 {
        se: 0x41a8,
        mant: 0x93ba47c980e98ce0,
    },
    Ext80 {
        se: 0x4351,
        mant: 0xaa7eebfb9df9de8e,
    },
    Ext80 {
        se: 0x46a3,
        mant: 0xe319a0aea60e91c7,
    },
    Ext80 {
        se: 0x4d48,
        mant: 0xc976758681750c17,
    },
    Ext80 {
        se: 0x5a92,
        mant: 0x9e8b3b5dc53d5de5,
    },
    Ext80 {
        se: 0x7525,
        mant: 0xc46052028a20979b,
    },
];
const TEN_POW_4096: Ext80 = POW_TABLE[9];
pub(crate) const E18: Ext80 = Ext80 {
    se: 0x403a,
    mant: 0xde0b6b3a76400000,
};

/// 0x6bf530: `x * 10^n` (n >= 0) or `x / 10^|n|` (n < 0). One factor of 10^4096 is applied first
/// if |n| > 4096. Then `10^|n|` is built as FILD(10^(|n| & 7)) times the table entries for the set
/// bits of |n| >> 3 (lowest first), each product rounded to 64 bits, and applied with one FMUL/FDIV.
pub(crate) fn scale10(mut x: Ext80, mut n: i32) -> Ext80 {
    if n > 0x1000 {
        x = x.fmul(TEN_POW_4096);
        n -= 0x1000;
    }
    if n < -0x1000 {
        x = x.fdiv(TEN_POW_4096);
        n += 0x1000;
    }
    if n == 0 {
        return x;
    }
    let mut m = n.unsigned_abs();
    let mut p = Ext80::from_i64(10i64.pow(m & 7));
    m >>= 3;
    let mut i = 0;
    while m != 0 {
        if m & 1 == 1 {
            // indices >= 10 would read past the table (never reached for |n| < 8192)
            p = p.fmul(if i < 10 { POW_TABLE[i] } else { E18 });
        }
        i += 1;
        m >>= 1;
    }
    if n > 0 { x.fmul(p) } else { x.fdiv(p) }
}

/// 0x6bf404: the first `n` significant decimal digits of |x| (ASCII), correctly placed decimal
/// exponent (number of integer digits) and the sign. See the module docs for the algorithm.
fn digits_core(x: Ext80, n: usize) -> (Vec<u8>, i32, bool) {
    let e = x.se & 0x7fff;
    if e == 0 {
        return (vec![b'0'; n], 1, false);
    }
    if e == 0x7fff {
        return if x.mant == 1 << 63 {
            (vec![b'9'; n], 0x3e6 + 1, x.is_neg())
        } else {
            (vec![b'0'; n], 1, false)
        };
    }
    let a = x.abs();
    let mut d = (((e as i16).wrapping_sub(0x3fff) as i32 * 0x4d10) >> 16) as i16;
    let k = 0x11i16.wrapping_sub(d) as i32;
    let mut s = scale10(a, k).frndint();
    if E18.cmp_val(s) != Ordering::Greater {
        s = s.fdiv(Ext80::from_i64(10));
        d = d.wrapping_add(1);
    }
    let v = s.to_i64();
    let mut all: Vec<u8> = v.unsigned_abs().to_string().into_bytes();
    if v == 0 {
        all.clear(); // the digit loop always emits at least one digit; v is never 0 here
        all.push(b'0');
    }
    all.extend_from_slice(b"00");
    let take = (n + 1).min(all.len());
    let mut digs: Vec<u8> = all[..take].to_vec();
    let r = digs.pop().unwrap();
    let round_up = r > b'5' || (r == b'5' && digs.last().is_some_and(|p| p & 1 == 1));
    if round_up {
        let mut i = digs.len();
        loop {
            i -= 1;
            digs[i] += 1;
            if digs[i] <= b'9' {
                break;
            }
            digs[i] = b'0';
            if i == 0 {
                digs[0] += 1;
                d = d.wrapping_add(1);
                break;
            }
        }
    }
    (digs, d as i32 + 1, x.is_neg())
}

/// 0x6bf294: build "<sign column><text> " for `n` digits (with the trailing blank the original
/// appends; callers strip it).
fn format_core(x: Ext80, n: usize) -> Vec<u8> {
    let (digs, exp, neg) = digits_core(x, n);
    // 0x6bf260: sign column + digits, strip trailing zeros
    let mut buf = Vec::with_capacity(n + 1);
    buf.push(if neg { b'-' } else { b' ' });
    buf.extend_from_slice(&digs);
    let sig = digs.iter().rposition(|&c| c != b'0').map_or(0, |p| p + 1);
    let (eax, ebx) = if sig == 0 {
        (0i32, 2usize)
    } else {
        (exp - sig as i32, sig + 1)
    };
    let mut out: Vec<u8> = Vec::with_capacity(32);
    let n1 = n as i32 + 1;
    let mut eform: Option<(u8, i32)> = None;
    if eax > 0 {
        if ebx as i32 + eax > n1 {
            eform = Some((b'+', ebx as i32 + eax - 2));
        } else {
            out.extend_from_slice(&buf[..ebx]);
            out.extend(std::iter::repeat_n(b'0', eax as usize));
        }
    } else if eax == 0 {
        out.extend_from_slice(&buf[..ebx]);
    } else {
        let f = -eax;
        if f > n as i32 {
            eform = Some((b'-', f - ebx as i32 + 2));
        } else {
            let f = f as usize;
            out.push(buf[0]);
            let c = ebx - 1;
            if c > f {
                out.extend_from_slice(&buf[1..1 + c - f]);
                out.push(b'.');
                out.extend_from_slice(&buf[1 + c - f..ebx]);
            } else {
                out.push(b'.');
                out.extend(std::iter::repeat_n(b'0', f - c));
                out.extend_from_slice(&buf[1..ebx]);
            }
        }
    }
    if let Some((esign, ev)) = eform {
        out.extend_from_slice(&buf[..2]);
        if ebx > 2 {
            out.push(b'.');
            out.extend_from_slice(&buf[2..ebx]);
        }
        out.push(b'E');
        out.push(esign);
        let ev = ev.unsigned_abs();
        let (th, hu, te, on) = (
            (ev / 100) / 10 % 256,
            (ev / 100) % 10,
            (ev % 100) / 10,
            ev % 10,
        );
        let ds = [
            th as u8 + b'0',
            hu as u8 + b'0',
            te as u8 + b'0',
            on as u8 + b'0',
        ];
        let skip = if ds[0] != b'0' {
            0
        } else if ds[1] != b'0' {
            1
        } else if ds[2] != b'0' {
            2
        } else {
            3
        };
        out.extend_from_slice(&ds[skip..]);
    }
    out.push(b' ');
    strip_fraction_zeros(&mut out);
    out
}

/// 0x6bf3c5: remove zeros at the end of a fraction (and a then-dangling '.'). The digit strings
/// produced above never end in '0' after a '.', so this is a no-op in practice; kept for fidelity.
fn strip_fraction_zeros(out: &mut Vec<u8>) {
    let Some(p) = out.iter().position(|&c| c == b'.') else {
        return;
    };
    let mut q = p + 1;
    while q < out.len() && out[q].is_ascii_digit() {
        q += 1;
    }
    let mut w = q;
    while out[w - 1] == b'0' {
        w -= 1;
    }
    if out[w - 1] == b'.' {
        w -= 1;
    }
    out.drain(w..q);
}

/// 0x6bd52d: digit clamp, trailing blank removed, sign column kept (STR$) or dropped (FORMAT$).
fn num_text(x: Ext80, digits: u32, keep_sign_column: bool) -> String {
    let n = if digits == 0 || digits >= 18 {
        18
    } else {
        digits as usize
    };
    let mut out = format_core(x, n);
    out.pop();
    if !keep_sign_column && out.first() == Some(&b' ') {
        out.remove(0);
    }
    String::from_utf8(out).expect("ASCII")
}

/// `STR$(x)` with `digits` significant digits (15 for EXT/DOUBLE/LONG/QUAD, 7 for SINGLE/INTEGER
/// in the original: 0x6bd503 / 0x6bd4ef / 0x6bd517). Non-negative numbers get a leading space.
pub fn str_num(x: f64, digits: u32) -> String {
    num_text(Ext80::from_f64(x), digits, true)
}

/// `FORMAT$(x)` without a mask (0x6bd4d0 = 15 digits, 0x6bd4bf = 7, 0x6bd4e1 = EAX): like
/// [`str_num`] without the leading space.
pub fn format_num(x: f64, digits: u32) -> String {
    num_text(Ext80::from_f64(x), digits, false)
}

/// [`str_num`] for a full 80-bit EXT value.
pub fn str_num_ext(x: Ext80, digits: u32) -> String {
    num_text(x, digits, true)
}

/// [`format_num`] for a full 80-bit EXT value.
pub fn format_num_ext(x: Ext80, digits: u32) -> String {
    num_text(x, digits, false)
}

/// Text of one numeric `WRITE #` item (PB_WriteNum 0x6b7d0b): `FORMAT$(x)` with 15 digits.
pub fn write_num(x: f64) -> String {
    format_num(x, 15)
}

/// Bytes of one string `WRITE #` item (PB_WriteStr 0x6b7d2f): the bytes in double quotes, embedded
/// quotes are *not* escaped.
pub fn write_str(s: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(s.len() + 2);
    v.push(b'"');
    v.extend_from_slice(s);
    v.push(b'"');
    v
}

/// `VAL(s)` (0x6bd6bb), rounded from the 80-bit result to f64.
pub fn val(s: &[u8]) -> f64 {
    val_ext(s).to_f64()
}

/// `VAL(s)` as the exact 80-bit result.
pub fn val_ext(s: &[u8]) -> Ext80 {
    // 0x6bd797: skip spaces, TABs and LFs
    let start = s
        .iter()
        .position(|&c| c != b' ' && c != b'\t' && c != b'\n')
        .unwrap_or(s.len());
    parse_number_ext(&s[start..])
}

/// 0x6bea23: parse a number token (no whitespace skipping) to an 80-bit value.
pub fn parse_number_ext(s: &[u8]) -> Ext80 {
    if s.first() == Some(&b'&') {
        parse_radix(s)
    } else {
        parse_dec(s)
    }
}

/// 0x6bea96: decimal parser (see module docs). Transliterated with the original's 16-bit state.
fn parse_dec(s: &[u8]) -> Ext80 {
    let mut acc: u64 = 0; // [ebp-0x14]
    let mut neg = false; // [ebp-8]
    let mut point: i16 = i16::MIN; // [ebp-2], 0x8000 = no '.'
    let mut di: i16 = -2; // significant digit count, -2 = no digit, -1 = only zeros
    let mut i = 0usize;
    let mut cx: i32 = s.len() as i32;
    let get = |i: &mut usize, cx: &mut i32| -> Option<u8> {
        *cx -= 1;
        if *cx < 0 {
            return None;
        }
        let c = s[*i];
        *i += 1;
        Some(c)
    };
    // mantissa; `term` = terminating char (None = end of input, the original then uses 'e')
    let mut c = get(&mut i, &mut cx);
    if let Some(ch @ (b'+' | b'-')) = c {
        neg = ch == b'-';
        c = get(&mut i, &mut cx);
    }
    let term: Option<u8> = loop {
        let Some(ch) = c else { break None };
        if ch == b'.' {
            if point != i16::MIN {
                break Some(ch);
            }
            point = if di > 0 { di } else { 0 };
        } else if ch.is_ascii_digit() {
            let d = ch - b'0';
            di = di.wrapping_add(1);
            if di > 0 {
                if di <= 18 {
                    acc = acc.wrapping_mul(10).wrapping_add(d as u64);
                }
            } else {
                acc = (acc & !0xff) | d as u64;
                di = 1;
                if d == 0 {
                    di = -1;
                    if point != i16::MIN {
                        point = point.wrapping_sub(1);
                    }
                }
            }
        } else {
            break Some(ch);
        }
        c = get(&mut i, &mut cx);
    };
    let signed = |v: Ext80| if neg { v.chs() } else { v };
    if di == -2 {
        return signed(Ext80::ZERO);
    }
    if point == i16::MIN {
        point = di;
    }
    let ndig = di;
    // exponent
    let mut exp: u16 = 0;
    let mut eneg = false;
    if matches!(term, None | Some(b'E' | b'e' | b'D' | b'd')) {
        let mut c = get(&mut i, &mut cx);
        if let Some(ch @ (b'+' | b'-')) = c {
            eneg = ch == b'-';
            c = get(&mut i, &mut cx);
        }
        while let Some(ch) = c {
            if !ch.is_ascii_digit() {
                break;
            }
            exp = exp.wrapping_mul(10).wrapping_add((ch - b'0') as u16);
            if exp > 9999 {
                exp = 9999;
            }
            c = get(&mut i, &mut cx);
        }
    }
    if ndig < 0 {
        return signed(Ext80::ZERO);
    }
    let exp: i16 = if eneg {
        (exp as i16).wrapping_neg()
    } else {
        exp as i16
    };
    let total = point.wrapping_add(exp);
    if total >= 0x1345 {
        return signed(Ext80 {
            se: 0x407e,
            mant: 1 << 63,
        }); // f32 0x7f000000 = 2^127
    }
    if total <= -0x1345 {
        return signed(Ext80::ZERO);
    }
    let used = ndig.min(18);
    let pow = exp.wrapping_add(point.wrapping_sub(used));
    let v = Ext80::from_i64(acc as i64);
    let r = if pow == 0 {
        v
    } else {
        let p = scale10(Ext80::ONE, (pow as i32).abs());
        if pow < 0 { v.fdiv(p) } else { v.fmul(p) }
    };
    signed(r)
}

/// 0x6bea3b / 0x6bed41: `&H`/`&O`/`&Q`/`&B`/`&` radix integers (see module docs).
fn parse_radix(s: &[u8]) -> Ext80 {
    let at = |k: usize| -> u8 { s.get(k).copied().unwrap_or(0) };
    // esi = index, ecx = remaining count
    let mut esi = 1usize;
    let mut ecx: i64 = s.len() as i64 - 2;
    let letter = at(1);
    esi += 1;
    let radix: u64 = match letter {
        b'H' | b'h' => 16,
        b'B' | b'b' => 2,
        b'O' | b'o' | b'Q' | b'q' => 8,
        _ => {
            esi -= 1;
            ecx += 1;
            8
        }
    };
    if ecx < 1 {
        return Ext80::ZERO;
    }
    let shift = match radix {
        2 => 1,
        8 => 3,
        _ => 4,
    };
    // 0x6bed21: one digit, consumed only if valid
    let digit = |esi: &mut usize, ecx: &mut i64| -> Option<u64> {
        *ecx -= 1;
        if *ecx < 0 {
            *ecx += 1;
            return None;
        }
        let c = s[*esi].to_ascii_uppercase();
        let mut d = c.wrapping_sub(b'0');
        if c < b'0' {
            *ecx += 1;
            return None;
        }
        if d > 9 {
            d = d.wrapping_sub(7);
            if d <= 9 {
                *ecx += 1;
                return None;
            }
        }
        if (d as u64) < radix {
            *esi += 1;
            Some(d as u64)
        } else {
            *ecx += 1;
            None
        }
    };
    let Some(first) = digit(&mut esi, &mut ecx) else {
        return Ext80::ZERO;
    };
    let mut acc: u64 = first;
    let mut bl: u8 = if first == 0 { 1 } else { 0 };
    while let Some(d) = digit(&mut esi, &mut ecx) {
        for _ in 0..shift {
            if acc >> 63 != 0 {
                return Ext80::ZERO; // overflow
            }
            acc <<= 1;
        }
        let (v, c) = acc.overflowing_add(d);
        if c {
            return Ext80::ZERO;
        }
        acc = v;
    }
    // 0x6bed8e: type suffix (0x6bee04 reads with the 16-bit count `cx`)
    let mut cx = ecx;
    let mut next = |esi: &mut usize| -> Option<u8> {
        cx -= 1;
        if cx < 0 {
            return None;
        }
        let c = s[*esi];
        *esi += 1;
        Some(c)
    };
    match next(&mut esi) {
        None | Some(b'%') => {}
        Some(b'?') => {
            bl |= 2;
            match next(&mut esi) {
                None => {}
                Some(b'?') => match next(&mut esi) {
                    None | Some(b'?') => {}
                    Some(_) => esi -= 1,
                },
                Some(_) => esi -= 1,
            }
        }
        Some(b'&') => match next(&mut esi) {
            None | Some(b'&') => {}
            Some(_) => esi -= 1,
        },
        Some(_) => esi -= 1,
    }
    let mut lo = acc as u32;
    let mut hi = (acc >> 32) as u32;
    let back = |k: usize| -> u8 { if esi >= k { s[esi - k] } else { 0 } };
    match bl {
        0 => {
            if hi == 0 && !(back(2) == b'&' && back(1) == b'&') {
                if lo & 0xffff_0000 != 0 {
                    if lo & 0x8000_0000 != 0 {
                        hi = u32::MAX;
                    }
                } else if back(1) != b'&' && lo & 0x8000 != 0 {
                    lo |= 0xffff_0000;
                    hi = u32::MAX;
                }
            }
        }
        1 => {}
        _ => {
            if hi & 0x8000_0000 != 0 {
                return Ext80::ZERO;
            }
        }
    }
    Ext80::from_i64((((hi as u64) << 32) | lo as u64) as i64)
}

/// PB error 62 "Input past end", raised by the INPUT # routines at end of file.
pub const ERR_INPUT_PAST_END: u8 = 62;

/// Reader over the bytes of a sequential file opened `FOR INPUT`, implementing the PB tokenization
/// of `INPUT #`, `LINE INPUT #` and `EOF()` exactly.
///
/// Rules (byte 0x1A acts as an end-of-file marker everywhere and is never consumed):
/// * Item terminator (0x6b8702), applied after an item ended at a delimiter: skip spaces, TABs and
///   LFs; then a ',' is consumed, or CR plus an optional following LF is consumed; anything else
///   is left for the next item. After a closing quote the quote itself is consumed first.
/// * `INPUT #` string item (0x6b85e3): skip leading spaces (only 0x20). If the item starts with '"',
///   everything up to the next '"' is taken verbatim (commas, CR, LF included; no escaping; end of
///   file ends it). Otherwise bytes up to ',' or CR are taken, TABs and LFs are dropped, other
///   bytes (including spaces and '"') are kept, so trailing spaces are *not* trimmed.
/// * `INPUT #` numeric item (0x6ba3ea): skip leading spaces; collect bytes up to ',', ' ' or CR,
///   dropping TABs and LFs (so "1\t2" reads 12), at most 255 bytes; parse with the VAL parser
///   [`parse_number_ext`] (no further whitespace handling; quotes make the item 0).
/// * `LINE INPUT #` (0x6b84bb): bytes up to CR (exclusive), LF and everything else kept; the CR and
///   one following LF are consumed. A lone LF does not end a line.
/// * At end of file (or 0x1A) before an item starts, all three raise error 62; `LINE INPUT` only if
///   the line would be empty.
pub struct InputReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> InputReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        InputReader { data, pos: 0 }
    }

    /// Byte offset of the next unread byte.
    pub fn position(&self) -> usize {
        self.pos
    }

    /// `EOF(f)` (0x6b7b17): no more bytes, or the next byte is 0x1A.
    pub fn eof(&self) -> bool {
        self.peek().is_none_or(|c| c == 0x1a)
    }

    fn peek(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    /// 0x6b86dc
    fn skip_spaces(&mut self) {
        while self.peek() == Some(b' ') {
            self.pos += 1;
        }
    }

    /// 0x6b8702: consume the terminator of an item; `c` is the byte at the current position.
    fn finish_item(&mut self, c: u8) {
        if c == b'"' {
            self.pos += 1;
        }
        while let Some(c) = self.peek() {
            match c {
                b'\r' => {
                    self.pos += 1;
                    if self.peek() == Some(b'\n') {
                        self.pos += 1;
                    }
                    return;
                }
                b'\n' | b' ' | b'\t' => self.pos += 1,
                b',' => {
                    self.pos += 1;
                    return;
                }
                _ => return,
            }
        }
    }

    /// `INPUT #f, x` for a numeric variable; `Err(62)` at end of file.
    pub fn try_input_num(&mut self) -> Result<f64, u8> {
        self.try_input_num_ext().map(Ext80::to_f64)
    }

    /// [`Self::try_input_num`] with the exact 80-bit result.
    pub fn try_input_num_ext(&mut self) -> Result<Ext80, u8> {
        self.skip_spaces();
        match self.peek() {
            None | Some(0x1a) => return Err(ERR_INPUT_PAST_END),
            _ => {}
        }
        let mut tok: Vec<u8> = Vec::with_capacity(16);
        while let Some(c) = self.peek() {
            if tok.len() == 255 || c == b',' || c == b' ' || c == b'\r' {
                self.finish_item(c);
                break;
            }
            if c == 0x1a {
                break;
            }
            if c != b'\t' && c != b'\n' {
                tok.push(c);
            }
            self.pos += 1;
        }
        Ok(parse_number_ext(&tok))
    }

    /// `INPUT #f, s$`; `Err(62)` at end of file.
    pub fn try_input_str(&mut self) -> Result<Vec<u8>, u8> {
        self.skip_spaces();
        let mut out = Vec::new();
        match self.peek() {
            None | Some(0x1a) => return Err(ERR_INPUT_PAST_END),
            Some(b'"') => {
                self.pos += 1;
                while let Some(c) = self.peek() {
                    if c == b'"' {
                        self.finish_item(c);
                        break;
                    }
                    out.push(c);
                    self.pos += 1;
                }
            }
            Some(_) => {
                while let Some(c) = self.peek() {
                    if c == b',' || c == b'\r' {
                        self.finish_item(c);
                        break;
                    }
                    if c == 0x1a {
                        break;
                    }
                    if c != b'\n' && c != b'\t' {
                        out.push(c);
                    }
                    self.pos += 1;
                }
            }
        }
        Ok(out)
    }

    /// `LINE INPUT #f, s$`; `Err(62)` at end of file.
    pub fn try_line_input(&mut self) -> Result<Vec<u8>, u8> {
        let mut out = Vec::new();
        loop {
            match self.peek() {
                None | Some(0x1a) => {
                    return if out.is_empty() {
                        Err(ERR_INPUT_PAST_END)
                    } else {
                        Ok(out)
                    };
                }
                Some(b'\r') => {
                    self.finish_item(b'\r');
                    return Ok(out);
                }
                Some(c) => {
                    out.push(c);
                    self.pos += 1;
                }
            }
        }
    }

    /// `INPUT #f, x`; 0 at end of file (where the original raises error 62).
    pub fn input_num(&mut self) -> f64 {
        self.try_input_num().unwrap_or(0.0)
    }

    /// `INPUT #f, s$`; empty at end of file (where the original raises error 62).
    pub fn input_str(&mut self) -> Vec<u8> {
        self.try_input_str().unwrap_or_default()
    }

    /// `LINE INPUT #f, s$`; empty at end of file (where the original raises error 62).
    pub fn line_input(&mut self) -> Vec<u8> {
        self.try_line_input().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basics() {
        assert_eq!(str_num(0.1, 15), " .1");
        assert_eq!(str_num(-1234.5, 15), "-1234.5");
        assert_eq!(str_num(1e15, 15), " 1E+15");
        assert_eq!(format_num(1e-20, 15), "1E-20");
        assert_eq!(str_num(0.0, 15), " 0");
        assert_eq!(val(b"1.5e3"), 1500.0);
        assert_eq!(val(b"&HFFFF"), -1.0);
    }
}
