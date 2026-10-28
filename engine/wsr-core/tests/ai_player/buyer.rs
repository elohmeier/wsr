//! Differential tests of AiFindAffordableBuyer (0x5d1bc6) and AddRecentOffering (0x498ad8).
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use wsr_core::logic::ai_player::{add_recent_offering, ai_find_affordable_buyer};
use wsr_core::pb::rng::Rng;

const FIND_BUYER: u32 = 0x5d1bc6;
const ADD_RECENT: u32 = 0x498ad8;

#[test]
fn diff_ai_find_affordable_buyer() {
    let __content = wsr_core::content::current();
    let found = std::sync::atomic::AtomicU32::new(0);
    par_seeds(1..=24, |seed| {
        let mut g = ai_world(seed + 300);
        let mut r = Rand(seed * 911 + 3);
        // candidates need CorpController = 1 (sic): give player 1 some rich companies
        for _ in 0..r.range(0, 40) {
            let c = active_in(&g, &mut r, 11, 1580);
            g.corp_controller[c] = 1;
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                g.cash[c] = f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(100000.0));
            }
        }
        let target = match seed % 6 {
            0 => cast::<i32, _>(r.range(-3, 1700)),
            1 => g.ai_target_corp.max(11),
            _ => active_corp(&g, &mut r),
        };
        if seed % 5 == 2 {
            g.asbestos_corp = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { -target } else { target };
        }
        if seed % 7 == 3 {
            g.superfund_corp = cast::<i16, _>(target);
        }
        g.ai_target_corp = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { active_corp(&g, &mut r) } else { 0 };
        g.market_cap[target.clamp(1, 1600)] = [f64r(&mut r, Ext::from_f64(0.1), Ext::from_f64(5.0)), f64r(&mut r, Ext::from_f64(5.0), Ext::from_f64(100.0)), f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(3000.0))][cast::<usize, _>(r.range(0, 2))];
        let pct = [0, 1, 5, 10, 25, 51][cast::<usize, _>(r.range(0, 5))];
        let toxic = cast::<i32, _>(r.range(0, 1));
        let rs = cast::<u32, _>(r.next());
        let args = [target.to_string(), pct.to_string(), toxic.to_string()];
        let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let Some(o) = run(FIND_BUYER, &args, &g, rs, &[], &[]) else { return };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        let res = ai_find_affordable_buyer(&mut port, &mut ui, &mut rng, target, pct, toxic);
        let what = format!("seed {seed}{__wsr_text_13552}{target} pct {pct}{__wsr_text_13553}{toxic}",__wsr_text_13552=__content.string(13552), __wsr_text_13553=__content.string(13553));
        if o.log.iter().any(|l| l.starts_with("PBERR")) {
            eprintln!("{what}: runtime error in the original, not compared");
            return;
        }
        cmp().rel_tol(Ext::from_f64(1e-12)).assert(&port, &o.globals);
        assert_eq!(cast::<u32, _>(res), o.return_value, "{what}: result");
        assert_eq!(rng.state(), o.rng, "{what}: RND");
        if res > 0 {
            found.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    });
    if super::diff::unavailable().is_none() {
        assert!(found.into_inner() >= 2, "too few seeds found a buyer");
    }
}

#[test]
fn diff_add_recent_offering() {
    par_seeds(1..=6, |seed| {
        let mut g = ai_world(seed);
        let mut r = Rand(seed);
        for i in 1..=10 {
            g.recent_offerings[i] = cast::<i32, _>(r.range(-1600, 1600));
        }
        let corp = cast::<i32, _>(r.range(11, 1600));
        let typ = cast::<i32, _>(seed % 4);
        let Some(o) = run(ADD_RECENT, &[&corp.to_string(), &typ.to_string()], &g, 1, &[], &[]) else { return };
        let mut port = g.clone();
        add_recent_offering(&mut port, corp, typ);
        cmp().assert(&port, &o.globals);
    });
}
