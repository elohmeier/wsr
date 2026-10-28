//! Differential tests of `logic::sim_entity::asset_sales`.
#![allow(unused_imports)]
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use wsr_core::logic::sim_entity::asset_sales::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

const GEN_DESC: u32 = 0x4e1095;

#[allow(non_snake_case)]
fn COUNTRIES(__content: &wsr_core::content::ContentPack) -> [&str; 12] { [
    "GERMANY", "FRANCE", "SPAIN", "JAPAN", __content.string(14151), "AUSTRALIA", "KOREA", "SWEDEN", "BRAZIL", "CANADA",
    "INDIA", "MEXICO",
] }

/// The common world plus country names / display names (some of the special-cased ones).
fn asset_world(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut g = world(seed);
    let mut r = Rand(seed ^ 0x5151);
    for i in 0..=55 {
        if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
            g.country_name[i] = st(COUNTRIES(&__content)[cast::<usize, _>(r.range(0, 11))]);
        }
        g.country_display_name[i] = if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
            st(COUNTRIES(&__content)[cast::<usize, _>(r.range(0, 11))])
        } else {
            word(&mut r, 10)
        };
    }
    g
}

fn rng_of(seed: u64) -> u32 {
    cast::<u32, _>(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 20)
}

/// An RND state derived from `seed` whose first `RND` satisfies `ok` (to get past a random gate).
fn rng_where(seed: u64, ok: impl Fn(Ext) -> bool) -> u32 {
    let mut s = rng_of(seed);
    loop {
        if ok(Ext::from(Rng::from_state(s).rnd())) {
            return s;
        }
        s = s.wrapping_mul(69069).wrapping_add(1);
    }
}

#[test]
fn diff_gen_asset_sale_description() {
    par_seeds(1..=24, |seed| {
        let mut g = asset_world(seed);
        let mut r = Rand(seed * 77 + 3);
        for k in 0..6 {
            let e = cast::<i32, _>(r.range(131, 1600));
            if k % 3 == 0 {
                g.country_idx[e] = [0, 6, 1, 5][cast::<usize, _>(r.range(0, 3))];
            }
            if k == 5 {
                g.industry_num[e] = cast::<i16, _>(r.range(0, 71));
            }
            g.rand_seed = cast::<i32, _>(r.range(0, 30000));
            let rng0 = rng_of(seed * 10 + k);
            let Some(o) = run(GEN_DESC, &[&e.to_string()], &g, rng0, &[], &[]) else { return };
            let mut port = g.clone();
            let mut rng = Rng::from_state(rng0);
            gen_asset_sale_description(&mut port, &mut rng, e);
            cmp().assert(&port, &o.globals);
            assert_eq!(rng.state(), o.rng, "rng seed {seed} k {k}");
            g = o.globals;
        }
    });
}

const BUY_BA: u32 = 0x66c796;
const BUY_STOCK: u32 = 0x66d13a;
const OFFER: u32 = 0x66b218;

