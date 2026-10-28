//! Software model of the x87 80-bit extended format (PB `EXT`), as the WSR.EXE runtime uses it.
//!
//! The PB runtime sets the FPU control word to 0x133f (0x6b7339): all exceptions masked, 64-bit
//! precision, round to nearest even. Every intermediate of a BASIC float expression lives on the x87
//! stack in that format; the port maps `EXT` to `f64` in general (see [`crate::pb::types::Ext`]), but
//! where the extra 11 bits decide a result (a value that is `STR$`-ed with 15 digits, a rounding tie of
//! `FISTP`, a CURRENCY/CURRENCYX store) the routines use [`Ext80`] instead.
//!
//! Implemented (each one correctly rounded to 64 bits, nearest even, like the x87 with the control
//! word above; verified bit-for-bit against the x87 emulation of Unicorn by `tools/emu/ext80.py`,
//! fixture `tests/fixtures/ext80.txt`):
//!
//! | x87                       | Rust                                     |
//! |---------------------------|------------------------------------------|
//! | FLD qword / dword         | [`Ext80::from_f64`] / [`Ext80::from_f32`] |
//! | FILD                      | [`Ext80::from_i64`]                      |
//! | FSTP qword / dword        | [`Ext80::to_f64`] / [`Ext80::to_f32`]    |
//! | FISTP qword / dword       | [`Ext80::to_i64`] / [`Ext80::to_i32`]    |
//! | FADD / FSUB               | [`Ext80::fadd`] / [`Ext80::fsub`] (`+`, `-`) |
//! | FMUL / FDIV               | [`Ext80::fmul`] / [`Ext80::fdiv`] (`*`, `/`) |
//! | FRNDINT                   | [`Ext80::frndint`]                       |
//! | FCHS / FABS               | [`Ext80::chs`] (unary `-`) / [`Ext80::abs`] |
//! | FCOM                      | [`Ext80::cmp_val`]                       |
//!
//! Zeros, denormals (gradual underflow) and infinities follow the x87 rules (masked responses:
//! overflow gives infinity, an invalid operation gives the "real indefinite" `-NaN` 0xffff_c000…).
//! NaN operands are not modelled beyond being passed through (the game never computes with NaNs), nor
//! are pseudo-denormals (exponent 0 with the integer bit set; the x87 never produces them).
//!
//! `PartialEq`/`Eq` compare the raw bits (`+0 != -0`); use [`Ext80::cmp_val`] for numeric comparison.

use std::cmp::Ordering;

/// An x87 80-bit extended value as raw bits: `se` = sign bit (0x8000) | 15-bit biased exponent,
/// `mant` = 64-bit mantissa with explicit integer bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Ext80 {
    pub se: u16,
    pub mant: u64,
}

const BIAS: i32 = 16383;

impl Ext80 {
    /// Lift a simulation value without a DOUBLE spill.
    pub fn from_ext(value: impl Into<crate::pb::real::Ext>) -> Self { value.into().raw() }
    pub fn into_ext(self) -> crate::pb::real::Ext { crate::pb::real::Ext(self) }
    pub const ZERO: Ext80 = Ext80 { se: 0, mant: 0 };
    pub const ONE: Ext80 = Ext80 {
        se: 0x3fff,
        mant: 1 << 63,
    };
    /// +infinity.
    pub const INF: Ext80 = Ext80 {
        se: 0x7fff,
        mant: 1 << 63,
    };
    /// The x87 "real indefinite" (result of an invalid operation with exceptions masked).
    pub const INDEFINITE: Ext80 = Ext80 {
        se: 0xffff,
        mant: 0xc000_0000_0000_0000,
    };

    /// Exact conversion (FLD qword).
    pub const fn from_f64(x: f64) -> Ext80 {
        Ext80::from_ieee(x.to_bits(), 52, 11)
    }

    /// Exact conversion (FLD dword).
    pub const fn from_f32(x: f32) -> Ext80 {
        Ext80::from_ieee(x.to_bits() as u64, 23, 8)
    }

