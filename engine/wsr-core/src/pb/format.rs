//! PowerBASIC 8 `FORMAT$(x, mask$)`, bit-exact port of the WSR.EXE runtime.
//!
//! Mirrors `PB_Format` (0x6b9006) and its engine (0x6b9101 .. 0x6b9e5b), including the decimal digit
//! generator at 0x6bf404, which is implemented here with a small software model of the x87 80-bit
//! arithmetic it performs (64-bit mantissa precision, round-to-nearest-even: the runtime's control
//! word is 0x133f, loaded at 0x6b7339). The code below is a deliberately close transliteration of
//! the original (variable names give the original frame offsets) so that odd masks behave exactly
//! like the original; it is verified against vectors produced by running the original routine
//! under CPU emulation (`tools/emu/format.py`, fixture `tests/fixtures/format.txt`).
//!
//! # Semantics (PB 8 FORMAT$ as implemented by this runtime)
//!
//! **Sections.** The mask is split at unquoted, unescaped `;` into up to three sections
//! (positive/zero-or-default; negative; zero). x < 0 uses section 2 with |x| if a non-empty
//! second section exists, else section 1 with x. x == 0 uses section 3 if present and non-empty,
//! else section 1. An empty selected section (or empty mask) falls back to `STR$`-style output
//! (15 significant digits, leading blank of positive numbers removed), see [`str15`].
//!
//! **Digits.** |x| is converted to 18 significant decimal digits (0x6bf404: scale by a power of
//! ten from a table of 80-bit constants, FRNDINT, divide by 10 if >= 1e18, FISTP), padded with
//! zeros. All further rounding is decimal on that digit string with **round half to even**
//! (banker's rounding): a `5` followed only by zeros rounds towards the even neighbour, so
//! `FORMAT$(2.5,"#")` = "2", `FORMAT$(3.5,"#")` = "4", `FORMAT$(0.125,"#.##")` = ".12" (if the f64
//! value is an exact tie; 2.675 is 2.67499999... in binary and gives "2.67").
//!
//! **Placeholders.** `#` = digit, `0` = digit that is always shown (leading/trailing zero),
//! `*c` = two digit positions *and* sets the fill character to `c`; every later `#` then also pads
//! with `c`. So `"* ###,"` is a 5-position field padded with blanks (`FORMAT$(0,"* ###,")` =
//! "    0"), while a mask without `*` is never padded (`"######"` of 12 = "12").
//! Integer digits that do not fit are **not** truncated: the surplus is emitted at the leftmost
//! integer position, so overflow just widens the result (`FORMAT$(123456,"##")` = "123456",
//! `FORMAT$(-1234,"* ###,")` = "-1,234", `FORMAT$(12345678.9,"* ##.00")` = "12345678.90").
//! The integer part is never empty: values below 1 show a `0` (`"#.00"` of 0.5 = "0.50",
//! `"##.##"` of 0.5 = "0.5"). Trailing zero fraction digits under `#` are dropped (`"#.##"` of
//! 1.5 = "1.5", of 0 = "0." -- the point stays), under `0` they are kept.
//!
//! **Comma.** A `,` directly after a digit placeholder (outside the exponent) switches on
//! thousands grouping for the whole integer part; its position is irrelevant (`"###,.00"` and
//! `"#,##.00"` are equivalent) and it does not take a digit position. Any other `,` is a literal.
//!
//! **Sign.** For a negative value formatted with the first section a `-` is placed immediately
//! left of the leftmost digit and takes one fill position (`"* ######,"` of -1234 =
//! "   -1,234"). A result that rounds to zero keeps the minus ("-0.00", `"* ###,.00"` of -0.004 =
//! "   -0.00"); only a bare single "0" drops it (`"#"` of -0.004 = "0").
//!
//! **Other characters.** `.` = decimal point (first one only; later ones are literals). `%`
//! directly after a digit placeholder multiplies by 100 and is shown (`"0%"` of 0.5 = "50%");
//! a `%` elsewhere is dropped. `E-`/`E+`/`e-`/`e+` after a digit placeholder starts an exponent
//! field whose digits use the following `0`/`#` placeholders; the exponent sign occupies one of
//! them and there is no zero padding (`"0.0E+00"` of 1234.5 = "1.2E+3"). `"..."` is a quoted
//! literal, `\c` a literal c, and `- + ( ) $ space` are literals. Every other character
//! (letters, `'`, `/`, a lone `E` ...) is silently **dropped** (`"x0"` = `"0"`).
//!
//! **Degenerate masks.** The original writes its right-to-left output buffer without a lower
//! bound; with `0` placeholders combined with a negative value and odd layouts (e.g.
//! `"0,0%(###* "` of -0.008) the minus lands in the buffer's length dword and the result comes out
//! as NUL bytes. This is reproduced (see `RBuf`); none of the game's masks get there.

