//! FORMAT$ vectors generated from the original runtime (tools/emu/format.py).

use wsr_core::pb::format::format;

fn unesc(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && b.get(i + 1) == Some(&b'x') {
            out.push(u8::from_str_radix(&s[i + 2..i + 4], 16).unwrap());
            i += 4;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    out
}

#[test]
fn format_fixture() {
    let text = include_str!("fixtures/format.txt");
    let mut n = 0;
    let mut fails = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let mut it = line.split('\t');
        let bits = u64::from_str_radix(it.next().unwrap(), 16).unwrap();
        let mask = unesc(it.next().unwrap());
        let want = unesc(it.next().unwrap());
        let x = f64::from_bits(bits);
        let got = format(x, &mask);
        n += 1;
        if got != want {
            fails.push(format!(
                "FORMAT$({:e} [{:016x}], {:?}) = {:?}, want {:?}",
                x,
                bits,
                String::from_utf8_lossy(&mask),
                String::from_utf8_lossy(&got),
                String::from_utf8_lossy(&want)
            ));
        }
    }
    assert!(n > 1000, "fixture too small: {n}");
    if !fails.is_empty() {
        let shown: Vec<_> = fails.iter().take(40).cloned().collect();
        panic!(
            "{} of {} vectors failed:\n{}",
            fails.len(),
            n,
            shown.join("\n")
        );
    }
}

#[test]
fn format_examples() {
    let f = |x: f64, m: &str| String::from_utf8(format(x, m.as_bytes())).unwrap();
    assert_eq!(f(1234.5, "* ###,.00"), " 1,234.50");
    assert_eq!(f(1234.5, "* ###,"), " 1,234");
    assert_eq!(f(-1234567.5, "* ###,"), "-1,234,568");
    assert_eq!(f(1234.5, "0.0E+00"), "1.2E+3");
}
