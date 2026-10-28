//! Checks crate::pb::numfmt against vectors produced by the original WSR.EXE runtime under
//! emulation (tools/emu/numfmt.py; format documented there).

use wsr_core::pb::numfmt::*;

fn x80(h: &str) -> Ext80 {
    Ext80 {
        se: u16::from_str_radix(&h[..4], 16).unwrap(),
        mant: u64::from_str_radix(&h[4..], 16).unwrap(),
    }
}

fn bytes(h: &str) -> Vec<u8> {
    if h == "-" {
        return Vec::new();
    }
    (0..h.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
        .collect()
}

fn hx(e: Ext80) -> String {
    format!("{:04x}{:016x}", e.se, e.mant)
}

/// The value as f64 if it is exactly representable (then the f64 API must give the same result).
fn as_f64(e: Ext80) -> Option<f64> {
    let f = e.to_f64();
    (Ext80::from_f64(f) == e || (e.se & 0x7fff == 0 && f == 0.0)).then_some(f)
}

#[test]
fn fixture() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/numfmt.txt"
    ))
    .unwrap();
    let mut fails = Vec::new();
    let mut counts = [0usize; 5];
    for line in text.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        match f[0] {
            "S" => {
                counts[0] += 1;
                let digits: i64 = f[1].parse().unwrap();
                let digits = digits as u32;
                let keep = f[2] == "1";
                let x = x80(f[3]);
                let want = f.get(4).copied().unwrap_or("");
                let got = if keep {
                    str_num_ext(x, digits)
                } else {
                    format_num_ext(x, digits)
                };
                if got != want {
                    fails.push(format!("{line}  got {got:?}"));
                }
                if let Some(v) = as_f64(x) {
                    let got = if keep {
                        str_num(v, digits)
                    } else {
                        format_num(v, digits)
                    };
                    if got != want {
                        fails.push(format!("{line}  (f64) got {got:?}"));
                    }
                }
            }
            "W" => {
                counts[1] += 1;
                let v = as_f64(x80(f[1])).expect("W vectors are f64 values");
                let got = write_num(v);
                if got != f[2] {
                    fails.push(format!("{line}  got {got:?}"));
                }
            }
            "Q" => {
                counts[2] += 1;
                if write_str(&bytes(f[1])) != bytes(f[2]) {
                    fails.push(line.to_string());
                }
            }
            "V" => {
                counts[3] += 1;
                let s = bytes(f[1]);
                let got = val_ext(&s);
                let want = x80(f[2]);
                // NaN results are not produced; compare bits exactly
                if got != want {
                    fails.push(format!("{line}  got {}", hx(got)));
                }
                let g = val(&s);
                let w = want.to_f64();
                if g.to_bits() != w.to_bits() {
                    fails.push(format!("{line}  (f64) got {g:e} want {w:e}"));
                }
            }
            "I" => {
                counts[4] += 1;
                let data = bytes(f[2]);
                let ops = f[3];
                let mut r = InputReader::new(&data);
                let mut got: Vec<String> = Vec::new();
                for op in ops.chars() {
                    let res = match op {
                        'N' => r.try_input_num_ext().map(hx),
                        'S' => r.try_input_str().map(|b| {
                            if b.is_empty() {
                                "-".into()
                            } else {
                                b.iter().map(|c| format!("{c:02x}")).collect()
                            }
                        }),
                        'L' => r.try_line_input().map(|b| {
                            if b.is_empty() {
                                "-".into()
                            } else {
                                b.iter().map(|c| format!("{c:02x}")).collect()
                            }
                        }),
                        'E' => Ok(if r.eof() { "1".into() } else { "0".into() }),
                        _ => unreachable!(),
                    };
                    match res {
                        Ok(s) => got.push(s),
                        Err(e) => {
                            got.push(format!("!{e}"));
                            break;
                        }
                    }
                }
                let want: Vec<String> = f[4..].iter().map(|s| s.to_string()).collect();
                if got != want {
                    fails.push(format!("{line}\n    got {}", got.join("\t")));
                }
            }
            other => panic!("unknown record {other}"),
        }
    }
    assert!(counts.iter().all(|&c| c > 0), "{counts:?}");
    if !fails.is_empty() {
        let n = fails.len();
        fails.truncate(40);
        panic!("{n} mismatches:\n{}", fails.join("\n"));
    }
    eprintln!(
        "checked S={} W={} Q={} V={} I={}",
        counts[0], counts[1], counts[2], counts[3], counts[4]
    );
}

#[test]
fn api_examples() {
    assert_eq!(str_num(1.0 / 3.0, 15), " .333333333333333");
    assert_eq!(str_num(-0.001234, 7), "-.001234");
    assert_eq!(format_num(1e15 - 1.0, 15), "999999999999999");
    assert_eq!(write_num(12345.67), "12345.67");
    assert_eq!(write_str(b"a\"b"), b"\"a\"b\"".to_vec());
    let data = b"1.5, \"a,b\" ,x\r\nline 2\r\n";
    let mut r = InputReader::new(data);
    assert_eq!(r.input_num(), 1.5);
    assert_eq!(r.input_str(), b"a,b".to_vec());
    assert_eq!(r.input_str(), b"x".to_vec());
    assert_eq!(r.line_input(), b"line 2".to_vec());
    assert!(r.eof());
    assert_eq!(r.position(), data.len());
}
