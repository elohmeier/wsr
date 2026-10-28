//! Simulation numbers: x87 extended storage and arithmetic, with numeric comparison semantics.
//! `Ext80` remains the raw-bit utility used by runtime vector tests and exact snapshots.
use super::ext80::Ext80;
use std::{cmp::Ordering, fmt, ops};

#[repr(transparent)]
#[derive(Clone, Copy, Default)]
pub struct Ext(pub Ext80);

impl Ext {
    pub const ZERO: Self = Self(Ext80::ZERO);
    pub const ONE: Self = Self(Ext80::ONE);
    pub const NAN: Self = Self(Ext80::INDEFINITE);
    pub const INFINITY: Self = Self(Ext80::INF);
    pub const NEG_INFINITY: Self = Self(Ext80 {
        se: 0xffff,
        mant: 1 << 63,
    });
    pub const EPSILON: Self = Self(Ext80 {
        se: 16383 - 63,
        mant: 1 << 63,
    });
    pub const MIN_POSITIVE: Self = Self(Ext80 {
        se: 1,
        mant: 1 << 63,
    });
    pub const MAX: Self = Self(Ext80 {
        se: 0x7ffe,
        mant: u64::MAX,
    });
    pub const MIN: Self = Self(Ext80 {
        se: 0xfffe,
        mant: u64::MAX,
    });
    pub const fn from_f64(value: f64) -> Self {
        Self(Ext80::from_f64(value))
    }
    pub const fn from_f32(value: f32) -> Self {
        Self(Ext80::from_f32(value))
    }
    pub const fn from_i64(value: i64) -> Self {
        Self(Ext80::from_i64(value))
    }
    pub const fn from_u64(value: u64) -> Self {
        if value == 0 {
            return Self::ZERO;
        }
        let shift = value.leading_zeros();
        Self(Ext80 {
            se: (16383 + 63 - shift) as u16,
            mant: value << shift,
        })
    }
    pub const fn raw(self) -> Ext80 {
        self.0
    }
    pub fn to_f64(self) -> f64 {
        self.0.to_f64()
    }
    pub fn to_f32(self) -> f32 {
        self.0.to_f32()
    }
    pub fn is_nan(self) -> bool {
        self.0.is_nan()
    }
    pub fn is_finite(self) -> bool {
        !self.0.is_inf()
    }
    pub fn is_infinite(self) -> bool {
        self.0.is_inf() && !self.0.is_nan()
    }
    pub fn is_sign_negative(self) -> bool {
        self.0.is_neg()
    }
    pub fn total_cmp(&self, other: &Self) -> Ordering {
        if self.0.is_neg() != other.0.is_neg() {
            return if self.0.is_neg() {
                Ordering::Less
            } else {
                Ordering::Greater
            };
        }
        let order = (self.0.se & 0x7fff, self.0.mant).cmp(&(other.0.se & 0x7fff, other.0.mant));
        if self.0.is_neg() {
            order.reverse()
        } else {
            order
        }
    }
    pub fn signum(self) -> Self {
        if self.is_nan() {
            Self::NAN
        } else if self.is_sign_negative() {
            -Self::ONE
        } else {
            Self::ONE
        }
    }
    pub fn abs(self) -> Self {
        Self(self.0.abs())
    }
    pub fn round_ties_even(self) -> Self {
        Self(self.0.frndint())
    }
    pub fn trunc(self) -> Self {
        if !self.is_finite() || self.0.is_zero() {
            return self;
        }
        let exponent = (self.0.se & 0x7fff) as i32 - 16383;
        if exponent < 0 {
            return Self(Ext80 {
                se: self.0.se & 0x8000,
                mant: 0,
            });
        }
        if exponent >= 63 {
            return self;
        }
        Self(Ext80 {
            se: self.0.se,
            mant: self.0.mant & (u64::MAX << (63 - exponent)),
        })
    }
    pub fn floor(self) -> Self {
        let integer = self.trunc();
        if self < integer {
            integer - Self::ONE
        } else {
            integer
        }
    }
    pub fn ceil(self) -> Self {
        let integer = self.trunc();
        if self > integer {
            integer + Self::ONE
        } else {
            integer
        }
    }
    pub fn fract(self) -> Self {
        self - self.trunc()
    }
    pub fn round(self) -> Self {
        let integer = self.trunc();
        if (self - integer).abs() >= Self::from_f64(0.5) {
            integer + self.signum()
        } else {
            integer
        }
    }
    pub fn min(self, other: Self) -> Self {
        if other.is_nan() || self < other {
            self
        } else {
            other
        }
    }
    pub fn max(self, other: Self) -> Self {
        if other.is_nan() || self > other {
            self
        } else {
            other
        }
    }
    pub fn clamp(self, min: Self, max: Self) -> Self {
        assert!(min <= max);
        if self < min {
            min
        } else if self > max {
            max
        } else {
            self
        }
    }
    /// FSQRT: exact integer square root followed by a single nearest-even 64-bit rounding.
    pub fn sqrt(self) -> Self {
        if self.is_nan() {
            return self + Self::ZERO;
        }
        if self.0.is_zero() {
            return self;
        }
        if self.is_sign_negative() {
            return Self::NAN;
        }
        if self.is_infinite() {
            return self;
        }
        let shift = self.0.mant.leading_zeros();
        let mantissa = self.0.mant << shift;
        let exponent = (self.0.se & 0x7fff).max(1) as i32 - 16383 - shift as i32;
        let radicand = (mantissa as u128) << (63 + (exponent & 1));
        let root = radicand.isqrt();
        let remainder = radicand - root * root;
        let rounded = root + u128::from(remainder > root);
        let mut result_exponent = (exponent >> 1) + 16383;
        let result_mantissa = if rounded > u64::MAX as u128 {
            result_exponent += 1;
            1 << 63
        } else {
            rounded as u64
        };
        Self(Ext80 {
            se: result_exponent as u16,
            mant: result_mantissa,
        })
    }
    pub fn powi(self, exponent: i32) -> Self {
        self.powf(Self::from_i64(exponent as i64))
    }
    pub fn powf(self, exponent: Self) -> Self {
        // PB rounds the exponent for a negative base. Small integer powers use
        // left-to-right squaring, preserving the runtime's intermediate rounding.
        let exponent = if self < Self::ZERO {
            exponent.round_ties_even()
        } else {
            exponent
        };
        if exponent == Self::ZERO {
            return Self::ONE;
        }
        if self.is_nan() || exponent.is_nan() {
            return Self::NAN;
        }
        if self == Self::ONE {
            return self;
        }
        if exponent == Self::ONE {
            return self;
        }
        if self.0.is_zero() {
            return if exponent < Self::ZERO {
                Self::INFINITY
            } else {
                self
            };
        }
        let integer = exponent.trunc();
        if exponent == integer && exponent.abs() < Self::from_i64(65536) {
            let n = cast::<u64, _>(exponent.abs());
            let mut result = self;
            for bit in (0..63 - n.leading_zeros()).rev() {
                result *= result;
                if n & (1 << bit) != 0 {
                    result *= self;
                }
            }
            return if exponent < Self::ZERO {
                Self::ONE / result
            } else {
                result
            };
        }
        let magnitude = self.abs();
        let log = magnitude.log2();
        let result = (exponent * log).exp2();
        if self < Self::ZERO && integer % Self::from_i64(2) != Self::ZERO {
            -result
        } else {
            result
        }
    }
    // Range-reduced atanh and exponential series. All steps, including scale
    // reduction, use EXT; no conversion to a host Double occurs in the kernel.
    fn log2(self) -> Self {
        if self.is_infinite() {
            return self;
        }
        let shift = self.0.mant.leading_zeros();
        let exponent = (self.0.se & 0x7fff).max(1) as i32 - 16383 - shift as i32;
        let mut m = Self(Ext80 {
            se: 16383,
            mant: self.0.mant << shift,
        });
        let mut exponent = exponent;
        if m > Self(Ext80 {
            se: 16383,
            mant: 0xb504f333f9de6484,
        }) {
            m = Self(m.0.scalbn(-1));
            exponent += 1;
        }
        let z = (m - Self::ONE) / (m + Self::ONE);
        let square = z * z;
        let mut term = z;
        let mut sum = z;
        for denominator in (3..=101).step_by(2) {
            term *= square;
            let next = sum + term / Self::from_i64(denominator);
            if next == sum {
                break;
            }
            sum = next;
        }
        Self::from_i64(exponent as i64) + (sum + sum) / Self::LN_2
    }
    const LN_2: Self = Self(Ext80 {
        se: 0x3ffe,
        mant: 0xb17217f7d1cf79ac,
    });
    fn exp2(self) -> Self {
        if self.is_nan() {
            return Self::NAN;
        }
        if self >= Self::from_i64(16384) {
            return Self::INFINITY;
        }
        if self <= Self::from_i64(-16446) {
            return Self::ZERO;
        }
        let integer = self.round_ties_even();
        let fraction = (self - integer) * Self::LN_2;
        let mut term = Self::ONE;
        let mut sum = Self::ONE;
        for denominator in 1..=40 {
            term = term * fraction / Self::from_i64(denominator);
            let next = sum + term;
            if next == sum {
                break;
            }
            sum = next;
        }
        Self(sum.0.scalbn(cast::<i32, _>(integer)))
    }
    pub fn from_bits(bits: u64) -> Self {
        Self::from_f64(f64::from_bits(bits))
    }
    pub fn to_bits(self) -> u64 {
        self.to_f64().to_bits()
    }
    /// Magnitude for Rust-style truncating/saturating integer casts. FISTP uses the runtime helpers.
    fn integer_parts(self) -> (bool, u128) {
        if self.is_nan() {
            return (false, 0);
        }
        let exponent = (self.0.se & 0x7fff) as i32 - 16383;
        let magnitude = if exponent < 0 {
            0
        } else if exponent > 127 {
            u128::MAX
        } else if exponent >= 63 {
            (self.0.mant as u128) << (exponent - 63)
        } else {
            (self.0.mant >> (63 - exponent)) as u128
        };
        (self.is_sign_negative(), magnitude)
    }
}