/// Length of the digit buffer used by the original (0x1344 = 4932 digits).
const DIGBUF: usize = 0x1344;

use crate::pb::ext80::Ext80;
use crate::pb::numfmt::{E18, scale10};
use std::cmp::Ordering;

/// Decimal digits as produced by 0x6bf404 and post-processed at 0x6b9136..0x6b9171.
struct Digits {
    /// Significant digits, padded with '0' to `DIGBUF` bytes.
    d: Vec<u8>,
    /// Decimal exponent: |x| = 0.d * 10^exp.
    exp: i32,
    neg: bool,
    /// [ebp-0xb8]: some of the first 19 digits are non-zero.
    nonzero: bool,
}

/// 0x6bf404: 18 significant digits of |x| computed in 80-bit arithmetic ([`Ext80`]; the same
/// steps as `numfmt`'s digit generator: scale by `10^(17 - k)`, FRNDINT, divide by 10 if >= 1e18).
fn digits(x: Ext80) -> Digits {
    let e = x.se & 0x7fff;
    let mut d = vec![b'0'; DIGBUF];
    if e == 0 || (e == 0x7fff && x.mant != 1 << 63) {
        // zero (either sign) and NaN: all zeros, exponent 1, positive
        return Digits {
            d,
            exp: 1,
            neg: false,
            nonzero: false,
        };
    }
    if e == 0x7fff {
        // infinity: 0x1344 nines, exponent 999; the caller keeps the first 19
        for c in d.iter_mut().take(19) {
            *c = b'9';
        }
        return Digits {
            d,
            exp: 999,
            neg: x.is_neg(),
            nonzero: true,
        };
    }
    let neg = x.is_neg();
    // imul by 0x4d10, high word = floor(e2 * log10(2)) estimate
    let mut k = ((e as i32 - 0x3fff) * 0x4d10) >> 16;
    let mut s = scale10(x.abs(), 17 - k).frndint();
    if E18.cmp_val(s) != Ordering::Greater {
        s = s.fdiv(Ext80::from_i64(10));
        k += 1;
    }
    let n = s.to_i64();
    let ds = n.to_string();
    d[..ds.len()].copy_from_slice(ds.as_bytes());
    let nonzero = d[..19].iter().any(|&c| c != b'0');
    Digits {
        d,
        exp: k + 1,
        neg,
        nonzero,
    }
}

// ---------------------------------------------------------------------------------------------
// STR$-style fallback used for an empty (selected) mask: 0x6bf28f with 15 digits.
// ---------------------------------------------------------------------------------------------

/// Fallback for an empty mask section (0x6b90d9 -> 0x6bf28f, 15 significant digits): like
/// `STR$(x)` but without the leading blank of non-negative numbers.
///
/// The 18 generated digits are cut to 15 with the original's quirky rounding (only the 16th digit
/// is looked at; a 16th digit of exactly 5 rounds to even), trailing zeros are removed, and the
/// number is written plainly if it fits into 15 digits, otherwise as `d.dddE+x` / `d.dddE-x` (exponent without padding).
pub fn str15(x: f64) -> Vec<u8> {
    str15_ext(Ext80::from_f64(x))
}

