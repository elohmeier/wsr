//! Differential tests of `logic::trading::futures_trade`: SelectFuturesContract (0x624c1b),
//! FindFuturesCounterparty (0x63389c) and the opening trades (`tran_type` 1) of the commodity desk's
//! FuturesBuyOrCover (0x61cebb) / FuturesSellOrShort (0x620f39).
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use super::script::*;
use std::cell::RefCell;
use wsr_core::logic::commod_ai::desk::{futures_buy_or_cover, futures_sell_or_short};
use wsr_core::logic::trading::futures_trade::*;

// The commodity world of the commod_ai tests (it refers to `super::diff` / `super::sim_common`).
use super::common as sim_common;
use super::diff;
#[allow(unused_imports)]
#[path = "../commod_ai/common.rs"]
mod ccommon;
use ccommon::{COMMODS, cworld};

const SELECT: u32 = 0x624c1b;
const COUNTERPARTY: u32 = 0x63389c;
const BUY_OR_COVER: u32 = 0x61cebb;
const SELL_OR_SHORT: u32 = 0x620f39;
/// DisableMainControls (0x4242c6): UI only.
const SKIPS: [(u32, u32); 1] = [(0x4242c6, 0)];

fn cmp9() -> Compare {
    cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-9))
}

#[test]
fn diff_select_futures_contract() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(seeds(1..=48), |seed| {
        let (mut g, _) = cworld(seed + 9900);
        let mut r = Rand(seed * 7 + 1);
        let commod = if seed % 16 == 15 { "COCOA" } else { COMMODS[cast::<usize, _>(r.range(0, 7))] };
        let mode = cast::<i32, _>(r.range(1, 2));
        // month position: sometimes exactly between (CEIL = ROUND: "Near month")
        g.sim_co_count = cast::<i32, _>([r.range(1, 1500), 260, 1250, 1000][cast::<usize, _>(r.range(0, 3))]);
        let answers: Vec<String> = match r.range(0, 4) {
            0 => vec![],
            1 => vec!["pick:0".to_string()],
            _ => vec![format!("pick:{}", r.range(1, 59))],
        };
        let args = [format!("{__wsr_text_13622}{commod}",__wsr_text_13622=__content.string(13622)), mode.to_string()];
        let out = RefCell::new(None);
        let Some((ui, _, o)) = check(seed, SELECT, &args, &g, &answers, &[], cmp9(), |g, ui, rng| {
            *out.borrow_mut() = Some(select_futures_contract(g, ui, rng, &st(commod), mode));
        }) else {
            return;
        };
        let port = out.into_inner().unwrap();
        let orig = o.return_str.clone().unwrap_or_default();
        assert_eq!(port, orig, "seed {seed}: result");
        hits.record(&ui);
        let p = port.to_display();
        hits.add(match p.as_str() {
            "" => "<none>",
            "1" => "<spot>",
            "ERROR" => "<error>",
            _ => "<month>",
        });
    });
    hits.assert(&["<none>", "<spot>", "<error>", "<month>"]);
}

#[test]
fn diff_find_futures_counterparty() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(seeds(1..=60), |seed| {
        let (mut g, _) = cworld(seed + 9950);
        let mut r = Rand(seed * 13 + 5);
        g.cur_commodity = st(COMMODS[cast::<usize, _>(r.range(0, 7))]);
        // rich, well-rated companies without futures in part of the world
        for _ in 0..r.range(0, 40) {
            let e = cast::<i32, _>(r.range(86, 1600));
            g.credit_rating_num[e] = cast::<i32, _>(r.range(7, 10));
            g.cash[e] = f64r(&mut r, Ext::from_f64(200.0), Ext::from_f64(20000.0));
        }
        let qty = r.range(1, 100) * if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { -1 } else { 1 };
        let needed = qty.abs() + if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { r.range(0, 50) } else { 0 };
        let cv = if seed % 12 == 0 { f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(5000.0)) } else { f64r(&mut r, Ext::from_f64(0.2), Ext::from_f64(150.0)) };
        let args = [format!("{__wsr_text_13664}{needed}",__wsr_text_13664=__content.string(13664)), format!("{__wsr_text_14156}{cv:?}",__wsr_text_14156=__content.string(14156)), format!("q:{qty}")];
        let out = RefCell::new(0);
        let Some((_, _, o)) = check(seed, COUNTERPARTY, &args, &g, &[], &[], cmp9(), |g, ui, rng| {
            *out.borrow_mut() = find_futures_counterparty(g, ui, rng, needed, cv, qty);
        }) else {
            return;
        };
        let port = out.into_inner();
        assert_eq!(cast::<u32, _>(port), o.return_value, "seed {seed}: counterparty");
        hits.add(if port > 0 { "<found>" } else { "<none>" });
    });
    hits.assert(&["<found>", "<none>"]);
}

