//! Self-test of the differential harness (tools/emu/game.py): a state with every field randomised must
//! survive the trip into emulator memory and back unchanged (calling the harness's own `hlt` sentinel
//! instead of a game routine).

mod diff;

use diff::{Compare, Rand, layout, run_original};
use std::fmt::Write as _;
use wsr_core::state::Globals;

/// Return address of the harness trampoline (`hlt`): "calling" it runs no game code.
const SENTINEL: u32 = 0x0f00_0080;

fn rand_val(r: &mut Rand, typ: &str, out: &mut String) {
    let __content = wsr_core::content::current();
    match typ {
        "INTEGER" => write!(out, "{}", r.range(-32768, 32767)).unwrap(),
        "LONG" => write!(out, "{}", r.range(i32::MIN as i64, i32::MAX as i64)).unwrap(),
        "BYTE" => write!(out, "{}", r.range(0, 255)).unwrap(),
        "QUAD" => write!(out, "{}", r.next() as i64).unwrap(),
        "CUR" | "CUX" => write!(out, "{}", r.next() as i64).unwrap(),
        "SINGLE" => write!(
            out,
            "{:?}",
            (f32::from_bits(r.next() as u32 & 0x7f7f_ffff) as f64)
        )
        .unwrap(),
        "EXT" | "DOUBLE" => {
            let x = match r.range(0, 5) {
                0 => 0.0,
                1 => r.range(-1_000_000_000, 1_000_000_000) as f64 / 64.0,
                2 => f64::from_bits(r.next() & 0x000f_ffff_ffff_ffff), // subnormal
                _ => {
                    let x = f64::from_bits(r.next());
                    if x.is_finite() { x } else { 1.5e300 }
                }
            };
            write!(out, "{:?}", x).unwrap()
        }
        "STRING" => {
            out.push('"');
            if r.chance(0.6) {
                for _ in 0..r.range(1, 12) {
                    let b = r.range(0, 255) as u8;
                    match b {
                        b'"' => out.push_str("\\\""),
                        b'\\' => out.push_str("\\\\"),
                        0x20..=0x7e => out.push(b as char),
                        _ => write!(out, "{__wsr_text_13527}{:04x}", b,__wsr_text_13527=__content.string(13527)).unwrap(),
                    }
                }
            }
            out.push('"');
        }
        t => panic!("{__wsr_text_13745}{t}",__wsr_text_13745=__content.string(13745)),
    }
}

#[test]
fn snapshot_survives_emulator_memory() {
    let __content = wsr_core::content::current();
    let mut r = Rand(0x5eed);
    let mut j = String::from("{");
    for (k, f) in layout().iter().enumerate() {
        if k > 0 {
            j.push(',');
        }
        write!(j, "\"{}\":", f.key).unwrap();
        if f.bounds.is_empty() {
            rand_val(&mut r, &f.typ, &mut j);
        } else {
            let n: i64 = f
                .bounds
                .iter()
                .map(|&(lo, hi)| (hi - lo + 1) as i64)
                .product();
            j.push('[');
            for i in 0..n {
                if i > 0 {
                    j.push(',');
                }
                // keep arrays mostly default so the test stays fast; randomise a sprinkling
                if r.chance(0.05) {
                    rand_val(&mut r, &f.typ, &mut j);
                } else {
                    j.push_str(if f.typ == "STRING" { "\"\"" } else { "0" });
                }
            }
            j.push(']');
        }
    }
    j.push('}');
    let mut g = Globals::default();
    g.from_json(&j).unwrap();
    assert_eq!(layout().len(), wsr_core::state::GLOBALS.len());
    let Some((back, log)) = run_original(SENTINEL, &[], &g, &[]) else {
        return;
    };
    assert!(log.is_empty(), "{log:?}");
    Compare::default().rel_tol(0.0).assert(&g, &back);
}

#[test]
fn cached_original_outputs_survive_multiple_reads() {
    if diff::unavailable().is_some() { return; }
    let cache = std::env::temp_dir().join(format!("wsr-cache-regression-{}", std::process::id()));
    std::fs::create_dir_all(&cache).unwrap();
    for attempt in 0..3 {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", "snapshot_survives_emulator_memory"])
            .env("WSR_EMU_CACHE_DIR", &cache)
            .env("WSR_EMU_CACHE_MB", "100")
            .env_remove("WSR_EMU_NO_CACHE");
        if attempt > 0 {
            // A cache miss must fail, rather than silently re-running the original.
            command.env("WSR_EMU_PYTHON", cache.join("no-emulator-launcher"));
        }
        let output = command.output().unwrap();
        assert!(output.status.success(), "cache read {attempt}: {}{}",
            String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    }
    std::fs::remove_dir_all(cache).unwrap();
}