/// A world with an asset-sale offer list: business-asset offers ("seller|CA|amount|S|q|y") by corps of
/// the buyer's industry and stock offers ("seller|stock|pct|S|q|y").
fn offers_world(seed: u64) -> (Globals, i32) {
    let __content = wsr_core::content::current();
    let mut g = asset_world(seed);
    let mut r = Rand(seed * 31 + 7);
    let buyer = cast::<i32, _>(r.range(131, 1580));
    g.sim_co_count = buyer;
    g.entity_status[buyer] = 1;
    let ind = cast::<i16, _>(r.range(3, 40));
    g.industry_num[buyer] = ind;
    g.ind_demand_gap[cast::<i32, _>(ind)] = cast::<f64, _>(cast::<Ext, _>(r.range(-3, 15)));
    g.business_assets[buyer] = f64r(&mut r, Ext::from_f64(10.0), Ext::from_f64(3000.0));
    g.ind_demand[cast::<i32, _>(ind)] = r.range(5000, 80000);
    g.liquid_funds = f64r(&mut r, Ext::from_f64(20.0), Ext::from_f64(5000.0));
    g.lender_id = cast::<i32, _>(r.range(11, 85));
    g.active_entity = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { buyer } else { cast::<i32, _>(r.range(1, 1600)) };
    let n = cast::<i32, _>(if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) { r.range(0, 3) } else { r.range(3, 14) });
    let mut k = 0;
    for _ in 0..n {
        k += 1;
        let seller = if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) { buyer } else { cast::<i32, _>(r.range(131, 1580)) };
        let (eq, eqy) = (r.range(1, 4), g.cur_year + cast::<i32, _>(r.range(0, 1)));
        if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) {
                g.industry_num[seller] = ind;
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
                g.industry_num[seller] = 70;
            }
            let amt = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { cast::<i64, _>(g.business_assets[seller]) + r.range(0, 50) } else { r.range(5, 600) };
            g.asset_sale_offers[k] = st(&format!("{seller}{__wsr_text_13580}{amt}{__wsr_text_13588}{eq}|{eqy}",__wsr_text_13580=__content.string(13580), __wsr_text_13588=__content.string(13588)));
        } else {
            let stock = cast::<i32, _>(r.range(11, 1600));
            g.asset_sale_offers[k] = st(&format!("{seller}|{stock}|{}{__wsr_text_13588}{eq}|{eqy}", r.range(1, 60),__wsr_text_13588=__content.string(13588)));
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
            g.corp_controller[seller] = g.cur_player;
        }
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.15))) && k > 0 {
        g.asset_sale_offers[cast::<i32, _>(r.range(1, cast::<i64, _>(k)))] = PbStr::new();
    }
    g.asset_sale_offers[0] = st(&k.to_string());
    (g, buyer)
}

#[test]
fn diff_ai_buy_offered_business_assets() {
    par_seeds(1..=60, |seed| {
        let (g, _) = offers_world(seed);
        let rng0 = if seed % 5 == 0 { rng_of(seed) } else { rng_where(seed, |x| x * Ext::from_f64(2.0) >= Ext::from_f64(1.0)) };
        let Some(o) = run(BUY_BA, &[], &g, rng0, &[], &[]) else { return };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0);
        let mut ui = Rec::default();
        let rv = ai_buy_offered_business_assets(&mut port, &mut ui, &mut rng);
        cmp().assert(&port, &o.globals);
        assert_eq!(rng.state(), o.rng, "rng seed {seed}");
        assert_eq!(cast::<u32, _>(rv), o.return_value, "result seed {seed}");
        assert_eq!(ui.msgs.len(), orig_msgs(&o), "msgs seed {seed}");
    });
}

#[test]
fn diff_ai_offer_assets_for_sale() {
    par_seeds(1..=80, |seed| {
        let (mut g, corp) = offers_world(seed);
        let mut r = Rand(seed * 13 + 5);
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            // stock holdings of the corp
            let n = cast::<i32, _>(r.range(1, 6));
            for s in 1..=15 {
                g.hold_stock_id[(s, corp)] = 0;
                g.hold_stock_pct[(s, corp)] = 0;
            }
            for s in 1..=n {
                g.hold_stock_id[(s, corp)] = cast::<i16, _>(r.range(11, 1600));
                g.hold_stock_pct[(s, corp)] = cast::<i16, _>(r.range(1, 100));
            }
            g.hold_stock_id[(0, corp)] = cast::<i16, _>(n);
        }
        g.profit_margin[corp] = cast::<i16, _>(r.range(-10, 10));
        g.credit_rating_num[corp] = cast::<i32, _>(r.range(1, 9));
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.corp_controller[corp] = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
        }
        autopilot(&mut g, corp, &mut r);
        let rng0 = rng_of(seed);
        let (answers, ui_answers) = answers_for(seed);
        let Some(o) = run(OFFER, &[], &g, rng0, answers, &[]) else { return };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0);
        let mut ui = Rec { answers: ui_answers, ..Rec::default() };
        ai_offer_assets_for_sale(&mut port, &mut ui, &mut rng, 0);
        cmp().assert(&port, &o.globals);
        assert_eq!(rng.state(), o.rng, "rng seed {seed}");
        assert_eq!(ui.msgs.len(), orig_msgs(&o), "msgs seed {seed}");
    });
}

