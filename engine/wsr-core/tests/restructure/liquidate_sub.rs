//! Differential tests of `logic::restructure::liquidate_sub`: LiquidateSubIntoParent (all modes, kinds of
//! parent/subsidiary, bond resolutions, futures/commodity/ETF/option transfers) and the transfer routines.
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use std::sync::Mutex;
use wsr_core::logic::restructure::liquidate_sub::*;
use wsr_core::pb::rng::Rng;
use wsr_core::pb::numeric::Cur;
use wsr_core::state::Globals;

const LIQ: u32 = 0x512aa9;
const TRANSFER_FUTURES: u32 = 0x62f493;
const TRANSFER_PHYS: u32 = 0x663592;
const TRANSFER_ETF: u32 = 0x64e7d1;
/// SelectActingEntity (0x4721dc, 1 arg): owned by the trading module, PORT-TODO in the port.
const SKIPS: &[(u32, u32)] = &[];

const COMMODS: [&str; 8] = ["OIL", "GOLD", "SILVER", "WHEAT", "CORN", "STOCKS", "BITCOIN", "ETHEREUM"];

fn rng0(seed: u64) -> u32 {
    (cast::<u32, _>(seed)).wrapping_mul(2_654_435_761) ^ 0x5bd1_e995
}

fn dbg_log(o: &Original, ui: &Rec, what: &str) {
    if std::env::var("DBG").is_ok() {
        for l in o.log.iter().filter(|l| !l.starts_with("API")) {
            eprintln!("{what}: {l}");
        }
        eprintln!("{what}: port msgs {:?}\n{what}: port news {:?}", ui.msgs, ui.news);
    }
}

#[track_caller]
fn check(port: &Globals, rng: &Rng, ui: &Rec, o: &Original, what: &str) {
    dbg_log(o, ui, what);
    cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-7)).assert(port, &o.globals);
    assert_eq!(rng.state(), o.rng, "{what}: RND state");
    assert_eq!(ui.msgs.len(), orig_msgs(o), "{what}: message boxes {:?}", ui.msgs);
}

/// Adds futures records of `owner`.
fn add_futures(g: &mut Globals, r: &mut Rand, owner: i32, n: i64) {
    let mut k = (1..=1000).find(|&k| g.futures_recs[k].is_empty()).unwrap_or(1000);
    for _ in 0..n {
        let c = COMMODS[cast::<usize, _>(r.range(0, 7))];
        let qty = r.range(1, 60) * if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) { -1 } else { 1 };
        let year = g.cur_year + cast::<i32, _>(r.range(0, 3));
        let cost = f64r(r, Ext::from_f64(1.0), Ext::from_f64(2000.0));
        g.futures_recs[k] = st(&format!("{c}|{owner}|{qty}|{}|{year}|{cost:.4}|{:.2}", r.range(1, 12), f64r(r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(999.0)))));
        k += 1;
    }
    g.futures_recs[0] = st(&(k - 1).to_string());
}

fn add_phys(g: &mut Globals, r: &mut Rand, owner: i32, n: i64) {
    let k0 = (1..=1000).find(|&k| g.phys_commod_rec[k].is_empty()).unwrap_or(1000);
    for k in (k0..=1000).take(cast::<usize, _>(n)) {
        let code = ["GOLD", "SILVER", "OIL", "WHEAT", "CORN", "BITCOIN"][cast::<usize, _>(r.range(0, 5))];
        g.phys_commod_rec[k] = st(&format!(" {code}|{owner}|{}|{:.2} ", r.range(1, 5000), f64r(r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(900.0)))));
    }
}