/// The desk's opening trades (`tran_type` 1).
#[test]
fn diff_desk_open_positions() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(seeds(1..=200), |seed| {
        let (mut g, owners) = cworld(seed + 10_000);
        let mut r = Rand(seed * 101 + 9);
        let buy = seed % 2 == 0;
        let kind = (seed / 2) % 8;
        let e = owners[cast::<usize, _>(r.range(0, 9))];
        g.entity_status[e] = 1;
        // the desk trades for the acting entity (BorrowToCoverPurchase finances `ActiveEntity`)
        g.active_entity = e;
        g.active_entity_name = g.ent_name[e].clone();
        g.cur_commodity = st(COMMODS[cast::<usize, _>(r.range(0, 7))]);
        g.credit_rating_num[e] = if kind == 1 { cast::<i32, _>(r.range(1, 6)) } else { cast::<i32, _>(r.range(7, 10)) };
        g.cash[e] = match kind {
            2 => f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(0.5)),
            3 => f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(30.0)),
            _ => f64r(&mut r, Ext::from_f64(500.0), Ext::from_f64(50000.0)),
        };
        if kind == 2 || kind == 3 {
            g.t_bills[e] = Ext::from_f64(0.0);
        }
        if kind == 3 {
            // an expensive contract for little liquidity: "Insufficient Funds"
            g.cur_commodity = st(["BITCOIN", "STOCKS", "GOLD"][cast::<usize, _>(r.range(0, 2))]);
            g.cash[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(2.0));
            g.bank_debt[e] = f64r(&mut r, Ext::from_f64(5000.0), Ext::from_f64(50000.0));
        }
        if kind == 4 {
            // the peak-oil rules
            g.cur_commodity = st("OIL");
            g.peak_oil_crisis = if buy { 1 } else { [1, -1, -1][cast::<usize, _>(r.range(0, 2))] };
            g.oil_crisis_stage = cast::<i32, _>(r.range(10, 20));
            g.oil_crisis_level = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.1), Ext::from_f64(5.0)));
            g.oil_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(6.0)));
            g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
            g.oil_fut_buy_limit_player = [0, g.cur_player, -g.cur_player][cast::<usize, _>(r.range(0, 2))];
            g.oil_fut_short_limit_player = [0, g.cur_player, -g.cur_player][cast::<usize, _>(r.range(0, 2))];
            g.rand_seed = cast::<i32, _>(r.range(0, 30000));
            g.cash[g.cur_player] = f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(90000.0));
        }
        if kind == 7 {
            // no cash but a credit line: the outlay is borrowed (BorrowToCoverPurchase)
            g.cur_commodity = st(["OIL", "WHEAT", "CORN", "SILVER"][cast::<usize, _>(r.range(0, 3))]);
            g.cash[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(1.0));
            g.t_bills[e] = Ext::from_f64(0.0);
            g.bank_debt[e] = Ext::from_f64(0.0);
        }
        if kind == 5 {
            g.futures_recs[0] = st("998");
        }
        if kind == 6 {
            // well-funded counterparties
            for _ in 0..30 {
                let c = cast::<i32, _>(r.range(86, 1600));
                g.credit_rating_num[c] = cast::<i32, _>(r.range(7, 10));
                g.cash[c] = f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(90000.0));
            }
        }
        let pick = match r.range(0, 9) {
            0 => "IDOK".to_string(), // no pick
            1 => "pick:0".to_string(),
            _ => format!("pick:{}", r.range(1, 59)),
        };
        let qty = match if kind == 7 { 9 } else { r.range(0, 4) } {
            9 => format!("{}", r.range(40, 100)),
            0 => "0".to_string(),
            1 => format!("{}", r.range(101, 400)),
            _ => format!("{}", r.range(1, 30)),
        };
        let answers: Vec<String> = vec![
            pick,
            "IDOK".to_string(),
            qty,
            ["IDYES", "IDYES", "IDNO"][cast::<usize, _>(r.range(0, 2))].to_string(),
            "IDYES".to_string(),
            "IDOK".to_string(),
            "IDOK".to_string(),
        ];
        let args = ["1".to_string(), e.to_string(), "0".to_string()];
        let addr = if buy { BUY_OR_COVER } else { SELL_OR_SHORT };
        let n_recs = g.futures_recs[0].clone();
        let Some((ui, pg, _)) = check(seed, addr, &args, &g, &answers, &SKIPS, cmp9(), |g, ui, rng| {
            if buy {
                futures_buy_or_cover(g, ui, rng, 1, e, 0);
            } else {
                futures_sell_or_short(g, ui, rng, 1, e, 0);
            }
        }) else {
            return;
        };
        hits.record(&ui);
        if pg.futures_recs[0] != n_recs {
            hits.add(if buy { "<bought>" } else { "<shorted>" });
        }
    });
    hits.assert(&[
        "<bought>",
        "<shorted>",
        __content.string(14281),
        __content.string(14282),
        __content.string(12046),
        __content.string(12262),
        __content.string(14283),
        __content.string(14284),
        __content.string(14285),
        __content.string(12236),
        __content.string(12237),
        __content.string(12305),
        " buys ",
        __content.string(14286),
        __content.string(14287),
        __content.string(14288),
        __content.string(14289),
        __content.string(14290),
        __content.string(11887),
    ]);
}