/// [`str15`] of an 80-bit value.
pub fn str15_ext(x: Ext80) -> Vec<u8> {
    const N: usize = 15;
    let g = digits(x);
    let mut d: Vec<u8> = g.d[..N + 1].to_vec();
    let mut exp = g.exp;
    // 0x6bf4d3: drop the extra digit, round on it alone (half: to even)
    let last = d.pop().unwrap();
    // (infinity takes a path without this rounding: 15 nines, exponent 999)
    if !x.is_inf() && (last > b'5' || (last == b'5' && d[N - 1] & 1 == 1)) {
        let mut i = N;
        loop {
            i -= 1;
            d[i] += 1;
            if d[i] <= b'9' {
                break;
            }
            d[i] = b'0';
            if i == 0 {
                d[0] = b'1';
                exp += 1;
                break;
            }
        }
    }
    let mut out = Vec::new();
    if g.neg {
        out.push(b'-');
    }
    // significant digits without trailing zeros
    let sig = d.iter().rposition(|&c| c != b'0').map_or(0, |p| p + 1);
    if sig == 0 {
        out.clear();
        out.push(b'0');
        return out;
    }
    let digs = &d[..sig];
    let n = N as i32;
    if exp > 0 && exp <= n {
        let e = exp as usize;
        if sig <= e {
            out.extend_from_slice(digs);
            out.extend(std::iter::repeat_n(b'0', e - sig));
        } else {
            out.extend_from_slice(&digs[..e]);
            out.push(b'.');
            out.extend_from_slice(&digs[e..]);
        }
    } else if exp <= 0 && sig as i32 - exp <= n {
        out.push(b'.');
        out.extend(std::iter::repeat_n(b'0', (-exp) as usize));
        out.extend_from_slice(digs);
    } else {
        out.push(digs[0]);
        if sig > 1 {
            out.push(b'.');
            out.extend_from_slice(&digs[1..]);
        }
        let e = exp - 1;
        out.push(b'E');
        out.push(if e < 0 { b'-' } else { b'+' });
        out.extend_from_slice(e.unsigned_abs().to_string().as_bytes());
    }
    out
}

// ---------------------------------------------------------------------------------------------
// The formatting engine (0x6b9101).
// ---------------------------------------------------------------------------------------------

/// Bytes modelled in front of a right-to-left output buffer: its length dword plus some of the
/// memory before it (contents unknown in the original; modelled as zeros).
const PRE: usize = 64;

/// A runtime string buffer filled right to left ([ebp-0x30] / [ebp-0x24]). The original writes
/// without a lower bound check, so a few degenerate masks (`0` placeholders combined with a
/// negative value, e.g. `"0,0%(###* "`) write in front of the buffer, into its length dword.
/// The final compaction (`rep movsb` forward from the first written byte after storing the new
/// length) is reproduced byte by byte, which is why such results come out as NUL bytes.
#[derive(Default)]
struct RBuf {
    mem: Vec<u8>,
}

impl RBuf {
    fn new(n: usize) -> RBuf {
        let mut mem = vec![0u8; PRE + n];
        mem[PRE - 4..PRE].copy_from_slice(&(n as u32).to_le_bytes());
        mem[PRE..].fill(b' ');
        RBuf { mem }
    }

    fn put(&mut self, i: isize, c: u8) {
        let j = PRE as isize + i;
        if j >= 0 {
            self.mem[j as usize] = c;
        }
    }

    /// Compaction (0x6b9416 / 0x6b97ae): `last` is the final write index (one below the
    /// leftmost written byte).
    fn finish(mut self, last: isize) -> Vec<u8> {
        let cap = self.mem.len() - PRE;
        let used = last + 1;
        let n = (cap as isize - used).max(0) as usize;
        self.mem[PRE - 4..PRE].copy_from_slice(&(n as u32).to_le_bytes());
        let src = PRE as isize + used;
        let mut out = Vec::with_capacity(n);
        for k in 0..n {
            // forward byte copy; source and destination may overlap
            let s = src + k as isize;
            let c = if s >= 0 {
                self.mem.get(s as usize).copied().unwrap_or(0)
            } else {
                0
            };
            if let Some(d) = self.mem.get_mut(PRE + k) {
                *d = c;
            }
            out.push(c);
        }
        out
    }
}

/// Which buffer the digit source pointer [ebp-0xa0] points into.
#[derive(Clone, Copy, PartialEq, Default)]
enum Src {
    /// [ebp-0x14]: rounded digit string (integer part + fraction).
    #[default]
    All,
    /// [ebp-0x2c]: "-".
    Minus,
    /// [ebp-0x28]: zeros for '0' placeholders.
    Zeros,
}

