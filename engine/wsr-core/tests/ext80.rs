//! Checks crate::pb::ext80 against x87 results recorded under emulation (tools/emu/ext80.py; format
//! documented there).

use std::cmp::Ordering;
use wsr_core::pb::ext80::Ext80;

fn x80(h: &str) -> Ext80 {
    Ext80 {
        se: u16::from_str_radix(&h[..4], 16).unwrap(),
        mant: u64::from_str_radix(&h[4..], 16).unwrap(),
    }
}

fn hx(e: Ext80) -> String {
    format!("{:04x}{:016x}", e.se, e.mant)
}

#[test]
fn fixture() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ext80.txt")).unwrap();
    let mut fails = Vec::new();
    let mut n = 0;
    for line in text.lines().filter(|l| !l.starts_with('#') && !l.is_empty()) {
        let f: Vec<&str> = line.split('\t').collect();
        let a = x80(f[1]);
        let (got, want) = match f[0] {
            "A" => (hx(a + x80(f[2])), f[3].to_string()),
            "S" => (hx(a - x80(f[2])), f[3].to_string()),
            "M" => (hx(a * x80(f[2])), f[3].to_string()),
            "D" => (hx(a / x80(f[2])), f[3].to_string()),
            "R" => (hx(a.frndint()), f[2].to_string()),
            "Q" => (a.to_i64().to_string(), f[2].to_string()),
            "L" => (a.to_i32().to_string(), f[2].to_string()),
            "F" => (format!("{:016x}", a.to_f64().to_bits()), f[2].to_string()),
            "G" => (format!("{:08x}", a.to_f32().to_bits()), f[2].to_string()),
            "C" => {
                let c = match a.cmp_val(x80(f[2])) {
                    Ordering::Less => "lt",
                    Ordering::Equal => "eq",
                    Ordering::Greater => "gt",
                };
                (c.to_string(), f[3].to_string())
            }
            op => panic!("unknown op {op}"),
        };
        n += 1;
        if got != want {
            fails.push(format!("{line}\t-> got {got}"));
        }
    }
    assert!(n >= 40_000, "only {n} vectors");
    assert!(fails.is_empty(), "{} of {n} failed:\n{}", fails.len(), fails[..fails.len().min(30)].join("\n"));
}
