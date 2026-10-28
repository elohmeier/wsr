//! Extended-precision runtime entry points used by simulation routines.
pub use super::format::{format_ext, str15_ext};
pub use super::numfmt::*;
pub use super::real::{Ext, cast, ext};
pub use super::types::{Cur, Cux, Double, Integer, Long, Quad, Single, pb_bool};
pub fn val(bytes: &[u8]) -> Ext {
    Ext(super::numfmt::val_ext(bytes))
}
/// `FSTP qword; FLD qword`: retain an intentional DOUBLE local/store boundary.
pub fn double(value: impl Into<Ext>) -> Ext {
    Ext::from_f64(value.into().to_f64())
}
pub fn str_num(value: impl Into<Ext>, digits: u32) -> String {
    super::numfmt::str_num_ext(value.into().raw(), digits)
}
pub fn format_num(value: impl Into<Ext>, digits: u32) -> String {
    super::numfmt::format_num_ext(value.into().raw(), digits)
}
pub fn write_num(value: impl Into<Ext>) -> String {
    format_num(value, 15)
}
pub fn format(value: impl Into<Ext>, mask: &[u8]) -> Vec<u8> {
    super::format::format_ext(value.into().raw(), mask)
}
pub fn str15(value: impl Into<Ext>) -> Vec<u8> {
    super::format::str15_ext(value.into().raw())
}
pub fn round_half_even(value: impl Into<Ext>) -> Ext {
    value.into().round_ties_even()
}
pub fn to_quad(value: impl Into<Ext>) -> i64 {
    value.into().raw().to_i64()
}
pub fn to_long(value: impl Into<Ext>) -> i32 {
    value.into().raw().to_i32()
}
pub fn to_integer(value: impl Into<Ext>) -> i16 {
    i16::try_from(to_quad(value)).unwrap_or(i16::MIN)
}
pub fn int(value: impl Into<Ext>) -> Ext {
    value.into().floor()
}
pub fn fix(value: impl Into<Ext>) -> Ext {
    value.into().trunc()
}
pub fn ceil(value: impl Into<Ext>) -> Ext {
    value.into().ceil()
}
pub fn frac(value: impl Into<Ext>) -> Ext {
    value.into().fract()
}
pub fn sqr(value: impl Into<Ext>) -> Ext {
    let x = value.into();
    if x < 0.0 { x } else { x.sqrt() }
}

pub struct InputReader<'a>(super::numfmt::InputReader<'a>);
impl<'a> InputReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self(super::numfmt::InputReader::new(data))
    }
    pub fn position(&self) -> usize {
        self.0.position()
    }
    pub fn eof(&self) -> bool {
        self.0.eof()
    }
    pub fn try_input_num(&mut self) -> Result<Ext, u8> {
        self.0.try_input_num_ext().map(Ext)
    }
    pub fn try_input_num_ext(&mut self) -> Result<super::ext80::Ext80, u8> {
        self.0.try_input_num_ext()
    }
    pub fn try_input_str(&mut self) -> Result<Vec<u8>, u8> {
        self.0.try_input_str()
    }
    pub fn try_line_input(&mut self) -> Result<Vec<u8>, u8> {
        self.0.try_line_input()
    }
    pub fn input_num(&mut self) -> Ext {
        self.try_input_num().unwrap_or(Ext::ZERO)
    }
    pub fn input_str(&mut self) -> Vec<u8> {
        self.0.input_str()
    }
    pub fn line_input(&mut self) -> Vec<u8> {
        self.0.line_input()
    }
}
