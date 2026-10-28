//! JSON snapshots of [`crate::state::Globals`] keyed by original address.
//!
//! Used to exchange state with the emulator harness (`tools/emu/game.py`), which runs the original
//! routines on the same state for differential tests. Minimal JSON: objects of numbers, strings and flat
//! lists; strings carry one char per byte (U+0000..U+00FF).

use crate::pb::array::{Arr1, Arr2};
use crate::pb::pbstr::PbStr;
use crate::pb::types::{Cur, Cux};
use std::collections::HashMap;
use std::fmt::Write as _;

/// A value that can be stored in a snapshot.
pub trait SnapVal: Sized {
    fn write(&self, out: &mut String);
    fn read(v: &Json) -> Result<Self, String>;
    fn write_portable(&self, out: &mut String) { self.write(out); }
}

macro_rules! int_snap {
    ($($t:ty),*) => {$(
        impl SnapVal for $t {
            fn write(&self, out: &mut String) {
                let _ = write!(out, "{}", self);
            }
            fn read(v: &Json) -> Result<Self, String> {
                match v {
                    Json::Num(x) => match x.parse::<i64>() {
                        Ok(i) => Ok(i as $t),
                        Err(_) => Ok(x.parse::<f64>().map_err(|e| e.to_string())? as $t),
                    },
                    _ => Err("expected number".into()),
                }
            }
        }
    )*};
}
int_snap!(u8, i16, i32, i64);

impl SnapVal for crate::pb::real::Ext {
    fn write_portable(&self, out: &mut String) { self.to_f64().write(out); }
    fn write(&self, out: &mut String) {
        let rounded = self.to_f64();
        if self.is_finite() && crate::pb::ext80::Ext80::from_f64(rounded) == self.raw() {
            rounded.write(out);
        } else {
            let raw = self.raw();
            let _ = write!(out, "\"x87:{:04x}{:016x}\"", raw.se, raw.mant);
        }
    }
    fn read(value: &Json) -> Result<Self, String> {
        match value {
            Json::Str(bytes) if bytes.starts_with(b"x87:") && bytes.len() == 24 => {
                let text = std::str::from_utf8(&bytes[4..]).map_err(|e| e.to_string())?;
                let se = u16::from_str_radix(&text[..4], 16).map_err(|e| e.to_string())?;
                let mant = u64::from_str_radix(&text[4..], 16).map_err(|e| e.to_string())?;
                Ok(crate::pb::real::Ext(crate::pb::ext80::Ext80 { se, mant }))
            }
            // Legacy numeric snapshots describe DOUBLE values, just as the emulator writer does.
            _ => f64::read(value).map(Self::from_f64),
        }
    }
}

impl SnapVal for f64 {
    fn write(&self, out: &mut String) {
        if self.is_finite() {
            let _ = write!(out, "{:?}", self);
        } else {
            out.push_str("null");
        }
    }
    fn read(v: &Json) -> Result<Self, String> {
        match v {
            Json::Num(x) => x.parse::<f64>().map_err(|e| e.to_string()),
            Json::Null => Ok(f64::NAN),
            _ => Err("expected number".into()),
        }
    }
}
impl SnapVal for f32 {
    fn write(&self, out: &mut String) {
        (*self as f64).write(out)
    }
    fn read(v: &Json) -> Result<Self, String> {
        f64::read(v).map(|x| x as f32)
    }
}
impl SnapVal for Cur {
    fn write(&self, out: &mut String) {
        let _ = write!(out, "{}", self.0);
    }
    fn read(v: &Json) -> Result<Self, String> {
        match v {
            Json::Num(x) => x.parse::<i64>().map(Cur).map_err(|e| e.to_string()),
            _ => Err("expected integer".into()),
        }
    }
}
impl SnapVal for Cux {
    fn write(&self, out: &mut String) {
        let _ = write!(out, "{}", self.0);
    }
    fn read(v: &Json) -> Result<Self, String> {
        match v {
            Json::Num(x) => x.parse::<i64>().map(Cux).map_err(|e| e.to_string()),
            _ => Err("expected integer".into()),
        }
    }
}
impl SnapVal for PbStr {
    fn write(&self, out: &mut String) {
        out.push('"');
        for &b in self.as_bytes() {
            match b {
                b'"' => out.push_str("\\\""),
                b'\\' => out.push_str("\\\\"),
                0x20..=0x7e => out.push(b as char),
                _ => {
                    let _ = write!(out, "\\u{:04x}", b);
                }
            }
        }
        out.push('"');
    }
    fn read(v: &Json) -> Result<Self, String> {
        match v {
            Json::Str(s) => Ok(PbStr(s.clone())),
            _ => Err("expected string".into()),
        }
    }
}