    /// Exact conversion of an IEEE binary format with `mbits` fraction bits and `ebits` exponent bits.
    const fn from_ieee(b: u64, mbits: u32, ebits: u32) -> Ext80 {
        let sign = ((b >> (mbits + ebits)) as u16 & 1) << 15;
        let emax = (1i32 << ebits) - 1;
        let ebias = (1i32 << (ebits - 1)) - 1;
        let e = ((b >> mbits) & emax as u64) as i32;
        let m = b & ((1 << mbits) - 1);
        if e == 0 && m == 0 {
            return Ext80 { se: sign, mant: 0 };
        }
        if e == emax {
            return Ext80 {
                se: sign | 0x7fff,
                mant: (1 << 63) | (m << (63 - mbits)),
            };
        }
        let (m, e) = if e == 0 {
            let sh = (mbits + 1) as i32 - (64 - m.leading_zeros()) as i32;
            (m << sh, 1 - sh)
        } else {
            (m | (1 << mbits), e)
        };
        Ext80 {
            se: sign | (e - ebias + BIAS) as u16,
            mant: m << (63 - mbits),
        }
    }

    /// Conversion to f64 with round to nearest even (FSTP qword with all exceptions masked):
    /// overflow gives infinity, tiny values become f64 subnormals or zero.
    pub fn to_f64(self) -> f64 {
        f64::from_bits(self.to_ieee(52, 11))
    }

    /// Conversion to f32 with round to nearest even (FSTP dword), as [`Ext80::to_f64`].
    pub fn to_f32(self) -> f32 {
        f32::from_bits(self.to_ieee(23, 8) as u32)
    }

    /// Rounds to an IEEE binary format with `mbits` fraction bits and `ebits` exponent bits.
    fn to_ieee(self, mbits: u32, ebits: u32) -> u64 {
        let emax_field = (1u64 << ebits) - 1;
        let ebias = (1i32 << (ebits - 1)) - 1;
        let sign = (self.is_neg() as u64) << (mbits + ebits);
        let inf = sign | (emax_field << mbits);
        let e = (self.se & 0x7fff) as i32;
        if e == 0x7fff {
            let nan = self.mant << 1 != 0;
            return inf
                | if nan {
                    (1 << (mbits - 1)) | ((self.mant << 1) >> (64 - mbits))
                } else {
                    0
                };
        }
        if self.mant == 0 {
            return sign;
        }
        // value = m * 2^(ue - 63), m in [2^63, 2^64)
        let (eff, m) = self.unpack();
        let mut ue = eff - BIAS;
        let emin = 1 - ebias;
        let base = 63 - mbits as i32;
        let shift = if ue >= emin { base } else { base + (emin - ue) };
        if shift >= 64 + 2 {
            return sign;
        }
        let shift = shift as u32;
        let (mut q, rem, half) = if shift >= 64 {
            (0u64, m as u128, 1u128 << (shift - 1))
        } else {
            (
                m >> shift,
                (m as u128) & ((1u128 << shift) - 1),
                1u128 << (shift - 1),
            )
        };
        if rem > half || (rem == half && q & 1 == 1) {
            q += 1;
        }
        if ue >= emin {
            if q == 1 << (mbits + 1) {
                q >>= 1;
                ue += 1;
            }
            if ue > ebias {
                return inf;
            }
            sign | (((ue + ebias) as u64) << mbits) | (q & ((1 << mbits) - 1))
        } else {
            // subnormal (q may have rounded up to the smallest normal, which the encoding handles)
            sign | q
        }
    }

    /// Exact conversion of an integer (FILD).
    pub const fn from_i64(v: i64) -> Ext80 {
        if v == 0 {
            return Ext80::ZERO;
        }
        let sign = if v < 0 { 0x8000 } else { 0 };
        let m = v.unsigned_abs();
        let lz = m.leading_zeros();
        Ext80 {
            se: sign | (BIAS + 63 - lz as i32) as u16,
            mant: m << lz,
        }
    }

