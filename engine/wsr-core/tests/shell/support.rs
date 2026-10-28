//! Shared helpers of the shell tests: routine names (tools/names.tsv) and the comparison of skipped
//! calls (`Env::skipped` of the port vs. the original's `SKIP Name` log lines).
#![allow(dead_code)]

use super::diff::Original;
use std::collections::HashMap;

/// Routine names by address, from tools/names.tsv.
pub fn names() -> &'static HashMap<u32, String> {
    static N: std::sync::OnceLock<HashMap<u32, String>> = std::sync::OnceLock::new();
    N.get_or_init(|| {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/names.tsv");
        let mut m = HashMap::new();
        for l in std::fs::read_to_string(p).unwrap_or_default().lines() {
            let mut f = l.split('\t');
            if let (Some(a), Some(n)) = (f.next(), f.next())
                && a.len() == 6
                && let Ok(a) = u32::from_str_radix(a, 16)
            {
                m.insert(a, n.to_string());
            }
        }
        m
    })
}

/// The name of the routine at `addr` (or `0x..`).
pub fn name_of(addr: u32) -> String {
    let __content = wsr_core::content::current();
    names().get(&addr).cloned().unwrap_or_else(|| format!("{__wsr_text_13638}{addr:x}",__wsr_text_13638=__content.string(13638)))
}

/// The routines the original skipped, in call order (`SKIP Name [from 0x..]` log lines).
pub fn orig_skipped(o: &Original) -> Vec<String> {
    o.log
        .iter()
        .filter_map(|l| l.strip_prefix("SKIP "))
        .map(|l| l.split(" [from").next().unwrap_or(l).trim().to_string())
        .collect()
}

/// The port's skipped calls as names.
pub fn port_skipped(trace: &[u32]) -> Vec<String> {
    trace.iter().map(|&a| name_of(a)).collect()
}

/// Asserts the port skipped the same routines in the same order as the original.
pub fn assert_same_skips(seed: u64, trace: &[u32], o: &Original) {
    let p = port_skipped(trace);
    let q = orig_skipped(o);
    assert_eq!(p, q, "seed {seed}: skipped calls differ (port vs original)");
}