#[derive(Default)]
pub struct Writer {
    out: String,
    portable: bool,
}

impl Writer {
    pub fn portable() -> Self { Self { portable: true, ..Self::default() } }
    fn key(&mut self, k: &str) {
        self.out.push(if self.out.is_empty() { '{' } else { ',' });
        let _ = write!(self.out, "\"{k}\":");
    }
    pub fn val<T: SnapVal>(&mut self, k: &str, v: &T) {
        self.key(k);
        if self.portable { v.write_portable(&mut self.out); } else { v.write(&mut self.out); }
    }
    fn list<T: SnapVal>(&mut self, k: &str, xs: &[T]) {
        self.key(k);
        self.out.push('[');
        for (i, x) in xs.iter().enumerate() {
            if i > 0 {
                self.out.push(',');
            }
            if self.portable { x.write_portable(&mut self.out); } else { x.write(&mut self.out); }
        }
        self.out.push(']');
    }
    pub fn arr1<T: SnapVal + Clone + Default>(&mut self, k: &str, a: &Arr1<T>) {
        self.list(k, a.as_slice())
    }
    pub fn arr2<T: SnapVal + Clone + Default>(&mut self, k: &str, a: &Arr2<T>) {
        self.list(k, a.as_slice())
    }
    pub fn finish(mut self) -> String {
        if self.out.is_empty() {
            self.out.push('{');
        }
        self.out.push('}');
        self.out
    }
}

/// Parsed JSON value (just what snapshots need).
#[derive(Debug, Clone)]
pub enum Json {
    Null,
    Num(String),
    Str(Vec<u8>),
    List(Vec<Json>),
}

pub struct Reader {
    map: HashMap<String, Json>,
}