    /// Sign bit set (also for -0).
    pub fn is_neg(self) -> bool {
        self.se & 0x8000 != 0
    }
    /// Infinity or NaN.
    pub fn is_inf(self) -> bool {
        self.se & 0x7fff == 0x7fff
    }
    pub fn is_nan(self) -> bool {
        self.is_inf() && self.mant << 1 != 0
    }
    /// +0 or -0.
    pub fn is_zero(self) -> bool {
        self.se & 0x7fff != 0x7fff && self.mant == 0
    }
    /// FCHS.
    pub fn chs(self) -> Ext80 {
        Ext80 {
            se: self.se ^ 0x8000,
            mant: self.mant,
        }
    }
    /// FABS.
    pub fn abs(self) -> Ext80 {
        Ext80 {
            se: self.se & 0x7fff,
            mant: self.mant,
        }
    }
    /// (effective biased exponent, normalized mantissa); value = m * 2^(e - BIAS - 63). Nonzero finite only.
    fn unpack(self) -> (i32, u64) {
        let e = (self.se & 0x7fff) as i32;
        let e = if e == 0 { 1 } else { e };
        let lz = self.mant.leading_zeros() as i32;
        (e - lz, self.mant << lz)
    }

    /// Round `sig * 2^(e - BIAS - 127)` to extended precision (nearest even), with gradual underflow
    /// and overflow to infinity.
    fn round_pack(neg: bool, mut e: i32, mut sig: u128) -> Ext80 {
        let sign = if neg { 0x8000 } else { 0 };
        if sig == 0 {
            return Ext80 { se: sign, mant: 0 };
        }
        let lz = sig.leading_zeros() as i32;
        sig <<= lz;
        e -= lz;
        if e <= 0 {
            let sh = (1 - e) as u32;
            sig = if sh >= 128 {
                1
            } else {
                (sig >> sh) | ((sig & ((1u128 << sh) - 1) != 0) as u128)
            };
            e = 0;
        }
        let mut m = (sig >> 64) as u64;
        let rem = sig as u64;
        if rem > 1 << 63 || (rem == 1 << 63 && m & 1 == 1) {
            m = m.wrapping_add(1);
            if m == 0 {
                m = 1 << 63;
                e += 1;
            } else if e == 0 && m >> 63 == 1 {
                e = 1;
            }
        }
        if e >= 0x7fff {
            return Ext80 {
                se: sign | 0x7fff,
                mant: 1 << 63,
            };
        }
        Ext80 {
            se: sign | e as u16,
            mant: m,
        }
    }

    /// Exact binary exponent adjustment followed by the original storage rounding.
    pub fn scalbn(self, exponent: i32) -> Self {
        if self.is_inf() || self.is_zero() { return self; }
        let (e, mantissa) = self.unpack();
        Self::round_pack(self.is_neg(), e + exponent.clamp(-40000,40000), (mantissa as u128) << 64)
    }

    /// Complete FPREM remainder, including exponent gaps too large for a rounded quotient.
    pub fn frem(self, other: Self) -> Self {
        if let Some(nan) = Self::nan_result(self, other) { return nan; }
        if self.is_inf() || other.is_zero() { return Self::INDEFINITE; }
        if other.is_inf() || self.is_zero() { return self; }
        let (left_exponent, left_mantissa) = self.unpack();
        let (right_exponent, right_mantissa) = other.unpack();
        if left_exponent < right_exponent { return self; }
        let mut remainder = left_mantissa % right_mantissa;
        let mut distance = left_exponent - right_exponent;
        while distance > 0 {
            let shift = distance.min(64);
            remainder = (((remainder as u128) << shift) % right_mantissa as u128) as u64;
            distance -= shift;
        }
        Self::round_pack(self.is_neg(), right_exponent, (remainder as u128) << 64)
    }

    /// The NaN result of an operation with a NaN operand (first NaN operand, quieted).
    fn nan_result(a: Ext80, b: Ext80) -> Option<Ext80> {
        let q = |x: Ext80| Ext80 {
            se: x.se,
            mant: x.mant | (1 << 62),
        };
        if a.is_nan() {
            Some(q(a))
        } else if b.is_nan() {
            Some(q(b))
        } else {
            None
        }
    }