#[derive(Default)]
struct Fmt {
    // mask template buffers ([ebp-4], [ebp-8], [ebp-0xc])
    t11: Vec<u8>,
    t10: Vec<u8>,
    t9: Vec<u8>,
    out: RBuf, // [ebp-0x30]
    result: Vec<u8>,
    minus: Vec<u8>, // [ebp-0x2c]
    zeros: Vec<u8>, // [ebp-0x28]
    eout: Vec<u8>,  // [ebp-0x24]
    frac: Vec<u8>,  // [ebp-0x20]
    int: Vec<u8>,   // [ebp-0x1c]
    edig: Vec<u8>,  // [ebp-0x18]
    all: Vec<u8>,   // [ebp-0x14]
    d: Vec<u8>,     // [ebp-0x10]
    v34_commas: i32,
    v38_dot: i32,
    v3c_pct: i32,
    v40_int: i32,
    v44_frac: i32,
    v48_exp: i32,
    v4c_zeros: i32,
    v50_exp10: i32,
    v54_neg: i32,
    v58_emode: i32,
    v5c_epos: i32,
    v60_esign: i32,
    v6c_cnt: i32,
    v70_fill: i32,
    v74_lastdig: i32,
    v98_grp: i32,
    v9c_trail: i32,
    va4_left: i32,
    vb0: i32,
    vb4_ncomma: i32,
    vb8_nonzero: i32,
    // pointers (indices)
    i11: isize,
    i10: isize,
    i9: isize,
    va0: (Src, isize),
    va8: isize,
}

/// Increment a decimal string (0x6b9a0d). Returns the carry out; an empty string carries.
fn incr(b: &mut [u8]) -> bool {
    for c in b.iter_mut().rev() {
        if *c + 1 > b'9' {
            *c = b'0';
        } else {
            *c += 1;
            return false;
        }
    }
    true
}

/// Rounding decision at `pos` (digit `b[pos]` is the first one dropped): > 5 up, < 5 down, 5 with
/// any later non-zero digit up, exact tie: up if `prev` (the last kept digit) is odd.
fn round_up(b: &[u8], pos: usize, prev: Option<u8>) -> bool {
    let c = b[pos];
    if c > b'5' {
        return true;
    }
    if c < b'5' {
        return false;
    }
    if b[pos + 1..].iter().any(|&c| c != b'0') {
        return true;
    }
    prev.is_some_and(|p| p & 1 == 1)
}

impl Fmt {
    fn rd_src(&self) -> u8 {
        let (s, i) = self.va0;
        let b = match s {
            Src::All => &self.all,
            Src::Minus => &self.minus,
            Src::Zeros => &self.zeros,
        };
        if i < 0 {
            // bytes of the length dword in front of the string data
            (b.len() as u32).to_le_bytes()[(4 + i).clamp(0, 3) as usize]
        } else {
            b.get(i as usize).copied().unwrap_or(0)
        }
    }

    fn put(&mut self, c: u8) {
        self.out.put(self.va8, c);
        self.va8 -= 1;
    }

    fn t(b: &[u8], i: isize) -> u8 {
        if i < 0 {
            0
        } else {
            b.get(i as usize).copied().unwrap_or(0)
        }
    }

    // ---- mask parsing (0x6b9a34) ----

    fn digit(&mut self) {
        if self.v58_emode != 0 {
            self.v48_exp += 1;
        } else if self.v38_dot != 0 {
            self.v44_frac += 1;
        } else {
            self.v40_int += 1;
        }
    }

    fn lit(&mut self, c: u8) {
        self.t11.push(c);
        self.t10.push(b' ');
        self.t9.push(b' ');
    }