fn add_option(g: &mut Globals, r: &mut Rand, holder: i32, corp: i32) {
    let k = (1..=1000).find(|&k| g.option_recs[k].is_empty()).unwrap_or(1000);
    let qty = r.range(1, 20) * if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { -1 } else { 1 };
    let strike = (g.market_cap[corp] * f64r(r, Ext::from_f64(0.5), Ext::from_f64(1.5)) * Ext::from_f64(100.0)).round() / Ext::from_f64(100.0);
    g.option_recs[k] = st(&format!(
        "{corp}| {}| {holder}| {qty}| {strike}| {}| {}| {:.2}",
        r.range(1, 2),
        [1, 4, 7, 10][cast::<usize, _>(r.range(0, 3))],
        g.cur_year + cast::<i32, _>(r.range(0, 1)),
        f64r(r, Ext::from(Ext::from_f64(0.1)), Ext::from(Ext::from_f64(50.0)))
    ));
    let n = (1..=1000).filter(|&k| !g.option_recs[k].is_empty()).count();
    g.option_recs[0] = st(&n.to_string());
}

fn set_bonds(g: &mut Globals, r: &mut Rand, issuer: i32, conv: bool) {
    g.bonds_issued[issuer] = r.range(50, 1500);
    let mat = g.cur_year + cast::<i32, _>(r.range(1, 25));
    let coupon = f32r(r, Ext::from_f64(2.0), Ext::from_f64(15.0));
    g.bond_issue_info[issuer] = if conv {
        st(&format!("{mat} cv|{:.2}|{coupon:5.2}", g.market_cap[issuer] * f64r(r, Ext::from(Ext::from_f64(0.5)), Ext::from(Ext::from_f64(1.5)))))
    } else {
        st(&format!("{mat}   {coupon:5.2}"))
    };
    g.bond_price[issuer] = Cur::from_f64([f64r(r, Ext::from_f64(60.0), Ext::from_f64(101.0)), f64r(r, Ext::from_f64(95.0), Ext::from_f64(125.0)), Ext::from_f64(100.0)][cast::<usize, _>(r.range(0, 2))]);
}

/// Places bonds of `issuer` with holders (a share of the issue, or the whole issue).
fn place_bonds(g: &mut Globals, r: &mut Rand, issuer: i32, all: bool) {
    let mut left = g.bonds_issued[issuer];
    let holders: Vec<i32> = (0..r.range(1, 4))
        .map(|_| match r.range(0, 3) {
            0 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
            1 => cast::<i32, _>(r.range(11, 85)),
            _ => cast::<i32, _>(r.range(86, 130)),
        })
        .collect();
    for (n, &h) in holders.iter().enumerate() {
        let face = if all && n + 1 == holders.len() { left } else { r.range(1, left.max(1)) };
        if face <= 0 {
            break;
        }
        let slot = match (1..=15).find(|&s| g.hold_bond_id[(s, h)] == 0) {
            Some(s) => s,
            None => continue,
        };
        g.hold_bond_id[(slot, h)] = issuer;
        g.hold_bond_face[(slot, h)] = face;
        g.bond_hold_cost[(slot, h)] = cast::<Ext, _>(face) * f64r(r, Ext::from_f64(0.8), Ext::from_f64(1.1));
        g.hold_bond_id[(0, h)] = cast::<i32, _>((1..=15).filter(|&s| g.hold_bond_id[(s, h)] != 0).count());
        left -= face;
    }
}

struct Case {
    g: Globals,
    parent: i32,
    sub: i32,
    mode: i32,
    answers: Vec<bool>,
}

fn pick(r: &mut Rand, g: &Globals, lo: i64, hi: i64, not: i32) -> i32 {
    loop {
        let c = cast::<i32, _>(r.range(lo, hi));
        if c != not && g.entity_status[c] == 1 {
            return c;
        }
    }
}