    /// FADD (round to nearest even, 64-bit precision). `x + (-x)` and `(+0) + (-0)` give +0.
    pub fn fadd(self, o: Ext80) -> Ext80 {
        if let Some(n) = Ext80::nan_result(self, o) {
            return n;
        }
        if self.is_inf() || o.is_inf() {
            return if self.is_inf() && o.is_inf() && self.is_neg() != o.is_neg() {
                Ext80::INDEFINITE
            } else if self.is_inf() {
                self
            } else {
                o
            };
        }
        if o.is_zero() {
            return if self.is_zero() && self.is_neg() != o.is_neg() {
                Ext80::ZERO
            } else {
                self
            };
        }
        if self.is_zero() {
            return o;
        }
        let (mut ea, mut ma) = self.unpack();
        let (mut eb, mut mb) = o.unpack();
        let (mut neg_a, mut neg_b) = (self.is_neg(), o.is_neg());
        if (eb, mb) > (ea, ma) {
            std::mem::swap(&mut ea, &mut eb);
            std::mem::swap(&mut ma, &mut mb);
            std::mem::swap(&mut neg_a, &mut neg_b);
        }
        let big = (ma as u128) << 62;
        let sh = (ea - eb) as u32;
        let small = if sh >= 126 {
            1
        } else {
            let x = (mb as u128) << 62;
            (x >> sh) | ((x & ((1u128 << sh) - 1) != 0) as u128)
        };
        if neg_a != neg_b && big == small {
            return Ext80::ZERO;
        }
        let sig = if neg_a == neg_b {
            big + small
        } else {
            big - small
        };
        // sig * 2^(ea - BIAS - 63 - 62) = sig * 2^((ea + 2) - BIAS - 127)
        Ext80::round_pack(neg_a, ea + 2, sig)
    }

    /// FSUB `self - o`.
    pub fn fsub(self, o: Ext80) -> Ext80 {
        if o.is_nan() {
            return self.fadd(o);
        }
        self.fadd(o.chs())
    }

    /// FMUL (round to nearest even, 64-bit precision).
    pub fn fmul(self, o: Ext80) -> Ext80 {
        if let Some(n) = Ext80::nan_result(self, o) {
            return n;
        }
        let neg = self.is_neg() != o.is_neg();
        if self.is_inf() || o.is_inf() {
            return if self.is_zero() || o.is_zero() {
                Ext80::INDEFINITE
            } else {
                Ext80 {
                    se: Ext80::INF.se | if neg { 0x8000 } else { 0 },
                    mant: 1 << 63,
                }
            };
        }
        if self.is_zero() || o.is_zero() {
            return Ext80 {
                se: if neg { 0x8000 } else { 0 },
                mant: 0,
            };
        }
        let (ea, ma) = self.unpack();
        let (eb, mb) = o.unpack();
        Ext80::round_pack(neg, ea + eb - BIAS + 1, ma as u128 * mb as u128)
    }

    /// FDIV `self / o` (round to nearest even, 64-bit precision). `x / 0` gives a signed infinity,
    /// `0 / 0` and `inf / inf` the indefinite.
    pub fn fdiv(self, o: Ext80) -> Ext80 {
        if let Some(n) = Ext80::nan_result(self, o) {
            return n;
        }
        let neg = self.is_neg() != o.is_neg();
        let sign = if neg { 0x8000 } else { 0 };
        if (self.is_inf() && o.is_inf()) || (self.is_zero() && o.is_zero()) {
            return Ext80::INDEFINITE;
        }
        if self.is_inf() || o.is_zero() {
            return Ext80 {
                se: sign | 0x7fff,
                mant: 1 << 63,
            };
        }
        if o.is_inf() || self.is_zero() {
            return Ext80 { se: sign, mant: 0 };
        }
        let (ea, ma) = self.unpack();
        let (eb, mb) = o.unpack();
        let n = (ma as u128) << 64;
        let q1 = n / mb as u128;
        let r1 = n % mb as u128;
        let n2 = r1 << 62;
        let q2 = n2 / mb as u128;
        let r2 = n2 % mb as u128;
        let sig = (q1 << 62) | q2 | (r2 != 0) as u128;
        // sig ~ ma/mb * 2^126
        Ext80::round_pack(neg, ea - eb + BIAS + 1, sig)
    }