    fn parse(&mut self, mask: &[u8], start: usize, end: usize) {
        let at = |p: usize| mask.get(p).copied().unwrap_or(0);
        let mut p = start;
        let mut rem = end as isize - start as isize;
        self.v70_fill = -1;
        while rem > 0 {
            let c = at(p);
            match c {
                b'#' => {
                    self.digit();
                    self.t11.push(b'&');
                    if self.v70_fill > 0 {
                        self.t10.push(b'@');
                        self.t9.push(self.v70_fill as u8);
                    } else {
                        self.t10.push(b'&');
                        self.t9.push(b'&');
                    }
                    rem -= 1;
                    p += 1;
                    self.v74_lastdig = 1;
                }
                b'.' => {
                    if self.v58_emode == 0 {
                        self.v38_dot = 1;
                    }
                    self.lit(c);
                    rem -= 1;
                    p += 1;
                    self.v74_lastdig = 0;
                }
                b',' => {
                    if self.v74_lastdig != 0 && self.v58_emode == 0 {
                        self.v34_commas += 1;
                        if self.vb0 != 0 {
                            self.zero_ph(c);
                            rem -= 1;
                            p += 1;
                            continue;
                        }
                    } else {
                        self.lit(c);
                    }
                    rem -= 1;
                    p += 1;
                    self.v74_lastdig = 0;
                }
                b'%' => {
                    if self.v74_lastdig != 0 {
                        self.v3c_pct = 1;
                        self.lit(c);
                    }
                    rem -= 1;
                    p += 1;
                    self.v74_lastdig = 0;
                }
                b'0' => {
                    self.zero_ph(c);
                    rem -= 1;
                    p += 1;
                }
                b'e' | b'E' => {
                    if self.v58_emode == 0 && (self.v40_int | self.v44_frac) != 0 {
                        self.lit(c);
                        rem -= 1;
                        p += 1;
                        match at(p) {
                            b'-' => self.v58_emode = -1,
                            b'+' => self.v58_emode = 1,
                            _ => {
                                self.v74_lastdig = 0;
                                continue;
                            }
                        }
                        self.v5c_epos = self.t11.len() as i32;
                    }
                    rem -= 1;
                    p += 1;
                    self.v74_lastdig = 0;
                }
                b'*' => {
                    rem -= 1;
                    p += 1;
                    if rem == 0 {
                        self.lit(c);
                        self.v74_lastdig = 0;
                    } else {
                        self.digit();
                        self.digit();
                        let f = at(p);
                        self.v70_fill = f as i32;
                        self.t11.extend_from_slice(b"@@");
                        self.t10.extend_from_slice(b"@@");
                        self.t9.extend_from_slice(&[f, f]);
                        rem -= 1;
                        p += 1;
                        self.v74_lastdig = 1;
                    }
                }
                b'"' => {
                    p += 1;
                    rem -= 1;
                    while rem > 0 {
                        if at(p) == b'"' {
                            p += 1;
                            rem -= 1;
                            break;
                        }
                        self.lit(at(p));
                        p += 1;
                        rem -= 1;
                    }
                    self.v74_lastdig = 0;
                }
                b'\\' => {
                    rem -= 1;
                    if rem != 0 {
                        p += 1;
                        self.lit(at(p));
                        rem -= 1;
                    }
                    p += 1;
                    self.v74_lastdig = 0;
                }
                b'-' | b'+' | b'(' | b')' | b'$' | b' ' => {
                    self.lit(c);
                    rem -= 1;
                    p += 1;
                    self.v74_lastdig = 0;
                }
                _ => {
                    rem -= 1;
                    p += 1;
                    self.v74_lastdig = 0;
                }
            }
        }
        if self.v58_emode != 0 && self.v48_exp == 0 {
            self.v58_emode = 0;
        }
    }

    /// 0x6b9cba: '0' placeholder (or, unreachably, a ',' when [ebp-0xb0] is set).
    /// Does not advance the mask pointer for the '0' case (the caller does).
    fn zero_ph(&mut self, c: u8) {
        self.digit();
        self.t11.push(b'@');
        self.t10.push(b'@');
        if c == b'0' {
            if self.v48_exp == 0 {
                self.v4c_zeros += 1;
            }
            self.t9.push(b'0');
        } else {
            self.vb0 = 1;
            self.t9.push(b' ');
        }
        self.v74_lastdig = 1;
    }

    // ---- digit split and rounding (0x6b97d1) ----

