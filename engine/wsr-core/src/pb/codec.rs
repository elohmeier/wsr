//! Line obfuscation used for saved games (and, with the same shift, the text data files).
//!
//! Mirrors `EncodeDecodeSaveGameFile` (0x5d212d) and its GOSUBs `…_EncodeLine` (0x5d2a96) and
//! `…_DecodeLine` (0x5d24d2). `SaveGame` (0x4857af) first writes the plain `WRITE #` text to
//! `OLDGAME.DAT`; this codec then turns it into `GAMExx.DAT`:
//!
//! * the first output line is `@EC<version>` (plain);
//! * empty lines, `@END` and lines starting with `@IT` are copied unchanged;
//! * every other byte `c` at 1-based position `p` with `40 <= c <= 170` becomes `c + 26 + 2p` for
//!   `p < 6` (+28, +30, +32, +34, +36) and `c + 39` from position 6 on; other bytes are copied.
//!
//! Decoding applies the inverse to encoded bytes in `40..=170` only. **Original bug:** plain bytes
//! that encode above 170 (`c >= 132` from position 6, or e.g. `c >= 135` at position 5) are not
//! decoded, so characters such as `£` (0xa3) in a saved string come back altered. [`decode_line`]
//! reproduces this; [`decode_line_fixed`] undoes every shift the encoder applied.

fn shift(p: usize) -> u8 {
    if p < 6 { (26 + 2 * p) as u8 } else { 39 }
}

fn passthrough(line: &[u8]) -> bool {
    line.is_empty() || line == b"@END" || line.starts_with(b"@IT")
}

/// Encode one line (0x5d2a96). Returns `None` where the original aborts with the
/// "CHARACTER + 39 = …" message box (a byte would exceed 255; cannot happen for `c <= 170`).
pub fn encode_line(line: &[u8]) -> Vec<u8> {
    if passthrough(line) {
        return line.to_vec();
    }
    line.iter()
        .enumerate()
        .map(|(k, &c)| if (40..=170).contains(&c) { c + shift(k + 1) } else { c })
        .collect()
}

/// Decode one line exactly like the original (0x5d24d2), including its lossy handling of bytes
/// that were encoded above 170. Returns `None` where the original aborts ("CHARACTER - 39 = …",
/// a decoded byte below 16), and for the bare `@EC` line, which the original skips.
pub fn decode_line(line: &[u8]) -> Option<Vec<u8>> {
    if passthrough(line) {
        return Some(line.to_vec());
    }
    if line == b"@EC" {
        return None;
    }
    let mut out = Vec::with_capacity(line.len());
    for (k, &c) in line.iter().enumerate() {
        let p = k + 1;
        if !(40..=170).contains(&c) {
            out.push(c);
        } else if p < 6 {
            out.push(c - shift(p));
        } else {
            let d = c - 39;
            if d < 16 {
                return None;
            }
            out.push(d);
        }
    }
    Some(out)
}

/// Decode one line, inverting every shift the encoder can apply (fixes the bug described above).
/// Ambiguous only for encoded bytes that the encoder would also have copied unchanged; we prefer the
/// shifted interpretation when it yields a byte in `40..=170`.
pub fn decode_line_fixed(line: &[u8]) -> Vec<u8> {
    if passthrough(line) {
        return line.to_vec();
    }
    line.iter()
        .enumerate()
        .map(|(k, &c)| {
            let s = shift(k + 1);
            match c.checked_sub(s) {
                Some(d) if (40..=170).contains(&d) => d,
                _ => c,
            }
        })
        .collect()
}

/// Split a file into lines the way `LINE INPUT #` sees them (CRLF or LF terminated).
pub fn lines(data: &[u8]) -> Vec<&[u8]> {
    let mut v: Vec<&[u8]> = data.split(|&b| b == b'\n').map(|l| l.strip_suffix(b"\r").unwrap_or(l)).collect();
    if v.last().is_some_and(|l| l.is_empty()) {
        v.pop();
    }
    v
}

/// Encode a whole plain save (`OLDGAME.DAT` text) into the on-disk format, with `@EC<version>` header.
pub fn encode_save(plain: &[u8], version: &[u8]) -> Vec<u8> {
    let mut out = b"@EC".to_vec();
    out.extend_from_slice(version);
    out.extend_from_slice(b"\r\n");
    for l in lines(plain) {
        out.extend(encode_line(l));
        out.extend_from_slice(b"\r\n");
    }
    out
}

/// Result of [`decode_save`].
#[derive(Debug)]
pub struct DecodedSave {
    /// Version from the `@EC` header, if present.
    pub version: Option<Vec<u8>>,
    /// Plain text as `LoadGame` reads it.
    pub plain: Vec<u8>,
}

/// Decode an on-disk save. Files without an `@EC` header are treated as plain (very old versions).
pub fn decode_save(data: &[u8], fixed: bool) -> DecodedSave {
    let ls = lines(data);
    let (version, body) = match ls.first() {
        Some(h) if h.starts_with(b"@EC") => (Some(h[3..].to_vec()), &ls[1..]),
        _ => return DecodedSave { version: None, plain: data.to_vec() },
    };
    let mut plain = Vec::with_capacity(data.len());
    for l in body {
        let d = if fixed { Some(decode_line_fixed(l)) } else { decode_line(l) };
        if let Some(d) = d {
            plain.extend(d);
            plain.extend_from_slice(b"\r\n");
        }
    }
    DecodedSave { version, plain }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ascii() {
        let l = b"\"Wall Street Raider\",42,-1.5";
        let e = encode_line(l);
        assert_eq!(&e[..5], &[b'"', b'W' + 30, b'a' + 32, b'l' + 34, b'l' + 36]);
        assert_eq!(decode_line(&e).unwrap(), l.to_vec());
        assert_eq!(decode_line_fixed(&e), l.to_vec());
    }

    #[test]
    fn passthrough_lines() {
        for l in [&b""[..], b"@END", b"@IT0001"] {
            assert_eq!(encode_line(l), l.to_vec());
        }
    }

    #[test]
    fn original_bug_with_high_bytes() {
        let l = b"abcdef\xa3";
        let e = encode_line(l);
        assert_eq!(e[6], 0xa3 + 39);
        assert_ne!(decode_line(&e).unwrap(), l.to_vec());
        assert_eq!(decode_line_fixed(&e), l.to_vec());
    }

    #[test]
    fn whole_file() {
        let plain = b"1\r\n\"ABC\"\r\n2.5\r\n";
        let enc = encode_save(plain, b"9.85");
        assert!(enc.starts_with(b"@EC9.85\r\n"));
        let d = decode_save(&enc, false);
        assert_eq!(d.version.as_deref(), Some(&b"9.85"[..]));
        assert_eq!(d.plain, plain.to_vec());
    }
}
