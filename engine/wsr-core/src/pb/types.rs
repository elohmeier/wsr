//! PowerBASIC scalar types as used by WSR.EXE, mapped to Rust.
//!
//! `EXT` uses software x87 extended storage and arithmetic. SINGLE and DOUBLE retain
//! their original storage widths; CURRENCY stores exact scaled integers.

use crate::pb::ext80::Ext80;

/// PB `EXT`: numeric comparisons and software x87 extended arithmetic.
pub use crate::pb::real::Ext;
/// PB `DOUBLE`.
pub type Double = f64;
/// PB `SINGLE`.
pub type Single = f32;
/// PB `QUAD` (64-bit integer).
pub type Quad = i64;
/// PB `LONG` (32-bit integer).
pub type Long = i32;
/// PB `INTEGER` (16-bit integer).
pub type Integer = i16;

/// PB `CURRENCY`: a 64-bit integer scaled by 10,000 (helpers `PB_CurLoad`/`PB_CurStore` at
/// 0x6bcc93/0x6bcca5 divide/multiply by 10000.0 on the x87 stack).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cur(pub i64);

impl Cur {
    pub const SCALE: f64 = 10_000.0;
    /// Value as a float (`fild qword; fdiv 10000.0`).
    pub fn get(self) -> Ext {
        Ext(self.get_ext())
    }
    /// Store a float (`fmul 10000.0; fistp qword`, round-half-even like the x87 default mode). Out-of-range,
    /// infinite and NaN values give the x87 "integer indefinite" `i64::MIN` (like [`to_quad`]), not a
    /// saturated `i64::MAX` / 0 as Rust's `as` would.
    ///
    /// The product is formed in 80 bits like the original (`x * 10000` of a DOUBLE is not always exact
    /// in f64, e.g. a value just below x.xxxx5 must not round up to the tie).
    pub fn set(&mut self, x: impl Into<Ext>) {
        self.set_ext(x.into().raw());
    }
    /// Value as an 80-bit intermediate (`fild qword; fdiv 10000.0`).
    pub fn get_ext(self) -> Ext80 {
        Ext80::from_i64(self.0).fdiv(Ext80::from_i64(10_000))
    }
    /// Store an 80-bit intermediate (`fmul 10000.0; fistp qword`).
    pub fn set_ext(&mut self, x: Ext80) {
        self.0 = x.fmul(Ext80::from_i64(10_000)).to_i64();
    }
    pub fn from_f64(x: impl Into<Ext>) -> Self {
        let mut c = Cur(0);
        c.set(x);
        c
    }
}

/// PB `CURRENCYX`: 64-bit integer scaled by 100 (0x6bcc9c/0x6bccae).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cux(pub i64);

impl Cux {
    pub fn get(self) -> Ext {
        Ext(self.get_ext())
    }
    /// `fmul 100.0; fistp qword` (product in 80 bits, see [`Cur::set`]); out of range gives
    /// `i64::MIN` like [`Cur::set`].
    pub fn set(&mut self, x: impl Into<Ext>) {
        self.set_ext(x.into().raw());
    }
    /// Value as an 80-bit intermediate (`fild qword; fdiv 100.0`, 0x6bcc9c).
    pub fn get_ext(self) -> Ext80 {
        Ext80::from_i64(self.0).fdiv(Ext80::from_i64(100))
    }
    /// Store an 80-bit intermediate (`fmul 100.0; fistp qword`, 0x6bccae).
    pub fn set_ext(&mut self, x: Ext80) {
        self.0 = x.fmul(Ext80::from_i64(100)).to_i64();
    }
}

/// x87 `fistp`/`frndint` in the default rounding mode (round to nearest, ties to even). Every assignment of a
/// float expression to an integer variable in PB goes through this (shown as `ROUND()` in the decompiled view).
pub fn round_half_even(x: f64) -> f64 {
    x.round_ties_even()
}

/// Assignment of a float to a `LONG` (`fistp dword`). Out-of-range values give the x87 "integer
/// indefinite" 0x80000000 like the original.
pub fn to_long(x: f64) -> Long {
    let r = round_half_even(x);
    if r.is_nan() || r < i32::MIN as f64 || r > i32::MAX as f64 { i32::MIN } else { r as i32 }
}

/// Assignment of a float to an `INTEGER` (`fistp word`); out of range gives 0x8000.
pub fn to_integer(x: f64) -> Integer {
    let r = round_half_even(x);
    if r.is_nan() || r < i16::MIN as f64 || r > i16::MAX as f64 { i16::MIN } else { r as i16 }
}

/// Assignment of a float to a `QUAD` (`fistp qword`).
pub fn to_quad(x: f64) -> Quad {
    let r = round_half_even(x);
    if r.is_nan() || r < i64::MIN as f64 || r >= i64::MAX as f64 { i64::MIN } else { r as i64 }
}

/// PB `INT(x)`: floor.
pub fn int(x: f64) -> f64 {
    x.floor()
}

/// PB `FIX(x)`: truncate toward zero.
pub fn fix(x: f64) -> f64 {
    x.trunc()
}

/// PB `CEIL(x)`.
pub fn ceil(x: f64) -> f64 {
    x.ceil()
}

/// PB `FRAC(x)` = x - FIX(x).
pub fn frac(x: f64) -> f64 {
    x - x.trunc()
}

/// PB `SQR(x)` (0x6bcbb4): negative input is returned unchanged, without an error.
pub fn sqr(x: f64) -> f64 {
    if x < 0.0 { x } else { x.sqrt() }
}

/// Boolean result of a PB comparison: -1 for true, 0 for false.
pub fn pb_bool(b: bool) -> Long {
    if b { -1 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_is_half_even() {
        assert_eq!(to_long(0.5), 0);
        assert_eq!(to_long(1.5), 2);
        assert_eq!(to_long(2.5), 2);
        assert_eq!(to_long(-2.5), -2);
        assert_eq!(to_long(-3.5), -4);
        assert_eq!(to_long(2.4999), 2);
    }

    #[test]
    fn currency_round_trips() {
        // 123.45675 is 123.4567499999.. as a DOUBLE; x87 `fmul 10000` keeps that below the tie (f64 would
        // round the product to 1234567.5 and then to 1234568)
        assert_eq!(Cur::from_f64(123.45675).0, 1_234_567);
        assert_eq!(Cur::from_f64(123.45685).0, 1_234_569);
        let mut x = Cux::default();
        x.set(0.015);
        assert_eq!(x.0, 1);
        assert_eq!(Cux(2203).get_ext().fmul(crate::pb::ext80::Ext80::from_i64(100)).to_i64(), 2203);
        assert_eq!(Cur(1_234_500).get().to_f64(), 123.45);
        assert_eq!(Cur(1_234_500).get().raw(), Cur(1_234_500).get_ext());
    }

    #[test]
    fn currency_overflow_is_integer_indefinite() {
        assert_eq!(Cur::from_f64(f64::INFINITY).0, i64::MIN);
        assert_eq!(Cur::from_f64(f64::NEG_INFINITY).0, i64::MIN);
        assert_eq!(Cur::from_f64(f64::NAN).0, i64::MIN);
        assert_eq!(Cur::from_f64(1e15).0, i64::MIN);
        assert_eq!(Cur::from_f64(-9e14).0, -9_000_000_000_000_000_000);
    }
}
