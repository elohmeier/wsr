//! Differential tests of `logic::assets::buy`: BuyBusinessAssets (0x5d2f04) and its GOSUBs.
#![allow(unused_imports)]
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use super::script::*;
use std::sync::Mutex;
use wsr_core::logic::assets::buy::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

const BUY: u32 = 0x5d2f04;

/// Routines skipped in the original: none (BorrowToCoverPurchase, TransferEtfContracts and BuyStockMenu
/// run on both sides).
pub const SKIPS: [(u32, u32); 0] = [];

/// A world for the asset purchase: the human player 1 acts with `ActiveEntity` (an operating company of
/// industry `ind`, sometimes a holding company, bank or fund), other companies of the industry with
/// various controllers and sizes, selections, the Board's watch list and some discounted asset offers.
/// Returns the world and the industry.
pub fn buy_world(seed: u64) -> (Globals, i16) {
    let __content = wsr_core::content::current();
    let mut g = world(seed);
    let mut r = Rand(seed.wrapping_mul(6151) + 77);
    g.cur_player = 1;
    g.is_computer_player[1] = 0;
    g.cur_player_name = g.player_name[1].clone();
    g.sel_entity = 0;
    g.last_sel_entity = 0;
    // uncontrolled corps control themselves (CorpController 0 is unusual; see the error-9 test seeds)
    if !seed.is_multiple_of(11) {
        for e in 11..=1600 {
            if g.corp_controller[e] == 0 {
                g.corp_controller[e] = e;
            }
        }
    }
    for p in 1..=5 {
        if p > g.num_players {
            g.is_computer_player[p] = 0;
        }
    }
    let ind = cast::<i16, _>([5, 12, 20, 33, 37, 48, 60][cast::<usize, _>(r.range(0, 6))]);
    let ae = cast::<i32, _>(r.range(131, 1580));
    g.active_entity = ae;
    g.entity_status[ae] = 1;
    g.active_entity_name = g.ent_name[ae].clone();
    g.industry_num[ae] = ind;
    g.corp_controller[ae] = 1;
    g.business_assets[ae] = match r.range(0, 3) {
        0 => f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(0.9)),
        1 => f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(200.0)),
        _ => f64r(&mut r, Ext::from_f64(200.0), Ext::from_f64(3000.0)),
    };
    g.cash[ae] = match r.range(0, 4) {
        0 => f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(60.0)),
        1 => f64r(&mut r, Ext::from_f64(60.0), Ext::from_f64(300.0)),
        _ => f64r(&mut r, Ext::from_f64(300.0), Ext::from_f64(4000.0)),
    };
    match seed % 10 {
        0..=2 => {
            g.industry_num[ae] = 70;
            g.tax_loss_carry[ae] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { -f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(60.0)) } else { Ext::from_f64(0.0) };
            g.tax_credits[ae] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(30.0)) } else { Ext::from_f64(0.0) };
        }
        3 if seed % 30 == 3 => {
            let b = cast::<i32, _>(r.range(11, 130));
            g.active_entity = b;
            g.entity_status[b] = 1;
            g.active_entity_name = g.ent_name[b].clone();
        }
        _ => {}
    }
    // player 1 holds the acting company (so that a closing UpdateMarketDisplay keeps it the ActiveEntity)
    let actor = g.active_entity;
    if actor > 10 && g.corp_controller[actor] == 1 {
        for e in 1..=1600 {
            for s in 1..=15 {
                if cast::<i32, _>(g.hold_stock_id[(s, e)]) == actor {
                    g.hold_stock_id[(s, e)] = 0;
                    g.hold_stock_pct[(s, e)] = 0;
                }
            }
        }
        g.hold_stock_id[(1, 1)] = cast::<i16, _>(actor);
        g.hold_stock_pct[(1, 1)] = 70;
        g.hold_stock_basis[(1, 1)] = Ext::from_f64(100.0);
        g.hold_stock_id[(0, 1)] = cast::<i16, _>((1..=15).filter(|&s| g.hold_stock_id[(s, 1)] != 0).count());
    }
    // the industry's other companies
    let n = if seed.is_multiple_of(13) { 0 } else if seed % 13 == 1 { 1 } else { r.range(2, 12) };
    let mut members = Vec::new();
    for _ in 0..n {
        let c = cast::<i32, _>(r.range(131, 1580));
        if c == ae {
            continue;
        }
        g.entity_status[c] = 1;
        g.industry_num[c] = ind;
        g.corp_controller[c] = match r.range(0, 9) {
            0 | 1 => 1,
            2..=4 => c,
            5 => 2,
            6 if seed.is_multiple_of(11) => 0,
            _ => cast::<i32, _>(r.range(131, 1580)),
        };
        g.business_assets[c] = match r.range(0, 3) {
            0 => f64r(&mut r, Ext::from_f64(10.0), Ext::from_f64(240.0)),
            1 => f64r(&mut r, Ext::from_f64(250.0), Ext::from_f64(1000.0)),
            _ => f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(4000.0)),
        };
        g.profit_margin[c] = cast::<i16, _>(r.range(-5, 25));
        members.push(c);
    }
    // the industry's size (assets of companies not in the list above)
    for c in 131..=1600 {
        if g.industry_num[c] == ind && !members.contains(&c) && c != ae && (seed % 13 <= 1 || r.chance(cast::<f64, _>(Ext::from_f64(0.7)))) {
            g.industry_num[c] = 3;
        }
    }
    if !members.is_empty() {
        let pick = |r: &mut Rand| members[cast::<usize, _>(r.range(0, cast::<i64, _>(members.len()) - 1))];
        if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
            g.sel_entity = pick(&mut r);
        } else if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
            g.sel_entity = g.active_entity;
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            g.last_sel_entity = pick(&mut r);
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
            g.declined_seller_corp = pick(&mut r);
        }
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        g.asset_buy_watch_list[cast::<i32, _>(r.range(1, 50))] = g.active_entity;
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) {
        for k in 1..=50 {
            g.asset_buy_watch_list[k] = cast::<i32, _>(r.range(131, 1600));
        }
    }
    g.antitrust_cheat_used = if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) { 1 } else { 0 };
    g.ind_demand[cast::<i32, _>(ind)] = r.range(2000, 60000);
    g.ind_demand_gap[cast::<i32, _>(ind)] = cast::<f64, _>(f64r(&mut r, -Ext::from_f64(10.0), Ext::from_f64(30.0)));
    g.asset_warn_declined_ind = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { ind } else { 0 };
    // discounted asset offers
    let k = cast::<i32, _>(if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { r.range(1, 4) } else { 0 });
    for i in 1..=k {
        let seller = if !members.is_empty() && r.chance(cast::<f64, _>(Ext::from_f64(0.7))) {
            members[cast::<usize, _>(r.range(0, cast::<i64, _>(members.len()) - 1))]
        } else {
            cast::<i32, _>(r.range(131, 1580))
        };
        g.asset_sale_offers[i] = st(&format!("{seller}{__wsr_text_13580}{}|S|4|{}", r.range(20, 400), g.cur_year + 1,__wsr_text_13580=__content.string(13580)));
    }
    g.asset_sale_offers[0] = st(&k.to_string());
    if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
        g.currency_symbol = st(["", "EUR", "Fr", "$"][cast::<usize, _>(r.range(0, 3))]);
        g.currency_suffix = st([" Euro", "", " Fr"][cast::<usize, _>(r.range(0, 2))]);
    }
    (g, ind)
}

