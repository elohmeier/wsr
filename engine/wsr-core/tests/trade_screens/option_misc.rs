//! Differential tests of `logic::trade_screens::option_misc`: CloseAllOptionPositions (0x611ab6),
//! ShowOptionSizeLimits (0x612d58), TradeGovtBonds (0x5ba0ee) and TakePhysicalDelivery (0x664f9f).
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use super::script::*;
use wsr_core::logic::trade_screens::option_misc::*;
use wsr_core::pb::numeric::str_num;
use wsr_core::state::Globals;

const CLOSE_ALL: u32 = 0x611ab6;
const SIZE_LIMITS: u32 = 0x612d58;
const TRADE_GOVT_BONDS: u32 = 0x5ba0ee;
const TAKE_DELIVERY: u32 = 0x664f9f;

fn str15(x: Ext) -> String {
    str_num(x, 15)
}

/// An option record `corp|type|holder|qty|strike|month|year|premium` (STR$ fields).
#[allow(clippy::too_many_arguments)]
fn rec(corp: i32, opt_type: i32, holder: i32, qty: i32, strike: Ext, month: i32, year: i32, prem: &str) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}",
        str15(Ext::from(cast::<Ext, _>(corp))).trim(),
        str15(Ext::from(cast::<Ext, _>(opt_type))),
        str15(Ext::from(cast::<Ext, _>(holder))),
        str15(Ext::from(cast::<Ext, _>(qty))),
        str15(strike),
        str15(Ext::from(cast::<Ext, _>(month))),
        str15(Ext::from(cast::<Ext, _>(year))),
        prem
    )
}

fn add_rec(g: &mut Globals, text: &str) {
    if let Some(k) = (1..=1000).find(|&k| g.option_recs[k].is_empty()) {
        g.option_recs[k] = st(text);
    }
    let n = (1..=1000).filter(|&k| !g.option_recs[k].is_empty()).count();
    g.option_recs[0] = st(&n.to_string());
}

/// A stock with a unique symbol and a share price of 6..400.
fn stock(g: &mut Globals, r: &mut Rand, not: &[i32]) -> i32 {
    let c = pick_active(r, g, 131, 1580, not);
    g.ent_symbol[c] = st(&format!("ZQ{c}"));
    g.shares_outstanding[c] = f64r(r, Ext::from_f64(2.0), Ext::from_f64(40.0));
    g.market_cap[c] = f64r(r, Ext::from_f64(6.0), Ext::from_f64(400.0)) * g.shares_outstanding[c];
    g.credit_rating_num[c] = cast::<i32, _>(r.range(2, 10));
    c
}

/// The acting entity: the current player or a company he controls.
fn acting(g: &mut Globals, r: &mut Rand, corp: bool, lo: i64, hi: i64) -> i32 {
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    g.is_computer_player[g.cur_player] = 0;
    let ae = if corp { pick_active(r, g, lo, hi, &[]) } else { g.cur_player };
    if ae > 10 {
        g.corp_controller[ae] = g.cur_player;
    }
    g.active_entity = ae;
    g.active_entity_name = g.ent_name[ae].clone();
    g.sel_entity = ae;
    ae
}

fn yes_no(r: &mut Rand, p_yes: Ext) -> String {
    if r.chance(cast::<f64, _>(p_yes)) { "IDYES" } else { "IDNO" }.to_string()
}