    /// Round to an integer magnitude (nearest even) - shared by FRNDINT and FISTP.
    fn round_int_mag(self) -> u128 {
        if self.is_zero() {
            return 0;
        }
        let (e, m) = self.unpack();
        let ue = e - BIAS; // value = m * 2^(ue-63)
        if ue >= 63 {
            return if ue - 63 >= 64 {
                u128::MAX
            } else {
                (m as u128) << (ue - 63)
            };
        }
        let sh = (63 - ue) as u32;
        if sh > 64 {
            return 0;
        }
        let v = (m as u128) << 64 >> sh; // value * 2^64
        let int = v >> 64;
        let frac = v as u64;
        if frac > 1 << 63 || (frac == 1 << 63 && int & 1 == 1) {
            int + 1
        } else {
            int
        }
    }

    /// FRNDINT (nearest even; keeps the sign of zero results).
    pub fn frndint(self) -> Ext80 {
        if self.is_inf() {
            return self;
        }
        let m = self.round_int_mag();
        if m >> 64 != 0 {
            return self; // already integral (|x| >= 2^63)
        }
        Ext80::round_pack(self.is_neg(), BIAS + 127, m)
    }

    /// FISTP qword (nearest even). Out-of-range values, infinities and NaNs give the integer
    /// indefinite `i64::MIN`.
    pub fn to_i64(self) -> i64 {
        let m = self.round_int_mag();
        if self.is_inf() || m > i64::MAX as u128 + self.is_neg() as u128 {
            return i64::MIN;
        }
        if self.is_neg() {
            (m as i64).wrapping_neg()
        } else {
            m as i64
        }
    }

    /// FISTP dword (nearest even). Out-of-range values give the integer indefinite `i32::MIN`.
    pub fn to_i32(self) -> i32 {
        let m = self.round_int_mag();
        if self.is_inf() || m > i32::MAX as u128 + self.is_neg() as u128 {
            return i32::MIN;
        }
        if self.is_neg() {
            (m as i32).wrapping_neg()
        } else {
            m as i32
        }
    }

    /// Numeric order (FCOM) of non-NaN values; `+0 == -0`.
    pub fn cmp_val(self, o: Ext80) -> Ordering {
        let key = |x: Ext80| -> (i8, i32, u64) {
            if x.is_zero() {
                return (0, 0, 0);
            }
            let (e, m) = if x.is_inf() {
                (0x7fff, 1 << 63)
            } else {
                x.unpack()
            };
            (if x.is_neg() { -1 } else { 1 }, e, m)
        };
        let (sa, ea, ma) = key(self);
        let (sb, eb, mb) = key(o);
        if sa != sb {
            return sa.cmp(&sb);
        }
        let c = (ea, ma).cmp(&(eb, mb));
        if sa < 0 { c.reverse() } else { c }
    }

    /// The 10 bytes as stored in memory (little endian: mantissa, then sign/exponent).
    pub fn to_le_bytes(self) -> [u8; 10] {
        let mut b = [0u8; 10];
        b[..8].copy_from_slice(&self.mant.to_le_bytes());
        b[8..].copy_from_slice(&self.se.to_le_bytes());
        b
    }

    /// See [`Ext80::to_le_bytes`].
    pub fn from_le_bytes(b: [u8; 10]) -> Ext80 {
        Ext80 {
            se: u16::from_le_bytes([b[8], b[9]]),
            mant: u64::from_le_bytes(b[..8].try_into().unwrap()),
        }
    }
}

impl From<f64> for Ext80 {
    fn from(x: f64) -> Ext80 {
        Ext80::from_f64(x)
    }
}

impl From<f32> for Ext80 {
    fn from(x: f32) -> Ext80 {
        Ext80::from_f32(x)
    }
}

impl From<i64> for Ext80 {
    fn from(x: i64) -> Ext80 {
        Ext80::from_i64(x)
    }
}

impl From<i32> for Ext80 {
    fn from(x: i32) -> Ext80 {
        Ext80::from_i64(x as i64)
    }
}

