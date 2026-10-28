//! Sequential file I/O as the game uses it: `WRITE #` to produce saves, `INPUT #`/`LINE INPUT #` to read.
//!
//! Files are in-memory byte buffers; the browser host persists them in local storage.
//! Item text comes from [`crate::pb::numfmt`]; a `WRITE #` statement joins its items with `,` and
//! ends with CRLF (`PB_WriteSep` 0x6b7d5d, `PB_PrintEnd` 0x6b8fe8).

use crate::pb::numfmt::{self, InputReader};
use crate::pb::pbstr::PbStr;
use crate::pb::{ext80::Ext80, types::{Cur, Cux}};

/// One item of a `WRITE #` statement.
pub enum Item<'a> {
    Num(f64),
    /// Exact FILD / CURRENCY load, without an intermediate DOUBLE rounding.
    Ext(Ext80),
    Str(&'a PbStr),
}

impl From<f64> for Item<'_> {
    fn from(x: f64) -> Self {
        Item::Num(x)
    }
}
impl From<crate::pb::real::Ext> for Item<'_> {
    fn from(value: crate::pb::real::Ext) -> Self { Item::Ext(value.raw()) }
}
impl From<f32> for Item<'_> {
    fn from(x: f32) -> Self {
        Item::Num(x as f64)
    }
}
impl From<i64> for Item<'_> {
    fn from(x: i64) -> Self {
        Item::Ext(Ext80::from_i64(x))
    }
}
impl From<Cur> for Item<'_> {
    fn from(x: Cur) -> Self { Item::Ext(x.get_ext()) }
}
impl From<Cux> for Item<'_> {
    fn from(x: Cux) -> Self { Item::Ext(x.get_ext()) }
}
impl From<i32> for Item<'_> {
    fn from(x: i32) -> Self {
        Item::Num(x as f64)
    }
}
impl From<i16> for Item<'_> {
    fn from(x: i16) -> Self {
        Item::Num(x as f64)
    }
}
impl<'a> From<&'a PbStr> for Item<'a> {
    fn from(s: &'a PbStr) -> Self {
        Item::Str(s)
    }
}

/// An output file opened `FOR OUTPUT`.
#[derive(Default)]
pub struct OutFile {
    pub data: Vec<u8>,
}

impl OutFile {
    /// `WRITE #f, a, b, …`
    pub fn write(&mut self, items: &[Item]) {
        for (k, it) in items.iter().enumerate() {
            if k > 0 {
                self.data.push(b',');
            }
            match it {
                Item::Num(x) => self.data.extend_from_slice(numfmt::write_num(*x).as_bytes()),
                Item::Ext(x) => self.data.extend_from_slice(numfmt::format_num_ext(*x, 15).as_bytes()),
                Item::Str(s) => self.data.extend(numfmt::write_str(s.as_bytes())),
            }
        }
        self.data.extend_from_slice(b"\r\n");
    }
    /// `PRINT #f, s` (no quoting).
    pub fn print(&mut self, s: &PbStr) {
        self.data.extend_from_slice(s.as_bytes());
        self.data.extend_from_slice(b"\r\n");
    }
}

/// `WRITE #f, …` with mixed item types: `write!(f, a, b, c)`.
#[macro_export]
macro_rules! pb_write {
    ($f:expr $(, $x:expr)* $(,)?) => {
        $f.write(&[$($crate::pb::fileio::Item::from($x)),*])
    };
}

/// An input file opened `FOR INPUT`.
pub struct InFile<'a> {
    r: InputReader<'a>,
}

impl<'a> InFile<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        InFile { r: InputReader::new(data) }
    }
    /// `INPUT #f, x` for a numeric variable (assign with the `to_*` helpers in `pb::types`).
    pub fn num(&mut self) -> f64 {
        self.r.input_num()
    }
    /// `INPUT #f, s$`.
    pub fn str(&mut self) -> PbStr {
        PbStr(self.r.input_str())
    }
    /// `LINE INPUT #f, s$`.
    pub fn line(&mut self) -> PbStr {
        PbStr(self.r.line_input())
    }
    /// `EOF(f)`.
    pub fn eof(&self) -> bool {
        self.r.eof()
    }
}