#[test]
fn diff_close_all_option_positions() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(seeds(1..=80), |seed| {
        let mut g = world(seed + 36_600);
        let mut r = Rand(seed * 8_111 + 7);
        let co = r.chance(cast::<f64, _>(Ext::from_f64(0.4)));
        let ae = acting(&mut g, &mut r, co, 131, 1580);
        let corp = stock(&mut g, &mut r, &[ae]);
        let other = stock(&mut g, &mut r, &[ae, corp]);
        g.option_recs[0] = st("0");
        let kind = seed % 5;
        let n = match kind {
            0 => 0,
            1 => 1,
            _ => r.range(2, 5),
        };
        for _ in 0..n {
            let opt_type = cast::<i32, _>(r.range(1, 2));
            let mut qty = cast::<i32, _>(r.range(1, 6));
            if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
                qty = -qty;
            }
            let strike = (g.market_cap[corp] * f64r(&mut r, Ext::from_f64(0.6), Ext::from_f64(1.4)) * Ext::from_f64(100.0)).round() / Ext::from_f64(100.0);
            let month = cast::<i32, _>(r.range(1, 12));
            let year = g.cur_year + cast::<i32, _>(r.range(0, 1));
            let prem = if ae <= 10 && qty > 0 && opt_type == 1 && r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
                "GRANT".to_string()
            } else {
                str15((g.market_cap[corp] * f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(0.2)) * cast::<Ext, _>(qty.abs()) * Ext::from_f64(100.0)).round() / Ext::from_f64(100.0))
            };
            add_rec(&mut g, &rec(corp, opt_type, ae, qty, strike, month, year, &prem));
        }
        // records of others / on another stock
        for _ in 0..r.range(0, 3) {
            let h = pick_active(&mut r, &g, 131, 1580, &[ae]);
            let s = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { corp } else { other };
            let t = rec(s, 1, h, 2, g.market_cap[s], 6, g.cur_year + 1, "5");
            add_rec(&mut g, &t);
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            let t = rec(other, 2, ae, 3, g.market_cap[other], 6, g.cur_year + 1, "7");
            add_rec(&mut g, &t);
        }
        let answers = vec![yes_no(&mut r, Ext::from(Ext::from_f64(0.8)))];
        let args = [corp.to_string()];
        let cmp = cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-7));
        let res = std::cell::Cell::new(-1);
        let Some((ui, pg, o)) = check(seed, CLOSE_ALL, &args, &g, &answers, &[], cmp, |g, ui, rng| {
            res.set(close_all_option_positions(g, ui, rng, corp));
        }) else {
            return;
        };
        assert_eq!(cast::<u32, _>(res.get()), o.return_value, "result seed {seed}");
        hits.record(&ui);
        if pg.cash[ae] != g.cash[ae] {
            hits.add("<closed>");
        }
    });
    hits.assert(&[
        __content.string(14225),
        __content.string(14226),
        "<closed>",
        __content.string(11584),
        __content.string(11583),
        "(short)",
        "Closed",
    ]);
}

#[test]
fn diff_show_option_size_limits() {
    par_seeds(seeds(1..=12), |seed| {
        let mut g = world(seed + 36_700);
        let mut r = Rand(seed * 5_003 + 1);
        let co = r.chance(cast::<f64, _>(Ext::from_f64(0.5)));
        let ae = acting(&mut g, &mut r, co, 11, 1580);
        let corp = stock(&mut g, &mut r, &[ae]);
        g.option_recs[0] = st("0");
        for _ in 0..r.range(0, 4) {
            let h = pick_active(&mut r, &g, 131, 1580, &[]);
            let q = cast::<i32, _>(r.range(-8, 8));
            let t = rec(corp, cast::<i32, _>(r.range(1, 2)), h, q, g.market_cap[corp], 6, g.cur_year + 1, "5");
            add_rec(&mut g, &t);
        }
        let args = [corp.to_string()];
        check(seed, SIZE_LIMITS, &args, &g, &[], &[], cmp(), |g, ui, _| show_option_size_limits(g, ui, corp));
    });
}