impl std::ops::Add for Ext80 {
    type Output = Ext80;
    fn add(self, o: Ext80) -> Ext80 {
        self.fadd(o)
    }
}

impl std::ops::Sub for Ext80 {
    type Output = Ext80;
    fn sub(self, o: Ext80) -> Ext80 {
        self.fsub(o)
    }
}

impl std::ops::Mul for Ext80 {
    type Output = Ext80;
    fn mul(self, o: Ext80) -> Ext80 {
        self.fmul(o)
    }
}

impl std::ops::Div for Ext80 {
    type Output = Ext80;
    fn div(self, o: Ext80) -> Ext80 {
        self.fdiv(o)
    }
}

impl std::ops::Neg for Ext80 {
    type Output = Ext80;
    fn neg(self) -> Ext80 {
        self.chs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(x: f64) -> Ext80 {
        Ext80::from_f64(x)
    }

    #[test]
    fn add_sub() {
        assert_eq!((e(1.5) + e(2.25)).to_f64(), 3.75);
        assert_eq!((e(1.0) - e(1.0)), Ext80::ZERO);
        assert_eq!(e(-0.0) + e(0.0), Ext80::ZERO);
        assert_eq!(e(-0.0) + e(-0.0), e(-0.0));
        assert_eq!((e(-3.0) + e(1.0)).to_f64(), -2.0);
        // 1 + 2^-64 rounds to even (1), 1 + 3*2^-65 rounds up
        let tiny = Ext80 {
            se: 0x3fff - 64,
            mant: 1 << 63,
        };
        assert_eq!(Ext80::ONE + tiny, Ext80::ONE);
        let t3 = Ext80 {
            se: 0x3fff - 64,
            mant: 3 << 62,
        };
        assert_eq!(
            Ext80::ONE + t3,
            Ext80 {
                se: 0x3fff,
                mant: (1 << 63) | 1
            }
        );
        // 80-bit keeps what f64 loses
        let x = e(1e16) + e(1.0);
        assert_eq!(x.to_i64(), 10_000_000_000_000_001);
        assert_eq!((Ext80::INF + Ext80::INF.chs()), Ext80::INDEFINITE);
    }

    #[test]
    fn mul_div_round() {
        assert_eq!((e(3.0) * e(-0.5)).to_f64(), -1.5);
        assert_eq!((e(1.0) / e(3.0)).to_f64(), 1.0 / 3.0);
        assert_eq!(e(2.5).to_i64(), 2);
        assert_eq!(e(3.5).to_i64(), 4);
        assert_eq!(e(-2.5).to_i32(), -2);
        assert_eq!(e(3e9).to_i32(), i32::MIN);
        assert_eq!(e(-2.5).frndint().to_f64(), -2.0);
        // x * 100 is exact in 80-bit (53 + 7 bits); in f64 0.015 * 100 rounds up to the tie 1.5
        assert_eq!((e(0.015) * Ext80::from_i64(100)).to_i64(), 1);
        assert_eq!((0.015f64 * 100.0).round_ties_even(), 2.0);
    }

    #[test]
    fn conversions() {
        for x in [0.1f64, -1e300, 5e-324, 1.0, f64::MAX, f64::MIN_POSITIVE] {
            assert_eq!(e(x).to_f64().to_bits(), x.to_bits());
        }
        for x in [0.1f32, -3.4e38, 1e-45, 7.0] {
            assert_eq!(Ext80::from_f32(x).to_f32().to_bits(), x.to_bits());
            assert_eq!(Ext80::from_f32(x).to_f64(), x as f64);
        }
        assert_eq!(e(0.1).to_f32(), 0.1f32);
        assert_eq!(e(1e300).to_f32(), f32::INFINITY);
        let x = e(-123.456);
        assert_eq!(Ext80::from_le_bytes(x.to_le_bytes()), x);
        assert_eq!(e(1.0).cmp_val(e(-0.0)), Ordering::Greater);
        assert_eq!(e(0.0).cmp_val(e(-0.0)), Ordering::Equal);
    }
}