    fn split(&mut self) {
        if self.v50_exp10 < 0 {
            let z = (-self.v50_exp10) as usize;
            let n = (self.d.len() as isize + self.v50_exp10 as isize).max(0) as usize;
            let mut f = vec![b'0'; z];
            f.extend_from_slice(&self.d);
            f.truncate(n);
            self.frac = f;
            self.int.clear();
        } else {
            let mut vbc = self.v50_exp10;
            if self.v58_emode != 0 && vbc == 0 {
                vbc = 1;
            }
            let vbc = vbc as usize;
            let mut int = self.d[..vbc.min(self.d.len())].to_vec();
            int.resize(vbc, b'0');
            self.int = int;
            self.frac = self.d.get(vbc..).map_or(Vec::new(), |s| s.to_vec());
        }
        if self.v58_emode == 0 {
            let v44 = self.v44_frac as usize;
            if self.frac.len() > v44 {
                let prev = if v44 >= 1 {
                    Some(self.frac[v44 - 1])
                } else {
                    self.int.last().copied()
                };
                let up = round_up(&self.frac, v44, prev);
                self.frac.truncate(v44);
                if up && incr(&mut self.frac) && incr(&mut self.int) {
                    self.int.insert(0, b'1');
                }
            }
        }
        if self.int.is_empty() {
            self.int.push(b'0');
        }
        self.all = self.int.clone();
        self.all.extend_from_slice(&self.frac);
        if self.v58_emode != 0 {
            let pos = (self.v40_int + self.v44_frac) as usize;
            if self.all.len() > pos {
                let prev = if pos >= 1 {
                    Some(self.all[pos - 1])
                } else {
                    Some(0)
                };
                let up = round_up(&self.all, pos, prev);
                self.all.truncate(pos);
                if up && incr(&mut self.all) {
                    let n = self.all.len();
                    if n != 0 {
                        self.all.copy_within(0..n - 1, 1);
                        self.all[0] = b'1';
                    }
                    self.v50_exp10 += self.v60_esign;
                }
            }
        }
        if self.all.len() <= 1 && self.all.first().copied().unwrap_or(0) == b'0' {
            self.v54_neg = 0;
        }
    }

    // ---- mantissa formatting (0x6b9315) ----

    fn comma(&mut self) {
        if self.v34_commas == 0 {
            return;
        }
        self.v98_grp += 1;
        if self.v98_grp <= 2 {
            return;
        }
        if self.va4_left != 0 || self.v4c_zeros != 0 {
            if self.va8 <= 0 {
                return;
            }
            self.put(b',');
            self.vb4_ncomma += 1;
        }
        self.v98_grp = 0;
    }

    /// 0x6b9439: emit the next digit for a placeholder.
    fn emit(&mut self) {
        let z = (Fmt::t(&self.t9, self.i9) == b'0') as i32;
        self.v4c_zeros -= z;
        if self.v4c_zeros < 0 {
            self.v4c_zeros += 1;
        }
        if self.v9c_trail == 0 {
            let c = self.rd_src();
            self.put(c);
        } else if Fmt::t(&self.t10, self.i10) == b'@' {
            let c = Fmt::t(&self.t9, self.i9);
            self.put(c);
        }
        self.va0.1 -= 1;
        self.va4_left -= 1;
        if self.v44_frac != 0 {
            let mut eax = 0;
            self.v44_frac -= 1;
            if self.v44_frac != 0 && self.v9c_trail != 0 {
                eax = (self.rd_src() == b'0') as i32;
            }
            self.v9c_trail = eax;
            return;
        }
        if self.v40_int != 0 {
            self.v40_int -= 1;
            self.comma();
            if self.v40_int == 0 {
                while self.va4_left != 0 {
                    let c = self.rd_src();
                    self.put(c);
                    self.va0.1 -= 1;
                    self.va4_left -= 1;
                    self.comma();
                }
                if self.v54_neg != 0 {
                    self.put(b'-');
                    self.v54_neg = 0;
                }
                return;
            }
        }
        if self.va4_left == 0 {
            if self.v4c_zeros != 0 {
                self.va4_left = self.v4c_zeros;
                self.va0 = (Src::Zeros, self.v4c_zeros as isize - 1);
            } else if self.v54_neg != 0 {
                self.va0 = (Src::Minus, 0);
                self.va4_left += 1;
                self.v54_neg = 0;
            } else if self.vb0 != 0 {
                let mut ecx = self.vb4_ncomma;
                self.vb0 = 0;
                let mut ebx = self.i10 - 1;
                if ecx > self.v34_commas {
                    ecx = self.v34_commas;
                }
                while ecx != 0 {
                    if ebx >= 0 && Fmt::t(&self.t10, ebx) == b'@' {
                        self.t10[ebx as usize] = b'&';
                    }
                    ebx -= 1;
                    ecx -= 1;
                }
            }
        }
    }