/// [`offers_world`] with stock offers whose sellers hold the stock, some of them of stocks the buyer
/// holds, small caps and creditworthy targets.
fn stock_offers_world(seed: u64) -> (Globals, i32) {
    let (mut g, buyer) = offers_world(seed);
    let mut r = Rand(seed * 101 + 9);
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        g.industry_num[buyer] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { 71 } else { cast::<i16, _>(r.range(1, 2)) };
        if g.industry_num[buyer] == 71 && r.chance(cast::<f64, _>(Ext::from_f64(0.7))) {
            g.sim_co_count = cast::<i32, _>(r.range(1586, 1600));
            let b = g.sim_co_count;
            g.entity_status[b] = 1;
            g.industry_num[b] = 71;
            g.fund_adviser[b] = cast::<i32, _>(r.range(86, 1580));
        }
    }
    let buyer = g.sim_co_count;
    let n = to_n(&g);
    for k in 1..=n {
        let rec = g.asset_sale_offers[k].to_display();
        let f: Vec<&str> = rec.split('|').collect();
        if f.len() < 6 || f[1] == "CA" {
            continue;
        }
        let seller: i32 = f[0].parse().unwrap();
        let mut stock: i32 = f[1].parse().unwrap();
        if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) && g.hold_stock_id[(0, buyer)] > 0 {
            let s = cast::<i32, _>(r.range(1, 15));
            if g.hold_stock_id[(s, buyer)] > 0 {
                stock = cast::<i32, _>(g.hold_stock_id[(s, buyer)]);
            }
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            g.market_cap[stock] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(60.0));
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
            g.credit_rating_num[stock] = cast::<i32, _>(r.range(2, 9));
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.85))) {
            let s = cast::<i32, _>(r.range(1, 15));
            g.hold_stock_id[(s, seller)] = cast::<i16, _>(stock);
            g.hold_stock_pct[(s, seller)] = cast::<i16, _>(r.range(1, 70));
            g.hold_stock_basis[(s, seller)] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(500.0));
        }
        g.asset_sale_offers[k] = st(&format!("{seller}|{stock}|{}|{}|{}|{}", f[2], f[3], f[4], f[5]));
    }
    (g, buyer)
}

fn to_n(g: &Globals) -> i32 {
    g.asset_sale_offers[0].to_display().trim().parse().unwrap_or(0)
}

#[test]
fn diff_ai_buy_offered_stock() {
    par_seeds(1..=80, |seed| {
        let (mut g, buyer) = stock_offers_world(seed);
        let mut r = Rand(seed * 7 + 3);
        autopilot(&mut g, buyer, &mut r);
        let rng0 = if seed % 5 == 0 { rng_of(seed) } else { rng_where(seed, |x| x * Ext::from_f64(3.0) <= Ext::from_f64(1.0)) };
        let (answers, ui_answers) = answers_for(seed);
        let Some(o) = run(BUY_STOCK, &[], &g, rng0, answers, &[]) else { return };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0);
        let mut ui = Rec { answers: ui_answers, ..Rec::default() };
        ai_buy_offered_stock(&mut port, &mut ui, &mut rng, 0);
        cmp().assert(&port, &o.globals);
        assert_eq!(rng.state(), o.rng, "rng seed {seed}");
        assert_eq!(ui.msgs.len(), orig_msgs(&o), "msgs seed {seed}");
    });
}

const START_CORP: u32 = 0x4f21e7;
const ASK_PERM: u32 = 0x4be9a8;

/// Sometimes makes `corp` a company of the (human) current player on autopilot, so that
/// AskAutopilotPermission asks.
fn autopilot(g: &mut Globals, corp: i32, r: &mut Rand) {
    if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
        g.cur_player = 1;
        g.is_computer_player[1] = 0;
        g.corp_controller[corp] = 1;
        g.auto_pilot[corp] = 1;
        g.player_ceo_of[1] = 0;
        g.perm_ask_quarter = if r.chance(cast::<f64, _>(Ext::from_f64(0.8))) { 0 } else { g.cur_quarter };
        g.perm_ask_year = g.cur_year - cast::<i32, _>(r.range(0, 1));
        g.perm_ask_corp = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { corp } else { 0 };
    }
}

/// MSGBOX answers for the original and the port (OK for the notice, then YES / NO / CANCEL).
fn answers_for(seed: u64) -> (&'static [&'static str], std::collections::VecDeque<i32>) {
    match seed % 3 {
        0 => (&["IDOK", "IDYES"], [1, 6].into()),
        1 => (&["IDOK", "IDNO"], [1, 7].into()),
        _ => (&["IDOK", "IDCANCEL"], [1, 2].into()),
    }
}