impl From<Ext80> for Ext {
    fn from(value: Ext80) -> Self {
        Self(value)
    }
}
impl From<f64> for Ext {
    fn from(value: f64) -> Self {
        Self::from_f64(value)
    }
}
impl From<f32> for Ext {
    fn from(value: f32) -> Self {
        Self::from_f32(value)
    }
}
impl From<u128> for Ext {
    fn from(value: u128) -> Self {
        if value <= u64::MAX as u128 {
            return Self::from_u64(value as u64);
        }
        let exponent = 127 - value.leading_zeros();
        let shift = exponent - 63;
        let mut mantissa = value >> shift;
        let remainder = value & ((1u128 << shift) - 1);
        let half = 1u128 << (shift - 1);
        if remainder > half || remainder == half && mantissa & 1 != 0 {
            mantissa += 1;
        }
        if mantissa > u64::MAX as u128 {
            Self(Ext80 {
                se: (16383 + exponent + 1) as u16,
                mant: 1 << 63,
            })
        } else {
            Self(Ext80 {
                se: (16383 + exponent) as u16,
                mant: mantissa as u64,
            })
        }
    }
}
impl From<i128> for Ext {
    fn from(value: i128) -> Self {
        let result = Self::from(value.unsigned_abs());
        if value < 0 { -result } else { result }
    }
}
macro_rules! integer_from {
    ($ctor:ident; $($t:ty),*) => {$(impl From<$t> for Ext {
        fn from(value: $t) -> Self { Self::$ctor(value as _) }
    })*};
}
integer_from!(from_i64; i8, i16, i32, i64, isize);
integer_from!(from_u64; u8, u16, u32, u64, usize);
pub fn ext(value: impl Into<Ext>) -> Ext {
    value.into()
}

