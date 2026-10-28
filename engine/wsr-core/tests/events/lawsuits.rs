//! Differential tests of `logic::events::lawsuits`: ProcessMaliciousLawsuits against the original.
use wsr_core::pb::real::{Ext, cast};


use super::check::*;
use super::common::*;
use super::diff::*;
use std::sync::Mutex;
use wsr_core::logic::events::*;
use wsr_core::pb::rng::Rng;

const PROCESS_MALICIOUS: u32 = 0x4b145b;

#[test]
fn diff_process_malicious_lawsuits() {
    let __content = wsr_core::content::current();
    let hits = Mutex::new(Vec::<&str>::new());
    par_seeds(1..=64, |seed| {
        let mut r = Rand(seed * 5113 + 19);
        let mut g = world(seed + 3700);
        let ticks = [135, 422, 651, 922, 1301];
        g.sim_co_count = if seed % 16 == 0 {
            136
        } else {
            ticks[cast::<usize, _>(r.range(0, 4))]
        };
        let def = if r.chance(Ext::from_f64(0.3)) {
            cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)))
        } else {
            active_corp(&g, &mut r)
        };
        let plt = active_corp(&g, &mut r);
        if r.chance(Ext::from_f64(0.3)) {
            g.player_notoriety[cast::<i32, _>(r.range(1, 5))] = cast::<i16, _>(r.range(0, 9));
            g.corp_controller[def] = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
        }
        for k in 1..=10 {
            g.lawsuit_rec[k] = Default::default();
        }
        let rec = st(&format!("2|{def}|{plt}|{}|", r.range(0, 5)));
        match seed % 4 {
            0 => {
                // a new suit (maybe behind an empty first slot)
                let slot = if r.chance(Ext::from_f64(0.3)) { 2 } else { 1 };
                g.lawsuit_rec[slot] = rec;
                if r.chance(Ext::from_f64(0.2)) {
                    g.lawsuit_rec[slot] = st(&format!("2|0|{plt}|3|"));
                }
                g.malicious_suit_timer = 0;
            }
            1 => {
                g.lawsuit_rec[1] = st(&format!("1 | {def}| {plt}| 60|100|0|"));
                g.malicious_suit_timer = 0;
            }
            _ => {
                g.malicious_suit_timer = [1, 2, 3, 5, 60, 99][cast::<usize, _>(r.range(0, 5))];
                g.malicious_suit_defendant = def;
                g.malicious_suit_plaintiff = plt;
                if r.chance(Ext::from_f64(0.15)) {
                    g.entity_status[plt] = 0;
                }
            }
        }
        if r.chance(Ext::from_f64(0.3)) {
            g.news_msg = st("Some pending headline");
        }
        if r.chance(Ext::from_f64(0.3)) {
            g.cash[def] = f64r(&mut r, -Ext::from_f64(100.0), Ext::from_f64(30.0));
        }
        let rs = cast::<u32, _>(r.next());
        let Some(o) = run(PROCESS_MALICIOUS, &[], &g, rs, &[], &[]) else {
            return;
        };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        process_malicious_lawsuits(&mut port, &mut ui, &mut rng);
        let what = format!("ProcessMaliciousLawsuits seed {seed}");
        check(&what, &port, &rng, &ui, &o);
        let mut h = hits.lock().unwrap();
        for n in &ui.news {
            if n.contains(__content.string(13823)) {
                h.push(__content.string(8070));
            }
            if n.contains(" files countersuit") {
                h.push("files");
            }
            if n.contains(" drops countersuit") {
                h.push("drops");
            }
            if n.contains(__content.string(13824)) {
                h.push("awarded");
            }
        }
        if port.malicious_suit_timer < g.malicious_suit_timer && port.malicious_suit_timer > 0 {
            h.push("countdown");
        }
    });
    let h = hits.lock().unwrap();
    if h.is_empty() {
        return;
    }
    for b in [__content.string(8070), "files", "drops", "awarded", "countdown"] {
        assert!(h.contains(&b), "branch {b} never hit: {:?}", *h);
    }
}