impl Reader {
    pub fn parse(s: &str) -> Result<Self, String> {
        let mut p = Parser { s: s.as_bytes(), i: 0 };
        p.ws();
        p.expect(b'{')?;
        let mut map = HashMap::new();
        p.ws();
        if p.peek() == Some(b'}') {
            return Ok(Reader { map });
        }
        loop {
            p.ws();
            let k = match p.value()? {
                Json::Str(k) => String::from_utf8_lossy(&k).into_owned(),
                _ => return Err("expected key".into()),
            };
            p.ws();
            p.expect(b':')?;
            p.ws();
            let v = p.value()?;
            map.insert(k, v);
            p.ws();
            match p.next() {
                Some(b',') => continue,
                Some(b'}') => break,
                _ => return Err(format!("bad object at {}", p.i)),
            }
        }
        Ok(Reader { map })
    }
    pub fn val<T: SnapVal>(&self, k: &str, v: &mut T) -> Result<(), String> {
        if let Some(j) = self.map.get(k) {
            *v = T::read(j).map_err(|e| format!("{k}: {e}"))?;
        }
        Ok(())
    }
    fn list<T: SnapVal>(&self, k: &str, xs: &mut [T]) -> Result<(), String> {
        match self.map.get(k) {
            None => Ok(()),
            Some(Json::List(l)) if l.len() == xs.len() => {
                for (x, j) in xs.iter_mut().zip(l) {
                    *x = T::read(j).map_err(|e| format!("{k}: {e}"))?;
                }
                Ok(())
            }
            Some(Json::List(l)) => Err(format!("{k}: expected {} elements, got {}", xs.len(), l.len())),
            Some(_) => Err(format!("{k}: expected list")),
        }
    }
    pub fn arr1<T: SnapVal + Clone + Default>(&self, k: &str, a: &mut Arr1<T>) -> Result<(), String> {
        self.list(k, a.as_mut_slice())
    }
    pub fn arr2<T: SnapVal + Clone + Default>(&self, k: &str, a: &mut Arr2<T>) -> Result<(), String> {
        self.list(k, a.as_mut_slice())
    }
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }
    fn next(&mut self) -> Option<u8> {
        let c = self.peek();
        self.i += 1;
        c
    }
    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.i += 1;
        }
    }
    fn expect(&mut self, c: u8) -> Result<(), String> {
        if self.next() == Some(c) { Ok(()) } else { Err(format!("expected '{}' at {}", c as char, self.i - 1)) }
    }
    fn value(&mut self) -> Result<Json, String> {
        match self.peek() {
            Some(b'"') => {
                self.i += 1;
                let mut v = Vec::new();
                loop {
                    match self.next() {
                        Some(b'"') => return Ok(Json::Str(v)),
                        Some(b'\\') => match self.next() {
                            Some(b'u') => {
                                let h = std::str::from_utf8(self.s.get(self.i..self.i + 4).ok_or("bad \\u")?)
                                    .map_err(|e| e.to_string())?;
                                let c = u32::from_str_radix(h, 16).map_err(|e| e.to_string())?;
                                self.i += 4;
                                v.push(u8::try_from(c).map_err(|_| "non-byte \\u escape")?);
                            }
                            Some(b'n') => v.push(b'\n'),
                            Some(b'r') => v.push(b'\r'),
                            Some(b't') => v.push(b'\t'),
                            Some(b'b') => v.push(0x08),
                            Some(b'f') => v.push(0x0c),
                            Some(c) => v.push(c),
                            None => return Err("eof in string".into()),
                        },
                        Some(c) => v.push(c),
                        None => return Err("eof in string".into()),
                    }
                }
            }
            Some(b'[') => {
                self.i += 1;
                let mut l = Vec::new();
                self.ws();
                if self.peek() == Some(b']') {
                    self.i += 1;
                    return Ok(Json::List(l));
                }
                loop {
                    self.ws();
                    l.push(self.value()?);
                    self.ws();
                    match self.next() {
                        Some(b',') => continue,
                        Some(b']') => return Ok(Json::List(l)),
                        _ => return Err(format!("bad list at {}", self.i)),
                    }
                }
            }
            Some(b'n') => {
                self.i += 4;
                Ok(Json::Null)
            }
            _ => {
                let st = self.i;
                while matches!(self.peek(), Some(b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E')) {
                    self.i += 1;
                }
                if st == self.i {
                    return Err(format!("unexpected byte at {st}"));
                }
                Ok(Json::Num(String::from_utf8_lossy(&self.s[st..self.i]).into_owned()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::pb::pbstr::PbStr;
    use crate::state::Globals;

    #[test]
    fn globals_json_round_trip() {
        let mut g = Globals::default();
        g.cash[17] = crate::pb::real::Ext::from_f64(1234.5625);
        g.ent_name[131] = PbStr(b"Caf\xe9 \"Q\"".to_vec());
        g.cur_year = 2031;
        let j = g.to_json();
        let mut h = Globals::default();
        h.from_json(&j).unwrap();
        assert_eq!(h.cash[17], 1234.5625);
        assert_eq!(h.ent_name[131], g.ent_name[131]);
        assert_eq!(h.cur_year, 2031);
        assert_eq!(h.to_json(), j);
    }
}