#[test]
fn diff_trade_govt_bonds() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(seeds(1..=163), |seed| {
        let mut g = world(seed + 36_800);
        let mut r = Rand(seed * 92_821 + 5);
        let mut action = if seed % 23 == 0 { [0, 5][cast::<usize, _>(r.range(0, 1))] } else { cast::<i32, _>(r.range(1, 4)) };
        let ae = match r.range(0, 3) {
            0 => acting(&mut g, &mut r, false, 0, 0),
            1 => acting(&mut g, &mut r, true, 11, 85),
            2 => acting(&mut g, &mut r, true, 86, 130),
            _ => acting(&mut g, &mut r, true, 131, 1580),
        };
        if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
            g.long_gov_bond_face[ae] = r.range(1, 3000);
            g.lt_gov_bond_cost[ae] = cast::<Ext, _>(g.long_gov_bond_face[ae]) * f64r(&mut r, Ext::from_f64(0.7), Ext::from_f64(1.2));
        } else {
            g.long_gov_bond_face[ae] = 0;
            g.lt_gov_bond_cost[ae] = Ext::from_f64(0.0);
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
            g.short_gov_bond_face[ae] = r.range(1, 3000);
            g.st_gov_bond_cost[ae] = cast::<Ext, _>(g.short_gov_bond_face[ae]) * f64r(&mut r, Ext::from_f64(0.7), Ext::from_f64(1.2));
        } else {
            g.short_gov_bond_face[ae] = 0;
            g.st_gov_bond_cost[ae] = Ext::from_f64(0.0);
        }
        match r.range(0, 4) {
            0 => {
                // broke
                g.cash[ae] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(0.5));
                g.t_bills[ae] = Ext::from_f64(0.0);
            }
            1 => {
                // short of cash, needs a loan
                g.cash[ae] = f64r(&mut r, Ext::from_f64(5.0), Ext::from_f64(200.0));
                g.t_bills[ae] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(50.0));
            }
            _ => g.cash[ae] = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(200_000.0)),
        }
        g.short_bond_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(85.0), Ext::from_f64(110.0)));
        g.interest_rate_target = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(28.0)));
        if seed == 161 {
            // Random balance sheets generally still have credit despite low cash. Force an
            // insolvent buyer to exercise the minimum-one-unit refusal without an input dialog.
            action = 1;
            g.cash[ae] = -Ext::from_f64(1.0e12);
            g.t_bills[ae] = Ext::from_f64(0.0);
        }
        if seed == 162 {
            action = 1;
            g.cash[ae] = Ext::from_f64(1.0e9);
            g.long_gov_bond_face[ae] = 0;
            // The supply cap is tested below with an order larger than all issued bonds.
        }
        let held = if action == 2 { g.long_gov_bond_face[ae] } else { g.short_gov_bond_face[ae] };
        let mut answers: Vec<String> = Vec::new();
        for _ in 0..3 {
            let amt = match r.range(0, 7) {
                0 => "".to_string(),
                1 => "0".to_string(),
                2 => format!("{}", held + r.range(1, 100)),
                3 => format!("${},", r.range(1, 50)),
                4 => format!("{}", r.range(1, held.max(1))),
                5 => format!("{}", r.range(1000, 2_000_000)),
                _ => format!("{}", r.range(1, 500)),
            };
            answers.push(amt);
            // a large-trade question or the financing question
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                answers.push(yes_no(&mut r, Ext::from_f64(0.7)));
            }
        }
        if seed == 162 { answers = vec!["999999999".into(), "IDYES".into()]; }
        if seed == 163 {
            action = 2;
            g.long_gov_bond_face[ae] = 100_000_000;
            g.lt_gov_bond_cost[ae] = Ext::from_f64(100_000_000.0);
            answers = vec!["100000000".into(), "IDYES".into()];
        }
        let args = [action.to_string()];
        let cmp = cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-7));
        let Some((ui, pg, _)) = check(seed, TRADE_GOVT_BONDS, &args, &g, &answers, &[], cmp, |g, ui, rng| {
            trade_govt_bonds(g, ui, rng, action)
        }) else {
            return;
        };
        hits.record(&ui);
        if pg.interest_rate_target != g.interest_rate_target {
            hits.add("<irt>");
        }
    });
    hits.assert(&[
        __content.string(14227),
        __content.string(1928),
        __content.string(14228),
        "CONFIRMATION",
        __content.string(14229),
        "sold",
        __content.string(14230),
        __content.string(11619),
        "profit",
        __content.string(14231),
        __content.string(11887),
        __content.string(14232),
        "<irt>",
    ]);
}