/// A parent/subsidiary pair of kind `seed % 7` in a random world.
fn case(seed: u64) -> Case {
    let __content = wsr_core::content::current();
    let mut g = world(seed + 7100);
    let mut r = Rand(seed * 977 + 13);
    let kind = seed % 7;
    let mode = cast::<i32, _>((seed / 7) % 3);
    let (parent, sub) = match kind {
        2 => {
            let p = pick(&mut r, &g, 11, 84, 0);
            (p, pick(&mut r, &g, 11, 84, p))
        }
        3 => {
            let p = pick(&mut r, &g, 86, 129, 0);
            (p, pick(&mut r, &g, 86, 129, p))
        }
        6 => {
            // an ETF/fund liquidated into a corp
            let s = cast::<i32, _>(r.range(1581, 1583));
            g.entity_status[s] = 1;
            (pick(&mut r, &g, 131, 1580, 0), s)
        }
        _ => {
            let p = pick(&mut r, &g, 131, 1580, 0);
            (p, pick(&mut r, &g, 131, 1580, p))
        }
    };
    match kind {
        0 => g.industry_num[sub] = g.industry_num[parent],
        1 => g.industry_num[parent] = 70,
        4 => g.industry_num[sub] = 70,
        5 => {
            g.industry_num[sub] = 37;
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                g.industry_num[parent] = 70;
            }
        }
        _ => {}
    }
    g.tmp_ext1 = cast::<Ext, _>(mode);
    g.net_worth = f64r(&mut r, -Ext::from_f64(100.0), Ext::from_f64(5000.0));
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    g.cur_player_name = st(&format!("{__wsr_text_13556}{}", g.cur_player,__wsr_text_13556=__content.string(13556)));
    g.corp_controller[parent] = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { g.cur_player } else { cast::<i32, _>(r.range(11, 1580)) };
    g.corp_controller[sub] = parent;
    // the parent's stake in the subsidiary
    for slot in 0..=15 {
        if cast::<i32, _>(g.hold_stock_id[(slot, parent)]) == sub {
            g.hold_stock_id[(slot, parent)] = 0;
        }
    }
    let slot = cast::<i32, _>(r.range(1, 15));
    g.hold_stock_id[(slot, parent)] = cast::<i16, _>(sub);
    g.hold_stock_pct[(slot, parent)] = [100, 100, 95, 85][cast::<usize, _>(r.range(0, 3))];
    g.hold_stock_basis[(slot, parent)] =
        g.market_cap[sub] * if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { f64r(&mut r, Ext::from_f64(121.0), Ext::from_f64(300.0)) } else { f64r(&mut r, Ext::from_f64(10.0), Ext::from_f64(119.0)) };
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        // the subsidiary's holdings move to the parent (empty both for a simple case)
        for s in 1..=15 {
            g.hold_stock_id[(s, sub)] = 0;
            g.hold_stock_pct[(s, sub)] = 0;
        }
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.25))) {
        // the subsidiary holds the parent's stock
        let s = cast::<i32, _>(r.range(1, 15));
        g.hold_stock_id[(s, sub)] = cast::<i16, _>(parent);
        g.hold_stock_pct[(s, sub)] = [5, 15, 30][cast::<usize, _>(r.range(0, 2))];
        g.hold_stock_basis[(s, sub)] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(300.0));
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.15))) {
        // many holdings: more than 15 combined
        for s in 1..=15 {
            if g.hold_stock_id[(s, parent)] == 0 {
                g.hold_stock_id[(s, parent)] = cast::<i16, _>(pick(&mut r, &g, 131, 1580, sub));
                g.hold_stock_pct[(s, parent)] = cast::<i16, _>(r.range(1, 30));
            }
            if g.hold_stock_id[(s, sub)] == 0 {
                g.hold_stock_id[(s, sub)] = cast::<i16, _>(pick(&mut r, &g, 131, 1580, parent));
                g.hold_stock_pct[(s, sub)] = [5, 40, 85][cast::<usize, _>(r.range(0, 2))];
            }
        }
    }
    // tax attributes
    for e in [parent, sub] {
        g.tax_loss_carry[e] = match r.range(0, 3) {
            0 => -f64r(&mut r, Ext::from_f64(30.0), Ext::from_f64(900.0)),
            1 => f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(50.0)),
            _ => Ext::from_f64(0.0),
        };
        g.tax_credits[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(60.0)) } else { Ext::from_f64(0.0) };
    }
    if kind == 2 {
        g.bank_merger_player = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { -g.cur_player } else { 0 };
        if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
            g.earnings_y1[sub] = Ext::from_f64(0.0);
            g.earnings_yr2_ago[sub] = Ext::from_f64(0.0);
            g.earnings_yr3_ago[sub] = Ext::from_f64(0.0);
            g.earnings_yr4_ago[sub] = Ext::from_f64(0.0);
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            g.country_idx[sub] = g.country_idx[parent];
        }
        for e in [parent, sub] {
            g.long_gov_bond_face[e] = r.range(0, 5000);
            g.lt_gov_bond_cost[e] = cast::<Ext, _>(g.long_gov_bond_face[e]) * Ext::from_f64(0.9);
            g.subprime_secs[e] = r.range(0, 500);
        }
        for _ in 0..5 {
            let e = cast::<i32, _>(r.range(1, 1600));
            g.lending_bank[e] = cast::<i16, _>(sub);
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            g.lending_bank[sub] = cast::<i16, _>(parent);
        }
    }
    if kind == 3 {
        g.insur_uw_profit_ytd[sub] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(50.0));
    }
    if (kind == 2 || kind == 3) && r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
        // too many bond holdings combined
        for e in [parent, sub] {
            for s in 1..=15 {
                if g.hold_bond_id[(s, e)] == 0 && r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
                    g.hold_bond_id[(s, e)] = pick(&mut r, &g, 131, 1580, 0);
                    g.hold_bond_face[(s, e)] = r.range(1, 50);
                }
            }
            g.hold_bond_id[(0, e)] = cast::<i32, _>((1..=15).filter(|&s| g.hold_bond_id[(s, e)] != 0).count());
        }
    }
    // bonds of the subsidiary / the parent
    g.bonds_issued[sub] = 0;
    g.bonds_issued[parent] = 0;
    for h in (1..=130).chain(1581..=1583) {
        for s in 1..=15 {
            if g.hold_bond_id[(s, h)] == sub || g.hold_bond_id[(s, h)] == parent {
                g.hold_bond_id[(s, h)] = 0;
                g.hold_bond_face[(s, h)] = 0;
            }
        }
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
        { let b = r.chance(cast::<f64, _>(Ext::from_f64(0.15))); set_bonds(&mut g, &mut r, sub, b); }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
            { let b = r.chance(cast::<f64, _>(Ext::from_f64(0.5))); place_bonds(&mut g, &mut r, sub, b); }
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            set_bonds(&mut g, &mut r, parent, false);
            if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
                { let b = r.chance(cast::<f64, _>(Ext::from_f64(0.5))); place_bonds(&mut g, &mut r, parent, b); }
            }
        }
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        // short of funds: buy-backs and calls fail
        for e in [parent, sub] {
            g.cash[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5.0));
            g.t_bills[e] = Ext::from_f64(0.0);
            g.bank_debt[e] = f64r(&mut r, Ext::from_f64(5000.0), Ext::from_f64(20000.0));
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) && g.bonds_issued[sub] > 0 && g.bonds_issued[parent] == 0 {
            set_bonds(&mut g, &mut r, parent, false);
        }
        match r.range(0, 3) {
            0 => {
                // corporate reorg: both issues fully public, the subsidiary's a bit cheaper
                set_bonds(&mut g, &mut r, sub, false);
                set_bonds(&mut g, &mut r, parent, false);
                let mat = g.cur_year + cast::<i32, _>(r.range(3, 15));
                g.bond_issue_info[parent] = st(&format!("{mat}    8.00"));
                g.bond_issue_info[sub] = st(&format!("{mat}    {:.2}", f64r(&mut r, Ext::from(Ext::from_f64(6.5)), Ext::from(Ext::from_f64(7.9)))));
                g.bank_debt[sub] = Ext::from_f64(1e6);
                g.bank_debt[parent] = Ext::from_f64(1e6);
                g.credit_rating_num[sub] = g.credit_rating_num[parent];
                for k in 1..=15 {
                    if cast::<i32, _>(g.hold_stock_id[(k, sub)]) != parent {
                        g.hold_stock_id[(k, sub)] = 0;
                    }
                }
                for h in (1..=130).chain(1581..=1583) {
                    for k in 1..=15 {
                        if g.hold_bond_id[(k, h)] == sub || g.hold_bond_id[(k, h)] == parent {
                            g.hold_bond_id[(k, h)] = 0;
                            g.hold_bond_face[(k, h)] = 0;
                        }
                    }
                }
            }
            1 => {
                // the current player holds bonds of the subsidiary below 102: suit by bondholders
                set_bonds(&mut g, &mut r, sub, false);
                set_bonds(&mut g, &mut r, parent, false);
                g.bond_price[sub] = Cur::from_f64(Ext::from_f64(90.0));
                let p = g.cur_player;
                let k = (1..=15).find(|&k| g.hold_bond_id[(k, p)] == 0).unwrap_or(15);
                g.hold_bond_id[(k, p)] = sub;
                g.hold_bond_face[(k, p)] = g.bonds_issued[sub] / 3;
                g.hold_bond_id[(0, p)] = cast::<i32, _>((1..=15).filter(|&k| g.hold_bond_id[(k, p)] != 0).count());
            }
            _ => {
                // a stronger subsidiary: its bondholders block
                // (both issues mature in more than 9 years: CallBonds refuses them)
                g.bank_debt[sub] = Ext::from_f64(0.0);
                g.cash[sub] = f64r(&mut r, Ext::from_f64(500.0), Ext::from_f64(3000.0));
                g.business_assets[sub] = f64r(&mut r, Ext::from_f64(2000.0), Ext::from_f64(5000.0));
                g.credit_rating_num[sub] = 9;
                g.credit_rating_num[parent] = 1;
                for (e, frac) in [(sub, 3), (parent, 2)] {
                    set_bonds(&mut g, &mut r, e, false);
                    g.bond_issue_info[e] = st(&format!("{}    7.50", g.cur_year + 15));
                    let h = cast::<i32, _>(r.range(11, 85));
                    let k = (1..=15).find(|&k| g.hold_bond_id[(k, h)] == 0).unwrap_or(15);
                    g.hold_bond_id[(k, h)] = e;
                    g.hold_bond_face[(k, h)] = g.bonds_issued[e] / frac;
                    g.hold_bond_id[(0, h)] = cast::<i32, _>((1..=15).filter(|&k| g.hold_bond_id[(k, h)] != 0).count());
                }
            }
        }
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
        // a zero-percent position in the parent's stock (cashed out when the parent retires own stock)
        let h = pick(&mut r, &g, 131, 1580, sub);
        let k = (1..=15).find(|&k| g.hold_stock_id[(k, h)] == 0).unwrap_or(15);
        g.hold_stock_id[(k, h)] = cast::<i16, _>(parent);
        g.hold_stock_pct[(k, h)] = 0;
        g.hold_stock_basis[(k, h)] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(30.0));
        let s5 = (1..=15).find(|&k| g.hold_stock_id[(k, sub)] == 0 || cast::<i32, _>(g.hold_stock_id[(k, sub)]) == parent).unwrap_or(15);
        g.hold_stock_id[(s5, sub)] = cast::<i16, _>(parent);
        g.hold_stock_pct[(s5, sub)] = 5;
    }
    if kind == 6 && r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
        let s = 1;
        g.hold_bond_id[(s, sub)] = pick(&mut r, &g, 131, 1580, 0);
        g.hold_bond_face[(s, sub)] = r.range(1, 50);
        g.hold_bond_id[(0, sub)] = cast::<i32, _>((1..=15).filter(|&k| g.hold_bond_id[(k, sub)] != 0).count());
    }
    // futures, stored commodities, options, ETF advisory contracts, swaps
    if r.chance(cast::<f64, _>(Ext::from_f64(0.35))) {
        let n = r.range(1, 4);
        add_futures(&mut g, &mut r, sub, n);
        g.commod_margin_bal[sub] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(30.0));
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        let n = r.range(1, 3);
        add_futures(&mut g, &mut r, parent, n);
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        let n = r.range(1, 3);
        add_phys(&mut g, &mut r, sub, n);
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.25))) {
        add_option(&mut g, &mut r, sub, parent);
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
        let c = pick(&mut r, &g, 131, 1580, parent);
        add_option(&mut g, &mut r, sub, c);
    }
    for etf in 1581..=1600 {
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.fund_adviser[etf] = sub;
        }
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
        g.swap_contracts[1] = st(&format!("{sub}|{}|5|P|{}.1|{}.4|200", r.range(11, 85), g.cur_year - 1, g.cur_year + 3));
    }
    // globals that follow the subsidiary
    match r.range(0, 9) {
        0 => g.scenario_corp = cast::<i16, _>(sub),
        1 => {
            g.scenario_corp = cast::<i16, _>(sub);
            g.scenario_num = 7;
        }
        2 => {
            g.scenario_corp = cast::<i16, _>(sub);
            g.cheat_eq_fund_target = cast::<i16, _>(sub);
        }
        3 => g.asbestos_corp = -sub,
        4 => g.strike_corp_code = cast::<i16, _>(sub + 10000),
        5 => g.merger_target = sub,
        6 => g.malicious_suit_defendant = sub,
        _ => {}
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.15))) {
        g.sel_entity = sub;
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.15))) {
        g.last_sel_entity = sub;
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        g.player_advances[(1, sub)] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(50.0));
    }
    let answers = (0..6).map(|_| !r.chance(cast::<f64, _>(Ext::from_f64(0.15)))).collect();
    Case { g, parent, sub, mode, answers }
}

