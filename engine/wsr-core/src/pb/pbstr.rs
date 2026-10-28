//! PowerBASIC dynamic strings: byte strings (Windows ANSI, code page 1252), not UTF-8.
//!
//! Comparisons are byte-wise like `PB_StrEqual` (0x6be966); the ordering is the binary order of the bytes.

use std::fmt;
use std::ops::{Add, AddAssign};

#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PbStr(pub Vec<u8>);

impl PbStr {
    pub fn new() -> Self {
        PbStr(Vec::new())
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
    /// `LEN(s)`.
    pub fn len(&self) -> i32 {
        self.0.len() as i32
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Decode as Windows-1252 for display.
    pub fn to_display(&self) -> String {
        self.0.iter().map(|&b| cp1252(b)).collect()
    }
    /// Encode a Rust string as Windows-1252 (unmappable characters become `?`).
    pub fn from_display(s: &str) -> Self {
        PbStr(s.chars().map(|c| from_cp1252(c).unwrap_or(b'?')).collect())
    }
}

impl From<&str> for PbStr {
    fn from(s: &str) -> Self {
        PbStr::from_display(s)
    }
}
impl From<&[u8]> for PbStr {
    fn from(s: &[u8]) -> Self {
        PbStr(s.to_vec())
    }
}
impl From<Vec<u8>> for PbStr {
    fn from(s: Vec<u8>) -> Self {
        PbStr(s)
    }
}
impl PartialEq<&str> for PbStr {
    fn eq(&self, other: &&str) -> bool {
        self.0 == PbStr::from_display(other).0
    }
}
impl Add<&PbStr> for PbStr {
    type Output = PbStr;
    fn add(mut self, rhs: &PbStr) -> PbStr {
        self.0.extend_from_slice(&rhs.0);
        self
    }
}
impl Add<&str> for PbStr {
    type Output = PbStr;
    fn add(mut self, rhs: &str) -> PbStr {
        self.0.extend_from_slice(&PbStr::from_display(rhs).0);
        self
    }
}
impl AddAssign<&PbStr> for PbStr {
    fn add_assign(&mut self, rhs: &PbStr) {
        self.0.extend_from_slice(&rhs.0);
    }
}
impl fmt::Debug for PbStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.to_display())
    }
}
impl fmt::Display for PbStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_display())
    }
}

const CP1252_HI: [char; 32] = [
    '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}', '\u{90}', '‘',
    '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
];

pub fn cp1252(b: u8) -> char {
    match b {
        0x80..=0x9f => CP1252_HI[(b - 0x80) as usize],
        _ => b as char,
    }
}

pub fn from_cp1252(c: char) -> Option<u8> {
    let u = c as u32;
    if u < 0x80 || (0xa0..=0xff).contains(&u) {
        return Some(u as u8);
    }
    CP1252_HI.iter().position(|&x| x == c).map(|k| 0x80 + k as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concat_and_compare() {
        let s = PbStr::from("Prime is ") + "5%";
        assert_eq!(s, "Prime is 5%");
        assert_eq!(PbStr::from("£").0, vec![0xa3]);
        assert_eq!(PbStr::from("€").0, vec![0x80]);
        assert!(PbStr(b"A".to_vec()) < PbStr(b"a".to_vec()));
    }
}