impl PartialEq for Ext {
    fn eq(&self, other: &Self) -> bool {
        !self.is_nan() && !other.is_nan() && self.0.cmp_val(other.0) == Ordering::Equal
    }
}
impl PartialOrd for Ext {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.is_nan() || other.is_nan() {
            None
        } else {
            Some(self.0.cmp_val(other.0))
        }
    }
}
macro_rules! arithmetic {
    ($trait:ident, $method:ident, $raw:ident, $assign:ident, $assign_method:ident) => {
        impl ops::$trait for Ext {
            type Output = Self;
            fn $method(self, rhs: Self) -> Self {
                Self(self.0.$raw(rhs.0))
            }
        }
        impl ops::$assign for Ext {
            fn $assign_method(&mut self, rhs: Self) {
                *self = ops::$trait::$method(*self, rhs);
            }
        }
    };
}
arithmetic!(Add, add, fadd, AddAssign, add_assign);
arithmetic!(Sub, sub, fsub, SubAssign, sub_assign);
arithmetic!(Mul, mul, fmul, MulAssign, mul_assign);
arithmetic!(Div, div, fdiv, DivAssign, div_assign);
impl ops::Neg for Ext {
    type Output = Self;
    fn neg(self) -> Self {
        Self(self.0.chs())
    }
}
impl ops::Rem for Ext {
    type Output = Self;
    fn rem(self, rhs: Self) -> Self {
        Self(self.0.frem(rhs.0))
    }
}
macro_rules! mixed {
    ($($t:ty),*) => {$(
        impl PartialEq<$t> for Ext { fn eq(&self, rhs: &$t) -> bool { *self == Ext::from(*rhs) } }
        impl PartialEq<Ext> for $t { fn eq(&self, rhs: &Ext) -> bool { Ext::from(*self) == *rhs } }
        impl PartialOrd<$t> for Ext { fn partial_cmp(&self, rhs: &$t) -> Option<Ordering> { self.partial_cmp(&Ext::from(*rhs)) } }
        impl PartialOrd<Ext> for $t { fn partial_cmp(&self, rhs: &Ext) -> Option<Ordering> { Ext::from(*self).partial_cmp(rhs) } }
        impl ops::Add<$t> for Ext { type Output = Ext; fn add(self, rhs: $t) -> Ext { self + Ext::from(rhs) } }
        impl ops::Sub<$t> for Ext { type Output = Ext; fn sub(self, rhs: $t) -> Ext { self - Ext::from(rhs) } }
        impl ops::Mul<$t> for Ext { type Output = Ext; fn mul(self, rhs: $t) -> Ext { self * Ext::from(rhs) } }
        impl ops::Div<$t> for Ext { type Output = Ext; fn div(self, rhs: $t) -> Ext { self / Ext::from(rhs) } }
        impl ops::Add<Ext> for $t { type Output = Ext; fn add(self, rhs: Ext) -> Ext { Ext::from(self) + rhs } }
        impl ops::Sub<Ext> for $t { type Output = Ext; fn sub(self, rhs: Ext) -> Ext { Ext::from(self) - rhs } }
        impl ops::Mul<Ext> for $t { type Output = Ext; fn mul(self, rhs: Ext) -> Ext { Ext::from(self) * rhs } }
        impl ops::Div<Ext> for $t { type Output = Ext; fn div(self, rhs: Ext) -> Ext { Ext::from(self) / rhs } }
    )*};
}
mixed!(f32, f64);
impl std::iter::Sum for Ext {
    fn sum<I: Iterator<Item = Self>>(items: I) -> Self {
        items.fold(Self::ZERO, |a, b| a + b)
    }
}