static NEWS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static OUTCOMES: Mutex<Vec<(u64, i32, bool)>> = Mutex::new(Vec::new());
/// Liquidations whose subsidiary's bonds were exchanged 1:1 for the parent's ("corporate reorg"; its
/// headline is never posted, BUGS.md #226).
static REORGS: Mutex<usize> = Mutex::new(0);

#[test]
fn diff_liquidate_sub_into_parent() {
    let __content = wsr_core::content::current();
    let seeds = 1..=231;
    par_seeds(seeds, |seed| {
        let c = case(seed);
        let mut port = c.g.clone();
        let mut rng = Rng::from_state(rng0(seed));
        let mut ui = Rec::default();
        ui.answers = c.answers.iter().map(|&y| if y { 6 } else { 7 }).collect();
        liquidate_sub_into_parent(&mut port, &mut ui, &mut rng, c.parent, c.sub, 0);
        let answers: Vec<&str> = c.answers.iter().map(|&y| if y { "IDYES" } else { "IDNO" }).collect();
        let Some(o) = run(LIQ, &[&c.parent.to_string(), &c.sub.to_string()], &c.g, rng0(seed), &answers, SKIPS) else {
            return;
        };
        check(&port, &rng, &ui, &o, &format!("seed {seed}{__wsr_text_14037}{}{__wsr_text_14038}{}{__wsr_text_13548}{}", c.parent, c.sub, c.mode,__wsr_text_14037=__content.string(14037), __wsr_text_14038=__content.string(14038), __wsr_text_13548=__content.string(13548)));
        NEWS.lock().unwrap().extend(ui.news.iter().cloned());
        // (a bank/insurer slot is refilled at once, so the status is not the criterion)
        let done = ui.news.iter().any(|n| n.contains(__content.string(9544)));
        OUTCOMES.lock().unwrap().push((seed % 7, c.mode, done));
        let (p0, s0) = (c.g.bonds_issued[c.parent], c.g.bonds_issued[c.sub]);
        if p0 > 0 && s0 > 0 && port.bonds_issued[c.parent] == p0 + s0 && !ui.news.iter().any(|n| n.contains("swapped")) {
            *REORGS.lock().unwrap() += 1;
        }
    });
    if unavailable().is_some() {
        return;
    }
    let news = NEWS.lock().unwrap();
    let mut missing = Vec::new();
    for frag in [
        __content.string(14039),
        __content.string(9512),
        __content.string(14040),
        __content.string(14041),
        __content.string(14042),
        __content.string(14043),
        __content.string(14044),
        __content.string(14045),
        __content.string(14046),
        __content.string(14047),
        __content.string(14048),
        __content.string(14049),
        __content.string(14050),
        __content.string(14051),
        __content.string(14052),
        __content.string(14053),
        __content.string(14054),
        __content.string(14055),
        __content.string(14056),
        "A new bank",
        "A new insurer",
        __content.string(14057),
    ] {
        if !news.iter().any(|n| n.contains(frag)) {
            missing.push(frag);
        }
    }
    assert!(missing.is_empty(), "branches not hit (news): {missing:?}");
    assert!(*REORGS.lock().unwrap() > 0, "branch not hit: corporate reorg");
    let outcomes = OUTCOMES.lock().unwrap();
    for kind in 0..7 {
        for mode in 0..3 {
            assert!(
                outcomes.iter().any(|&(k, m, done)| k == kind && m == mode && done),
                "no completed liquidation of kind {kind} in mode {mode}"
            );
        }
    }
    assert!(outcomes.iter().any(|&(_, _, done)| !done), "no aborted liquidation");
}

