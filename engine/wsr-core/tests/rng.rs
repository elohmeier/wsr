//! Checks `wsr_core::pb::rng` against vectors produced by running WSR.EXE's runtime under emulation
//! (tools/emu/rng.py -> tests/fixtures/rng.txt).

use wsr_core::pb::rng::{timer_ext, timer_f64, timer_seed_bits, Rng};

const FIXTURE: &str = include_str!("fixtures/rng.txt");

fn hex32(s: &str) -> u32 {
    u32::from_str_radix(s, 16).unwrap()
}

fn hex64(s: &str) -> u64 {
    u64::from_str_radix(s, 16).unwrap()
}

/// 80-bit extended (20 hex digits, big-endian: sign/exp then significand) -> (sign_exp, significand).
fn ext(s: &str) -> (u16, u64) {
    assert_eq!(s.len(), 20);
    (u16::from_str_radix(&s[..4], 16).unwrap(), hex64(&s[4..]))
}

/// Exact 80-bit -> f64 (panics if not representable).
fn ext_to_f64(s: &str) -> f64 {
    let (se, m) = ext(s);
    let sign = if se >> 15 == 1 { -1.0 } else { 1.0 };
    if m == 0 {
        return sign * 0.0;
    }
    let e = (se & 0x7fff) as i32 - 16383 - 63;
    // split to avoid intermediate overflow/underflow of 2^e
    let v = sign * (m as f64) * 2f64.powi(e / 2) * 2f64.powi(e - e / 2);
    assert_eq!((v.abs() / 2f64.powi(e / 2) / 2f64.powi(e - e / 2)) as u64, m, "seed {s} not an f64");
    v
}

fn lines(tag: &str) -> impl Iterator<Item = Vec<&'static str>> + '_ {
    FIXTURE
        .lines()
        .filter(move |l| l.starts_with(tag) && l.as_bytes().get(tag.len()) == Some(&b' '))
        .map(|l| l.split_whitespace().skip(1).collect())
}

#[test]
fn default_state_sequence() {
    let mut count = 0;
    for f in lines("D") {
        let n: usize = f[0].parse().unwrap();
        let mut rng = Rng::default();
        assert_eq!(rng.state(), 0xffff_ffff);
        for i in 0..n {
            assert_eq!(rng.rnd().to_bits(), hex64(f[1 + i]), "default seq #{i}");
        }
        assert_eq!(rng.state(), hex32(f[1 + n]));
        count += 1;
    }
    assert_eq!(count, 1);
}

#[test]
fn randomize_sequences() {
    let mut count = 0;
    for f in lines("R") {
        let seed = ext_to_f64(f[0]);
        let mut rng = Rng::default();
        rng.randomize(seed);
        assert_eq!(rng.state(), hex32(f[1]), "RANDOMIZE {seed:e} ({})", f[0]);
        let n: usize = f[2].parse().unwrap();
        for i in 0..n {
            assert_eq!(rng.rnd().to_bits(), hex64(f[3 + i]), "seed {seed:e} #{i}");
        }
        assert_eq!(rng.state(), hex32(f[3 + n]));
        count += 1;
    }
    assert!(count > 300, "{count}");
}

#[test]
fn rnd_range() {
    let mut count = 0;
    for f in lines("I") {
        let mut rng = Rng::from_state(hex32(f[0]));
        let a: i32 = f[1].parse().unwrap();
        let b: i32 = f[2].parse().unwrap();
        let want: i32 = f[3].parse().unwrap();
        assert_eq!(rng.rnd_range(a, b), want, "state {} RND({a}, {b})", f[0]);
        assert_eq!(rng.state(), hex32(f[4]));
        count += 1;
    }
    assert!(count > 1000, "{count}");
}

#[test]
fn randomize_timer() {
    let mut count = 0;
    for f in lines("T") {
        let [h, m, s, ms]: [u32; 4] = std::array::from_fn(|i| f[i].parse().unwrap());
        let t = h * 3_600_000 + m * 60_000 + s * 1000 + ms;
        let (se, sig) = ext(f[4]);
        match timer_ext(t) {
            None => assert_eq!(sig, 0),
            Some((q, k)) => {
                assert_eq!(q, sig, "TIMER significand at {t} ms");
                assert_eq!(16383 + 63 - k, se as i32, "TIMER exponent at {t} ms");
            }
        }
        let mut rng = Rng::default();
        rng.randomize_timer(t);
        assert_eq!(rng.state(), hex32(f[5]), "RANDOMIZE TIMER at {t} ms");
        count += 1;
    }
    assert!(count > 100, "{count}");
}

/// The f64 approximation of TIMER seeds identically to the 80-bit one (sampled here; verified exhaustively
/// for all 86_400_000 ms with step 1 in release mode).
#[test]
fn timer_f64_matches_ext_seed() {
    for t in (0..86_400_000u32).step_by(997) {
        assert_eq!((timer_f64(t) as f32).to_bits(), timer_seed_bits(t), "{t} ms");
    }
}