pub fn answer_pool(r: &mut Rand) -> String {
    match r.range(0, 15) {
        12 | 13 => "dflt".into(),
        14 => format!("frac:{}", [Ext::from_f64(0.3), Ext::from_f64(0.5), Ext::from_f64(0.9)][cast::<usize, _>(r.range(0, 2))]),
        0..=3 => "IDYES".into(),
        4 | 5 => "IDNO".into(),
        6 => "IDCANCEL".into(),
        7 => "0".into(),
        8 => r.range(1, 99).to_string(),
        9 | 10 => r.range(100, 600).to_string(),
        _ => r.range(600, 5000).to_string(),
    }
}

/// Hits: titles, message texts and report titles of the port's run.
fn record(hits: &Mutex<Vec<String>>, ui: &Script) {
    let mut h = hits.lock().unwrap();
    h.extend(ui.titles.iter().cloned());
    h.extend(ui.msgs.iter().cloned());
    h.extend(ui.reports.iter().cloned());
    h.extend(ui.input_titles.iter().cloned());
}

fn assert_hits(hits: Mutex<Vec<String>>, starts: &[&str], contains: &[&str]) {
    let h = hits.into_inner().unwrap();
    for want in starts {
        assert!(h.iter().any(|t| t.starts_with(want)), "branch not hit: {want}");
    }
    for want in contains {
        assert!(h.iter().any(|t| t.contains(want)), "branch not hit: ..{want}..");
    }
}

