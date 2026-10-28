//! Tests of logic::shell::leftovers: ConvertOldFuturesUnits (0x63663c) and
//! DeliverPhysCommodForShortFutures (0x664ce0) against the original.
use wsr_core::pb::real::{Ext, cast};


use super::diff::{Rand, par_seeds};
use super::support::assert_same_skips;
#[allow(unused_imports)]
use super::{common as sim_common, diff};
use std::sync::Mutex;
use wsr_core::logic::newgame::LocalTime;
use wsr_core::logic::shell::Env;
use wsr_core::logic::shell::leftovers::{convert_old_futures_units, deliver_phys_commod_for_short_futures};
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;

#[path = "../commod_ai/common.rs"]
mod ccommon;
use ccommon::*;

#[test]
fn convert_old_futures_units_matches_original() {
    let __content = wsr_core::content::current();
    par_seeds(1..=12u64, |seed| {
        let (mut g, _) = cworld(seed);
        let mut r = Rand(seed ^ 0x55aa);
        // gaps, fractional and negative unit counts, padded names, extra fields
        for i in 1..=60 {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
                g.futures_recs[i] = PbStr::new();
            } else if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                let c = COMMODS[cast::<usize, _>(r.range(0, 7))];
                g.futures_recs[i] = st(&format!(
                    " {c} |{}|{}.{}|{}|{}|{:.3}|{:.2}{__wsr_text_14132}",
                    r.range(1, 1600),
                    r.range(-90, 90),
                    r.range(0, 99),
                    r.range(1, 12),
                    g.cur_year,
                    f64r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(900.0))),
                    f64r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(99.0)))
                ,__wsr_text_14132=__content.string(14132)));
            }
        }
        for i in 1..=40 {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                let c = PHYS[cast::<usize, _>(r.range(0, 6))];
                g.phys_commod_rec[i] = st(&format!("{c}|{}|{}.{}5|{:.2}{__wsr_text_14133}", r.range(1, 1600), r.range(0, 900), r.range(0, 9), f64r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(900.0))),__wsr_text_14133=__content.string(14133)));
            }
        }
        g.futures_recs[1000] = st("SILVER|12|3.25|5|2001|10|1");
        g.phys_commod_rec[1000] = st("GOLD|12|2.5|10");
        let mut pg = g.clone();
        let mut ui = Rec::default();
        convert_old_futures_units(&mut pg, &mut ui);
        let Some(o) = run(0x63663c, &[], &g, 1, &[], &[]) else { return };
        cmp().assert(&pg, &o.globals);
        assert_eq!(ui.msgs.len(), orig_msgs(&o), "seed {seed}");
    });
}

#[test]
fn deliver_phys_commod_for_short_futures_matches_original() {
    let __content = wsr_core::content::current();
    let hits = Mutex::new(std::collections::BTreeSet::new());
    par_seeds(1..=40u64, |seed| {
        let (mut g, owners) = cworld(seed);
        let mut r = Rand(seed ^ 0x77);
        // a futures record of an owner that (often) also holds the commodity physically
        let c = PHYS[cast::<usize, _>(r.range(0, 4))];
        let o = owners[cast::<usize, _>(r.range(0, 9))];
        let n: i32 = g.phys_commod_rec[0].to_display().parse().unwrap();
        let mut units = 0;
        if r.chance(cast::<f64, _>(Ext::from_f64(0.8))) {
            let mut k = (1..=n)
                .find(|&k| {
                    let t = g.phys_commod_rec[k].to_display();
                    t.starts_with(&format!("{c}|{o}|"))
                })
                .unwrap_or(0);
            if k == 0 {
                k = n + 1;
                g.phys_commod_rec[0] = st(&k.to_string());
            }
            units = if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) { 0 } else { r.range(1, 120) };
            g.phys_commod_rec[k] = st(&format!("{c}|{o}|{units}|{:.2}", f64r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(900.0)))));
        }
        if seed % 13 == 0 {
            g.phys_commod_rec[0] = st("0");
        }
        if seed % 11 == 0 {
            g.entity_status[o] = 2;
        }
        let contracts = if seed % 9 == 0 { 0 } else { r.range(1, 120) * if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) { -1 } else { 1 } };
        let rec = format!("{c}|{o}|{contracts}|{}|{}|{:.4}|{:.2}", r.range(1, 12), g.cur_year, f64r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(3000.0))), f64r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(99.0))));
        let mut pg = g.clone();
        let mut ui = Rec::default();
        let rng0 = rng0(seed);
        let mut rng = Rng::from_state(rng0);
        let now = LocalTime::HARNESS;
        let env = Env::new(&now);
        let res = deliver_phys_commod_for_short_futures(&mut pg, &mut ui, &mut rng, &env, &st(&rec));
        let Some(o) = run(0x664ce0, &[&format!("s:{rec}")], &g, rng0, &[], &[]) else { return };
        cmp().rel_tol(Ext::from_f64(1e-12)).assert(&pg, &o.globals);
        assert_eq!(rng.state(), o.rng, "seed {seed}");
        assert_eq!(cast::<u32, _>(res), o.return_value, "seed {seed}");
        assert_eq!(ui.msgs.len(), orig_msgs(&o), "seed {seed}");
        let mut h = hits.lock().unwrap();
        h.insert(format!("{__wsr_text_14134}{res}",__wsr_text_14134=__content.string(14134)));
        if res == 1 {
            h.insert(if (cast::<i64, _>(contracts.unsigned_abs())) < units { "partial" } else { "all" }.to_string());
        }
    });
    if diff::unavailable().is_none() && std::env::var("WSR_ONLY_SEED").is_err() {
        let h = hits.lock().unwrap();
        for w in ["res0", "res1", "partial", "all"] {
            assert!(h.contains(w), "branch not hit: {w} ({h:?})");
        }
    }
}

#[test]
fn deliver_phys_commod_skip_trace() {
    // SellPhysCommod skipped: the trace names it exactly when a delivery happens
    par_seeds(1..=6u64, |seed| {
        let (mut g, owners) = cworld(seed);
        let o = owners[1];
        g.entity_status[o] = 1;
        let n: i32 = g.phys_commod_rec[0].to_display().parse().unwrap();
        g.phys_commod_rec[n + 1] = st(&format!("GOLD|{o}|50|100"));
        g.phys_commod_rec[0] = st(&(n + 1).to_string());
        let rec = format!("GOLD|{o}|-{}|3|{}|100|1", seed * 7, g.cur_year);
        let mut pg = g.clone();
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(1);
        let now = LocalTime::HARNESS;
        let skip = [0x65db62];
        let env = Env::skipping(&now, &skip);
        deliver_phys_commod_for_short_futures(&mut pg, &mut ui, &mut rng, &env, &st(&rec));
        let Some(o) = run(0x664ce0, &[&format!("s:{rec}")], &g, 1, &[], &[(0x65db62, 5)]) else { return };
        assert_same_skips(seed, &env.skipped(), &o);
        cmp().assert(&pg, &o.globals);
    });
}