#[test]
fn diff_transfer_routines() {
    par_seeds(1..=12, |seed| {
        let mut g = world(seed + 7400);
        let mut r = Rand(seed * 31 + 5);
        let from = cast::<i32, _>(r.range(11, 1580));
        let to = if seed % 3 == 0 { cast::<i32, _>(r.range(11, 130)) } else { cast::<i32, _>(r.range(131, 1580)) };
        g.credit_rating_num[to] = if seed % 4 == 1 { 3 } else { 8 };
        let n = r.range(1, 5);
        add_futures(&mut g, &mut r, from, n);
        add_phys(&mut g, &mut r, from, 3);
        g.commod_margin_bal[from] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(30.0));
        for etf in 1581..=1600 {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                g.fund_adviser[etf] = from;
            }
        }
        let (a, b) = (from.to_string(), to.to_string());
        // TransferFuturesOnLiquidation(from, to)
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0(seed));
        let mut ui = Rec::default();
        let moved = transfer_futures_on_liquidation(&mut port, &mut ui, &mut rng, from, to);
        if let Some(o) = run(TRANSFER_FUTURES, &[&format!("v:{a}"), &format!("v:{b}")], &g, rng0(seed), &[], &[]) {
            check(&port, &rng, &ui, &o, &format!("futures seed {seed}"));
            assert_eq!(cast::<u32, _>(moved), o.return_value, "futures seed {seed}: moved");
        }
        // TransferPhysCommodOnLiquidation(new, old)
        let mut port = g.clone();
        let mut ui = Rec::default();
        transfer_phys_commod_on_liquidation(&mut port, &mut ui, to, from);
        if let Some(o) = run(TRANSFER_PHYS, &[&b, &a], &g, 1, &[], &[]) {
            check(&port, &Rng::from_state(1), &ui, &o, &format!("phys seed {seed}"));
        }
        // TransferEtfContracts(to, from)
        let mut port = g.clone();
        let mut ui = Rec::default();
        transfer_etf_contracts(&mut port, &mut ui, to, from);
        if let Some(o) = run(TRANSFER_ETF, &[&b, &a], &g, 1, &[], &[]) {
            check(&port, &Rng::from_state(1), &ui, &o, &format!("etf seed {seed}"));
        }
    });
}


