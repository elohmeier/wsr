//! Differential tests of `logic::sim_entity::acquisitions`: AiSellOrMergeHolding, AiCreateOptionPosition
//! and FindOptionCounterparty against the original on random plausible worlds.
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use wsr_core::logic::sim_entity::acquisitions::*;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

const AI_SELL_OR_MERGE: u32 = 0x4c64ef;

fn rng0(seed: u64) -> u32 {
    (cast::<u32, _>(seed)).wrapping_mul(0x9e37_79b1) ^ 0x1234_5678
}

/// `world(seed)` plus a few option records and credit ratings spread over 1..10.
fn opt_world(seed: u64) -> Globals {
    let mut g = world(seed);
    let mut r = Rand(seed ^ 0xabcd);
    let n = cast::<i32, _>(r.range(0, 40));
    for i in 1..=n {
        let corp = r.range(131, 1580);
        let holder = r.range(131, 1580);
        let q = r.range(1, 9) * if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { -1 } else { 1 };
        let strike = f64r(&mut r, Ext::from_f64(5.0), Ext::from_f64(3000.0));
        g.option_recs[i] = st(&format!(
            "{corp}|{}| {holder}|{}{q}| {strike}| {}| {}| {}",
            r.range(1, 2),
            if q < 0 { "" } else { " " },
            r.range(1, 12),
            g.cur_year + cast::<i32, _>(r.range(0, 1)),
            f64r(&mut r, Ext::from(Ext::from_f64(0.0)), Ext::from(Ext::from_f64(200.0)))
        ));
    }
    g.option_recs[0] = st(&format!(" {n}"));
    for e in 1..=1600 {
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            g.credit_rating_num[e] = cast::<i32, _>(r.range(1, 10));
        }
    }
    g
}

#[track_caller]
fn check(port: &Globals, rng: &Rng, ui: &Rec, o: &Original, what: &str) {
    let __content = wsr_core::content::current();
    if std::env::var("DBG").is_ok() {
        for l in o.log.iter().filter(|l| !l.starts_with("API")) {
            eprintln!("{what}: {l}");
        }
        eprintln!("{what}: port msgs {:?}{__wsr_text_13611}{:?}", ui.msgs, ui.news,__wsr_text_13611=__content.string(13611));
    }
    cmp().rel_tol(Ext::from_f64(1e-9)).assert(port, &o.globals);
    assert_eq!(rng.state(), o.rng, "{what}: RND state");
    assert_eq!(
        ui.msgs.len(),
        orig_msgs(o),
        "{what}: message boxes {:?} vs {:?}",
        ui.msgs,
        o.log
    );
}

fn report(what: &str, n: &AtomicUsize) {
    let __content = wsr_core::content::current();
    let k = n.load(Ordering::Relaxed);
    eprintln!("{__wsr_text_13574}{what}: {k}",__wsr_text_13574=__content.string(13574));
    if unavailable().is_none() {
        assert!(k > 0, "no seed reached: {what}");
    }
}

#[test]
fn diff_ai_sell_or_merge_holding() {
    let __content = wsr_core::content::current();
    let (divest, sell, opt) = (
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
    );
    par_seeds(1..=12, |seed| {
        let mut g = opt_world(seed);
        let mut r = Rand(seed * 31 + 7);
        let mut list = Vec::new();
        for h in 131..=1583 {
            if g.entity_status[h] != 1 {
                continue;
            }
            for slot in 1..=15 {
                let c = cast::<i32, _>(g.hold_stock_id[(slot, h)]);
                if c > 10 && c != h && g.entity_status[c] == 1 && g.hold_stock_pct[(slot, h)] > 0 {
                    list.push((slot, h, c));
                }
            }
        }
        let (slot, holder, held) = list[cast::<usize, _>(r.range(0, cast::<i64, _>(list.len()) - 1))];
        match seed % 4 {
            0 => {
                g.hold_stock_pct[(slot, holder)] = 100;
                g.difficulty_level = 2;
            }
            1 => {
                g.cash[holder] = g.market_cap[held] * cast::<Ext, _>(g.hold_stock_pct[(slot, holder)]) * Ext::from_f64(20.0)
            }
            2 => g.hold_stock_pct[(slot, holder)] = cast::<i16, _>(r.range(51, 90)),
            _ => {
                g.cash[holder] = Ext::from_f64(0.0);
                g.t_bills[holder] = Ext::from_f64(0.0);
            }
        }
        if seed % 4 == 2 {
            g.cash[holder] = g.market_cap[held] * Ext::from_f64(2000.0);
        }
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        ai_sell_or_merge_holding(&mut port, &mut ui, &mut rng, holder, held);
        let Some(o) = run(
            AI_SELL_OR_MERGE,
            &[&holder.to_string(), &held.to_string()],
            &g,
            rs,
            &[],
            &[],
        ) else {
            return;
        };
        check(
            &port,
            &rng,
            &ui,
            &o,
            &format!("seed {seed}{__wsr_text_13774}{holder}{__wsr_text_14145}{held}",__wsr_text_13774=__content.string(13774), __wsr_text_14145=__content.string(14145)),
        );
        if ui.news.iter().any(|n| n.contains("divests")) {
            divest.fetch_add(1, Ordering::Relaxed);
        } else if ui.news.iter().any(|n| n.contains(__content.string(14146))) {
            opt.fetch_add(1, Ordering::Relaxed);
        } else if !ui.news.is_empty() {
            sell.fetch_add(1, Ordering::Relaxed);
        }
    });
    report("AiSellOrMergeHolding option", &opt);
    report("AiSellOrMergeHolding sale", &sell);
    report("AiSellOrMergeHolding divest", &divest);
}