    /// 0x6b95bc: a '@' position (zero / fill) with no digits left.
    fn fill(&mut self) {
        let c = Fmt::t(&self.t9, self.i9);
        self.put(c);
        if self.v44_frac != 0 {
            self.v44_frac -= 1;
            return;
        }
        if self.v40_int != 0 {
            self.v40_int -= 1;
            self.v9c_trail = 0;
            if Fmt::t(&self.t9, self.i9) == b'0' {
                self.va4_left = self.v40_int;
                self.comma();
                self.va4_left = 0;
            }
        }
        if self.va4_left == 0 && self.v40_int == 0 && self.v54_neg != 0 {
            self.put(b'-');
            self.v54_neg = 0;
        }
    }

    fn mantissa(&mut self) {
        self.va4_left = self.all.len() as i32;
        self.va0 = (Src::All, self.va4_left as isize - 1);
        let vac = (self.v6c_cnt + 2 * self.va4_left).max(0) as usize;
        self.out = RBuf::new(vac);
        self.va8 = vac as isize - 1;
        if self.v54_neg != 0 {
            self.minus = vec![b'-'];
        }
        if self.v4c_zeros != 0 {
            self.zeros = vec![b'0'; self.v4c_zeros as usize];
        }
        self.v9c_trail = 0;
        if self.v44_frac != 0 {
            self.v9c_trail = (self.rd_src() == b'0') as i32;
        }
        while self.v6c_cnt != 0 {
            let a = Fmt::t(&self.t10, self.i10);
            if a != b'&' && a != b'@' {
                let c = Fmt::t(&self.t11, self.i11);
                self.put(c);
            } else if self.va4_left != 0 {
                self.emit();
            } else if self.v54_neg != 0 {
                self.va0 = (Src::Minus, 0);
                self.va4_left += 1;
                self.v54_neg = 0;
                self.emit();
            } else if a == b'@' {
                self.fill();
            }
            self.i11 -= 1;
            self.i10 -= 1;
            self.i9 -= 1;
            self.v6c_cnt -= 1;
        }
        self.result = std::mem::take(&mut self.out).finish(self.va8);
    }

    /// 0x6b968a: exponent field.
    fn exponent(&mut self) {
        let mut v7c = self.edig.len() as isize;
        let mut v78 = v7c - 1;
        let mut cnt = self.t11.len();
        let mut i11 = cnt as isize - 1;
        let mut i9 = self.t9.len() as isize - 1;
        let mut i10 = self.t10.len() as isize - 1;
        let n = cnt + 2 * v7c as usize;
        let mut o = RBuf::new(n);
        let mut v90 = n as isize - 1;
        let put = |o: &mut RBuf, v90: &mut isize, c: u8| {
            o.put(*v90, c);
            *v90 -= 1;
        };
        let ed = |i: isize| if i < 0 { 0 } else { self.edig[i as usize] };
        while cnt != 0 {
            let a = Fmt::t(&self.t10, i10);
            if a != b'&' && a != b'@' {
                put(&mut o, &mut v90, Fmt::t(&self.t11, i11));
            } else if v7c == 0 {
                if a == b'@' {
                    put(&mut o, &mut v90, Fmt::t(&self.t9, i9));
                    if self.v48_exp != 0 {
                        self.v48_exp -= 1;
                    }
                }
            } else {
                put(&mut o, &mut v90, ed(v78));
                v78 -= 1;
                v7c -= 1;
                if self.v48_exp != 0 {
                    self.v48_exp -= 1;
                    if self.v48_exp == 0 && v7c != 0 {
                        while v7c != 0 {
                            put(&mut o, &mut v90, ed(v78));
                            v78 -= 1;
                            v7c -= 1;
                        }
                    }
                }
            }
            i10 -= 1;
            i9 -= 1;
            i11 -= 1;
            cnt -= 1;
        }
        self.eout = o.finish(v90);
    }