#[test]
fn diff_buy_business_assets() {
    let __content = wsr_core::content::current();
    let hits = Mutex::new(Vec::new());
    par_seeds(seeds(1..=360), |seed| {
        let (g, _) = buy_world(seed);
        let mut r = Rand(seed * 37 + 11);
        let mut answers = Vec::new();
        for _ in 0..r.range(4, 12) {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.15))) {
                answers.push(format!("pick:{}", r.range(0, 30)));
            } else {
                answers.push(answer_pool(&mut r));
            }
        }
        let Some((ui, _)) = check(seed, BUY, &["0".to_string()], &g, &answers, &SKIPS, |g, ui, rng| {
            buy_business_assets(g, ui, rng, 0, &wsr_core::logic::newgame::LocalTime::HARNESS)
        }) else {
            return;
        };
        record(&hits, &ui);
    });
    assert_hits(
        hits,
        &[
            __content.string(1921),
            __content.string(1928),
            __content.string(1939),
            __content.string(1962),
            __content.string(1971),
            __content.string(2006),
            __content.string(2013),
            __content.string(1943),
            __content.string(2060),
            __content.string(2072),
            __content.string(2074),
            __content.string(13581),
            __content.string(2042),
            __content.string(2034),
            "Error number 9",
            __content.string(11887),
        ],
        &[
            __content.string(1991),
            __content.string(1935),
            __content.string(2011),
            __content.string(13582),
            __content.string(13583),
            __content.string(2033),
            __content.string(13584),
            __content.string(13585),
        ],
    );
}

/// BuyBusinessAssets with a given seller (`seller_id` ≥ 131, as SellBusinessAssets calls it).
#[test]
fn diff_buy_business_assets_from_seller() {
    let __content = wsr_core::content::current();
    let hits = Mutex::new(Vec::new());
    par_seeds(seeds(1..=160), |seed| {
        let (mut g, ind) = buy_world(seed + 1000);
        let mut r = Rand(seed * 41 + 3);
        let mut seller = cast::<i32, _>(r.range(131, 1580));
        if r.chance(cast::<f64, _>(Ext::from_f64(0.8))) {
            let cands: Vec<i32> =
                (131..=1580).filter(|&c| g.industry_num[c] == ind && g.entity_status[c] == 1 && c != g.active_entity).collect();
            if !cands.is_empty() {
                seller = cands[cast::<usize, _>(r.range(0, cast::<i64, _>(cands.len()) - 1))];
            }
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) {
            seller = g.active_entity;
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) && seller > 130 {
            g.corp_controller[seller] = 1;
        }
        let mut answers = Vec::new();
        for _ in 0..r.range(3, 8) {
            answers.push(if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) { "dflt".to_string() } else { answer_pool(&mut r) });
        }
        let args = [seller.to_string()];
        let Some((ui, _)) = check(seed, BUY, &args, &g, &answers, &SKIPS, |g, ui, rng| {
            buy_business_assets(g, ui, rng, seller, &wsr_core::logic::newgame::LocalTime::HARNESS)
        }) else {
            return;
        };
        record(&hits, &ui);
    });
    assert_hits(
        hits,
        &[
            __content.string(2030),
            __content.string(2013),
            __content.string(2060),
            __content.string(13586),
            __content.string(2016),
        ],
        &[__content.string(13587), __content.string(2062)],
    );
}