// Formatting is a presentation boundary. Lossless engine snapshots use raw x87 bytes instead.
impl fmt::Display for Ext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.to_f64(), f)
    }
}
impl fmt::Debug for Ext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.to_f64(), f)
    }
}
impl fmt::LowerExp for Ext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::LowerExp::fmt(&self.to_f64(), f)
    }
}
impl std::str::FromStr for Ext {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let parsed = value.parse::<f64>().map_err(|e| e.to_string())?;
        let word = value.trim_start_matches(['+', '-']);
        if parsed.is_nan() || word.eq_ignore_ascii_case("inf") || word.eq_ignore_ascii_case("infinity") {
            return Ok(Self::from_f64(parsed));
        }
        Ok(Self(super::numfmt::val_ext(value.as_bytes())))
    }
}

/// Explicit primitive stores/casts, including the original's SINGLE and DOUBLE spill boundaries.
pub trait Cast<T> {
    fn cast(self) -> T;
}
pub fn cast<T, V: Cast<T>>(value: V) -> T {
    value.cast()
}
macro_rules! primitive_casts {
    ($source:ty; $($target:ty),*) => {$(impl Cast<$target> for $source {
        fn cast(self) -> $target { self as $target }
    })*};
}
macro_rules! all_primitive_casts {
    ($($source:ty),*) => {$(primitive_casts!($source; u8,u16,u32,u64,u128,usize,i8,i16,i32,i64,i128,isize,f32,f64);)*};
}
all_primitive_casts!(
    u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize, f32, f64
);
impl<T: Into<Ext>> Cast<Ext> for T {
    fn cast(self) -> Ext {
        self.into()
    }
}
impl Cast<f32> for Ext {
    fn cast(self) -> f32 {
        self.to_f32()
    }
}
impl Cast<f64> for Ext {
    fn cast(self) -> f64 {
        self.to_f64()
    }
}
macro_rules! ext_signed_casts {
    ($($t:ty),*) => {$(impl Cast<$t> for Ext {
        fn cast(self) -> $t {
            let (negative, magnitude) = self.integer_parts();
            if negative {
                if magnitude >= (<$t>::MAX as u128) + 1 { <$t>::MIN } else { -(magnitude as $t) }
            } else { magnitude.min(<$t>::MAX as u128) as $t }
        }
    })*};
}
macro_rules! ext_unsigned_casts {
    ($($t:ty),*) => {$(impl Cast<$t> for Ext {
        fn cast(self) -> $t {
            let (negative, magnitude) = self.integer_parts();
            if negative { 0 } else { magnitude.min(<$t>::MAX as u128) as $t }
        }
    })*};
}
ext_signed_casts!(i8, i16, i32, i64, i128, isize);
ext_unsigned_casts!(u8, u16, u32, u64, u128, usize);
impl Cast<usize> for bool {
    fn cast(self) -> usize {
        self as usize
    }
}
impl Cast<i32> for bool {
    fn cast(self) -> i32 {
        self as i32
    }
}
impl Cast<u32> for char {
    fn cast(self) -> u32 {
        self as u32
    }
}