#[test]
fn diff_ask_autopilot_permission() {
    par_seeds(1..=36, |seed| {
        let mut g = asset_world(seed);
        let mut r = Rand(seed * 41 + 2);
        let corp = cast::<i32, _>(r.range(131, 1580));
        let target = cast::<i32, _>(r.range(11, 1600));
        g.entity_status[corp] = 1;
        autopilot(&mut g, corp, &mut r);
        if seed % 4 != 0 {
            g.cur_player = 1;
            g.is_computer_player[1] = 0;
            g.corp_controller[corp] = 1;
            g.auto_pilot[corp] = 1;
            g.player_ceo_of[1] = 0;
            g.perm_ask_quarter = 0;
        }
        let pct = cast::<i16, _>(r.range(1, 60));
        let owned = cast::<i32, _>(r.range(0, 40));
        let action = cast::<i32, _>(r.range(0, 3));
        let rng0 = rng_of(seed);
        let (answers, ui_answers) = answers_for(seed);
        let args = [corp.to_string(), target.to_string(), format!("i:{pct}"), owned.to_string(), action.to_string()];
        let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let Some(o) = run(ASK_PERM, &args, &g, rng0, answers, &[]) else { return };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0);
        let mut ui = Rec { answers: ui_answers, ..Rec::default() };
        let rv = ask_autopilot_permission(&mut port, &mut ui, &mut rng, corp, target, pct, owned, action, 0);
        cmp().assert(&port, &o.globals);
        assert_eq!(rng.state(), o.rng, "rng seed {seed}");
        assert_eq!(cast::<u32, _>(rv), o.return_value, "result seed {seed}");
        assert_eq!(ui.msgs.len(), orig_msgs(&o), "msgs seed {seed}");
        let orig_texts: Vec<&String> = o.log.iter().filter(|l| l.starts_with("MSGBOX")).collect();
        for (m, l) in ui.msgs.iter().zip(&orig_texts) {
            assert!(l.contains(&m.replace('\r', "\\r")), "msg text: {m:?} vs {l}");
        }
    });
}

#[test]
fn diff_start_new_corp_or_expand() {
    let __content = wsr_core::content::current();
    par_seeds(1..=40, |seed| {
        let mut g = asset_world(seed);
        let mut r = Rand(seed * 17 + 1);
        let industry = cast::<i16, _>(r.range(3, 69));
        let mode = if seed % 3 == 0 { 1 } else { 0 };
        g.active_corp_count = cast::<i32, _>(r.range(900, 1400));
        g.ind_demand[cast::<i32, _>(industry)] = r.range(5000, 200000);
        if mode == 1 {
            g.tmp_ext1 = f64r(&mut r, Ext::from_f64(50.0), Ext::from_f64(3000.0));
        }
        // headlines that may block mode 0
        for i in 1..=60 {
            g.news_headline[i] = if r.chance(cast::<f64, _>(Ext::from_f64(0.01))) { st("XYZ expanding rapidly in WIDGETS") } else { word(&mut r, 20) };
        }
        // a few free / shell slots
        for _ in 0..r.range(0, 40) {
            let s = cast::<i32, _>(r.range(131, 1580));
            g.entity_status[s] = 0;
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                g.industry_num[s] = industry;
            }
        }
        for _ in 0..r.range(0, 10) {
            let s = cast::<i32, _>(r.range(131, 1580));
            g.industry_num[s] = 70;
            g.corp_controller[s] = s;
        }
        let rng0 = rng_of(seed);
        let args = [format!("{__wsr_text_13653}{industry}",__wsr_text_13653=__content.string(13653)), mode.to_string()];
        let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let Some(o) = run(START_CORP, &args, &g, rng0, &[], &[]) else { return };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0);
        let mut ui = Rec::default();
        let rv = start_new_corp_or_expand(&mut port, &mut ui, &mut rng, industry, mode, 0);
        cmp().assert(&port, &o.globals);
        assert_eq!(rng.state(), o.rng, "rng seed {seed}");
        assert_eq!(cast::<u32, _>(rv), o.return_value, "result seed {seed}");
        assert_eq!(ui.msgs.len(), orig_msgs(&o), "msgs seed {seed}");
    });
}