#[test]
fn diff_take_physical_delivery() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(seeds(1..=80), |seed| {
        let mut g = world(seed + 36_900);
        let mut r = Rand(seed * 1_337 + 9);
        g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
        let commods = ["GOLD", "SILVER", "OIL", "WHEAT", "CORN", "BITCOIN", "ETHEREUM"];
        let commod = commods[cast::<usize, _>(r.range(0, 6))];
        let holder = match r.range(0, 4) {
            0 => g.cur_player,
            1 => {
                let c = pick_active(&mut r, &g, 131, 1580, &[]);
                g.corp_controller[c] = g.cur_player;
                c
            }
            2 => {
                let c = pick_active(&mut r, &g, 131, 1580, &[]);
                g.sel_entity = c;
                c
            }
            3 => pick_active(&mut r, &g, 11, 1580, &[]),
            _ => 0,
        };
        let holder = if seed % 17 == 0 { 0 } else { holder };
        if seed % 13 == 0 && holder > 10 {
            g.entity_status[holder] = 0;
        }
        if holder > 0 && seed % 11 == 0 {
            g.ent_symbol[holder] = st("AVERYLONGSYMBOLNAME-AVERYLONGSYMBOLNAME");
        }
        let contracts = if seed % 19 == 0 { 0 } else { cast::<i32, _>(r.range(1, 40)) };
        let cost = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(900.0));
        let fut = format!(
            "{}|{}|{}|{}|{}|{}",
            commod,
            str15(Ext::from(cast::<Ext, _>(holder))).trim(),
            str15(Ext::from(cast::<Ext, _>(contracts))),
            str15(Ext::from(cast::<Ext, _>(r.range(1, 12)))),
            str15(Ext::from(cast::<Ext, _>(g.cur_year))),
            str15(cost)
        );
        let mut n = 0;
        for k in 1..=1000 {
            g.phys_commod_rec[k] = Default::default();
        }
        if holder > 0 && r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            n += 1;
            let units = r.range(1, 30);
            let c0 = str15(f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(500.0)));
            g.phys_commod_rec[n] = st(&format!("{commod}|{holder}|{units}|{}", c0.trim()));
        }
        for _ in 0..r.range(0, 3) {
            n += 1;
            let h = pick_active(&mut r, &g, 11, 1580, &[]);
            let c = commods[cast::<usize, _>(r.range(0, 6))];
            let units = r.range(1, 30);
            g.phys_commod_rec[n] = st(&format!("{c}|{h}|{units}|100"));
        }
        g.phys_commod_rec[0] = st(&n.to_string());
        if holder > 0 {
            match r.range(0, 2) {
                0 => {
                    g.cash[holder] = f64r(&mut r, -Ext::from_f64(100.0), cost);
                    g.t_bills[holder] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(10.0));
                }
                1 => {
                    g.cash[holder] = -f64r(&mut r, Ext::from_f64(1e6), Ext::from_f64(5e6));
                    g.t_bills[holder] = Ext::from_f64(0.0);
                }
                _ => g.cash[holder] = f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(9000.0)),
            }
        }
        let args = [format!("s:{fut}")];
        let cmp = cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-7));
        let fr = st(&fut);
        let res = std::cell::Cell::new(-1);
        let Some((ui, _, o)) = check(seed, TAKE_DELIVERY, &args, &g, &[], &[], cmp, |g, ui, rng| {
            res.set(take_physical_delivery(g, ui, rng, &fr));
        }) else {
            return;
        };
        assert_eq!(cast::<u32, _>(res.get()), o.return_value, "result seed {seed}");
        hits.add(&format!("<result{}>", res.get()));
        hits.record(&ui);
    });
    hits.assert(&[__content.string(14233), __content.string(14234), "bullion", "takes delivery of Bitcoin", "<result2>", "<result0>"]);
}