    fn run(x: Ext80, mask: &[u8], start: usize, end: usize) -> Vec<u8> {
        let mut f = Fmt::default();
        let g = digits(x);
        f.d = g.d;
        f.v50_exp10 = g.exp;
        f.v54_neg = -(g.neg as i32);
        f.vb8_nonzero = g.nonzero as i32;
        f.parse(mask, start, end);
        if f.v44_frac > 0 && f.v50_exp10 >= 0 {
            let n = f.d.len() + f.v44_frac as usize;
            f.d.resize(n, b'0');
        }
        if f.v3c_pct != 0 {
            f.v50_exp10 += 2 * f.vb8_nonzero;
        }
        if f.v58_emode != 0 {
            if f.v50_exp10 >= 0 {
                f.v60_esign = 1;
                f.v50_exp10 -= f.v40_int;
                if f.v50_exp10 < 0 {
                    f.v50_exp10 = -f.v50_exp10;
                    f.v60_esign = -1;
                }
            } else {
                f.v60_esign = -1;
                f.v50_exp10 = f.v40_int - f.v50_exp10;
            }
        }
        f.split();
        if f.v58_emode == 0 {
            f.v6c_cnt = f.t11.len() as i32;
            f.i11 = f.t11.len() as isize - 1;
            f.i9 = f.t9.len() as isize - 1;
            f.i10 = f.t10.len() as isize - 1;
            f.mantissa();
            return f.result;
        }
        f.v6c_cnt = f.v5c_epos;
        let last = f.v5c_epos as isize - 1;
        f.i11 = last;
        f.i9 = last;
        f.i10 = last;
        f.mantissa();
        // exponent digits with sign (0x6b9221)
        let mut e = f.v50_exp10.to_string().into_bytes();
        if f.v60_esign < 0 {
            e.insert(0, b'-');
        } else if f.v58_emode > 0 {
            e.insert(0, b'+');
        }
        f.edig = e;
        let k = f.v5c_epos as usize;
        for t in [&mut f.t10, &mut f.t9, &mut f.t11] {
            if k < t.len() {
                t.drain(..k);
            } else {
                t.clear();
            }
        }
        f.exponent();
        let mut out = f.result;
        out.extend_from_slice(&f.eout);
        out
    }
}

/// `FORMAT$(x, mask$)` (PB_Format, 0x6b9006). See the module documentation for the semantics.
pub fn format(x: f64, mask: &[u8]) -> Vec<u8> {
    format_ext(Ext80::from_f64(x), mask)
}

/// `FORMAT$(x, mask$)` of an 80-bit value (the original's argument is always the EXT on the x87
/// stack; [`format`] is this with an f64 widened exactly).
pub fn format_ext(x: Ext80, mask: &[u8]) -> Vec<u8> {
    // split into sections at unquoted/unescaped ';' (0x6b9012..0x6b905d)
    let mut seps: Vec<usize> = Vec::with_capacity(3);
    let len = mask.len();
    let mut i = 0;
    while i < len && seps.len() < 3 {
        match mask[i] {
            b'"' => {
                i += 1;
                while i < len && mask[i] != b'"' {
                    i += 1;
                }
                i += 1;
            }
            b'\\' => i += 2,
            b';' => {
                seps.push(i);
                i += 1;
            }
            _ => i += 1,
        }
    }
    let i = i.min(len);
    while seps.len() < 3 {
        seps.push(i);
    }
    let (p1, p2, p3) = (seps[0], seps[1], seps[2]);
    let (mut x, mut start, mut end) = (x, 0usize, p1);
    // ftst: NaN compares "below" and takes the negative branch
    if x.is_nan() || x.cmp_val(Ext80::ZERO) == Ordering::Less {
        if p1 != p2 {
            if p1 + 1 == p3 {
                // quirk (0x6b907f): the pointer was already advanced past the ';', so section 1
                // is used *including* that ';' (which the parser then drops)
                end = p1 + 1;
            } else {
                start = p1 + 1;
                end = p2;
                x = x.chs();
            }
        }
    } else if x.is_zero() && p2 != p3 && p2 + 1 != p3 {
        start = p2 + 1;
        end = p3;
    }
    if end <= start {
        return str15_ext(x);
    }
    Fmt::run(x, mask, start, end)
}
