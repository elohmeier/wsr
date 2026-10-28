//! Tests of `logic::earnings`: hand-checked unit tests plus differential tests against the original
//! routines run under emulation (tools/emu/game.py, see tests/diff/mod.rs).

#![allow(clippy::field_reassign_with_default)]
use wsr_core::pb::real::{Ext, cast};


mod diff;

use diff::{Compare, Opts, Rand, run_original_with, par_seeds};
use wsr_core::logic::earnings::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::pb::numeric::Cur;
use wsr_core::state::Globals;
use wsr_core::ui::Scripted;

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

// ------------------------------------------------------------------------------------------- states

/// (Copied from tests/finance.rs.) A plausible mid-game world: players 1..NumPlayers, banks 11..85, insurers 86..130, operating corps
/// 131..1580, funds 1581..1583, with random balance sheets, holdings, bonds and economy.
fn world(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut r = Rand(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut g = Globals::default();
    let f32r = |r: &mut Rand, lo: Ext, hi: Ext| cast::<f32, _>(lo + (hi - lo) * cast::<Ext, _>(r.range(0, 1_000_000)) / Ext::from_f64(1e6));
    let f64r = |r: &mut Rand, lo: Ext, hi: Ext| lo + (hi - lo) * cast::<Ext, _>(r.range(0, 1_000_000_000)) / Ext::from_f64(1e9);
    g.world_generated = 1;
    g.game_end_flag = s("N");
    g.num_players = cast::<i32, _>(r.range(1, 4));
    g.difficulty_level = cast::<i32, _>(r.range(1, 4));
    g.cur_year = cast::<i32, _>(r.range(2000, 2030));
    g.cur_quarter = cast::<i32, _>(r.range(1, 4));
    g.sim_co_count = [300, 600, 800, 1100, 1600][cast::<usize, _>(r.range(0, 4))];
    g.base_interest_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(16.0)));
    g.prime_rate = cast::<f32, _>(g.base_interest_rate + f32r(&mut r, Ext::from_f64(0.5), Ext::from_f64(3.0)));
    g.sofr_rate = cast::<f32, _>(g.base_interest_rate + f32r(&mut r, -Ext::from_f64(0.5), Ext::from_f64(0.5)));
    g.gdp_growth = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(10.0), Ext::from_f64(8.0)));
    g.monetary_policy = cast::<i16, _>(r.range(0, 2));
    g.exchange_rate = cast::<f64, _>(if r.chance(Ext::from_f64(0.5)) { Ext::from_f64(1.0) } else { f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(2.0)) });
    g.loan_limit_multiple = cast::<f64, _>(if r.chance(Ext::from_f64(0.3)) { Ext::from_f64(0.0) } else { (cast::<Ext, _>(r.range(20, 35))) * Ext::from_f64(0.1) });
    g.corp_tax_rate = cast::<i32, _>(r.range(15, 45));
    g.bank_cd_rate = cast::<f32, _>(if r.chance(Ext::from_f64(0.2)) { Ext::from_f64(0.0) } else { Ext::from(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(9.0))) });
    g.subprime_crisis_state = cast::<i16, _>(r.range(-2, 2));
    g.long_bond_desc = s(&format!("{} LONG T-BOND {:5.2}", g.cur_year + cast::<i32, _>(r.range(8, 30)), f32r(&mut r, Ext::from(Ext::from_f64(2.0)), Ext::from(Ext::from_f64(12.0)))));
    g.short_bond_desc = s(&format!("{} SHORT T-NOTE {:5.2}", g.cur_year + cast::<i32, _>(r.range(0, 9)), f32r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(10.0)))));
    g.long_bond_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(70.0), Ext::from_f64(130.0)));
    g.short_bond_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(85.0), Ext::from_f64(110.0)));
    g.oil_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.3), Ext::from_f64(3.0)));
    g.gold_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(300.0), Ext::from_f64(2500.0)));
    g.silver_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(4.0), Ext::from_f64(40.0)));
    g.wheat_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(12.0)));
    g.corn_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(9.0)));
    g.bitcoin_price = cast::<i32, _>(r.range(100, 90000));
    g.ethereum_price = cast::<i32, _>(r.range(10, 5000));
    if r.chance(Ext::from_f64(0.6)) {
        g.wealth_tax_rate = cast::<i32, _>(r.range(1, 3));
        g.carbon_tax_rate = cast::<i32, _>(r.range(1, 5));
        g.health_care_tax_rate = cast::<i32, _>(r.range(1, 5));
        g.oil_windfall_tax_rate = cast::<i32, _>(r.range(1, 20));
        g.holding_co_tax_rate = cast::<i32, _>(r.range(1, 10));
        g.corp_shares_tax_country = cast::<i32, _>(r.range(0, 3));
    }
    for p in 1..=5 {
        g.is_computer_player[p] = if r.chance(Ext::from_f64(0.3)) { 1 } else { 0 };
        g.player_ceo_of[p] = if r.chance(Ext::from_f64(0.5)) { cast::<i32, _>(r.range(131, 1580)) } else { 0 };
        g.player_prepaid_tax[p] = if r.chance(Ext::from_f64(0.3)) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(50.0)) } else { Ext::from_f64(0.0) };
    }
    for e in 1..=1600 {
        let active = if e <= 10 { e <= g.num_players } else { r.chance(Ext::from_f64(0.93)) };
        g.entity_status[e] = if active { 1 } else if r.chance(Ext::from_f64(0.5)) { 0 } else { 2 };
        g.ent_name[e] = s(&format!("{__wsr_text_13758}{e}",__wsr_text_13758=__content.string(13758)));
        g.cash[e] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(3000.0));
        g.t_bills[e] = if r.chance(Ext::from_f64(0.5)) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(1000.0)) } else { Ext::from_f64(0.0) };
        g.bank_debt[e] = if r.chance(Ext::from_f64(0.6)) { f64r(&mut r, -Ext::from_f64(1.0), Ext::from_f64(2000.0)) } else { Ext::from_f64(0.0) };
        g.lending_bank[e] = if r.chance(Ext::from_f64(0.9)) { cast::<i16, _>(r.range(11, 50)) } else { 0 };
        g.credit_rating_num[e] = cast::<i32, _>(r.range(0, 10));
        g.credit_frozen[e] = if r.chance(Ext::from_f64(0.1)) { 1 } else { 0 };
        g.long_gov_bond_face[e] = if r.chance(Ext::from_f64(0.3)) { r.range(1, 500) } else { 0 };
        g.short_gov_bond_face[e] = if r.chance(Ext::from_f64(0.3)) { r.range(1, 500) } else { 0 };
        g.lt_gov_bond_cost[e] = cast::<Ext, _>(g.long_gov_bond_face[e]) * f64r(&mut r, Ext::from_f64(0.8), Ext::from_f64(1.2));
        g.st_gov_bond_cost[e] = cast::<Ext, _>(g.short_gov_bond_face[e]) * f64r(&mut r, Ext::from_f64(0.9), Ext::from_f64(1.1));
        g.corp_controller[e] = match r.range(0, 5) {
            0 => 0,
            1 | 2 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
            _ => cast::<i32, _>(r.range(11, 1600)),
        };
        g.commod_margin_bal[e] = if r.chance(Ext::from_f64(0.1)) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(30.0)) } else { Ext::from_f64(0.0) };
        g.auto_pilot[e] = if r.chance(Ext::from_f64(0.3)) { 1 } else { 0 };
        for slot in 1..=15 {
            if r.chance(Ext::from_f64(0.25)) {
                g.hold_stock_id[(slot, e)] = cast::<i16, _>(r.range(11, 1600));
                g.hold_stock_pct[(slot, e)] = cast::<i16, _>(r.range(1, 100));
            }
            if !(131..=1580).contains(&e) && r.chance(Ext::from_f64(0.2)) {
                g.hold_bond_id[(slot, e)] = cast::<i32, _>(r.range(11, 1600));
                g.hold_bond_face[(slot, e)] = r.range(1, 300);
                g.bond_hold_cost[(slot, e)] = cast::<Ext, _>(g.hold_bond_face[(slot, e)]) * f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(1.2));
            }
        }
        if e <= 5 {
            for c in 11..=1600 {
                if r.chance(Ext::from_f64(0.01)) {
                    g.player_advances[(e, c)] = f64r(&mut r, -Ext::from_f64(0.01), Ext::from_f64(20.0));
                }
            }
        }
        if e > 10 {
            g.industry_num[e] = match e {
                11..=85 => 1,
                86..=130 => 2,
                1581..=1583 => 71,
                _ => cast::<i16, _>(r.range(3, 70)),
            };
            g.market_cap[e] = f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(300.0));
            g.shares_outstanding[e] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(500.0));
            g.tax_loss_carry[e] = if r.chance(Ext::from_f64(0.2)) { f64r(&mut r, -Ext::from_f64(100.0), Ext::from_f64(0.0)) } else { Ext::from_f64(0.0) };
            g.extraordinary_pl[e] = if r.chance(Ext::from_f64(0.1)) { r.range(-50, 50) } else { 0 };
            g.accrued_pl[e] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(200.0));
            g.div_recvd1[e] = if r.chance(Ext::from_f64(0.2)) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(10.0)) } else { Ext::from_f64(0.0) };
            g.div_recvd2[e] = if r.chance(Ext::from_f64(0.2)) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(10.0)) } else { Ext::from_f64(0.0) };
            g.consol_tax_inc_ytd[e] = f64r(&mut r, -Ext::from_f64(30.0), Ext::from_f64(100.0));
            g.tax_credits[e] = if r.chance(Ext::from_f64(0.2)) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(20.0)) } else { Ext::from_f64(0.0) };
            g.contingency_reserve[e] = if r.chance(Ext::from_f64(0.3)) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(50.0)) } else { Ext::from_f64(0.0) };
            g.earnings_y1[e] = f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(100.0));
            g.earnings_yr2_ago[e] = f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(100.0));
            g.earnings_yr3_ago[e] = f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(100.0));
            g.earnings_yr4_ago[e] = f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(100.0));
            g.earnings_ytd[e] = f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(60.0));
            g.proj_qtr_earnings[e] = f64r(&mut r, -Ext::from_f64(10.0), Ext::from_f64(30.0));
            g.country_idx[e] = cast::<i32, _>(r.range(0, 25));
            if r.chance(Ext::from_f64(0.4)) {
                g.bonds_issued[e] = r.range(1, 1500);
                let mat = g.cur_year + cast::<i32, _>(r.range(1, 25));
                let coupon = f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(15.0));
                g.bond_issue_info[e] = if r.chance(Ext::from_f64(0.3)) {
                    let cap = g.market_cap[e] * f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(2.0));
                    s(&format!("{mat}|{cap:.2}|cv|{coupon:5.2}"))
                } else {
                    s(&format!("{mat}|0||{coupon:5.2}"))
                };
                g.bond_price[e] = Cur::from_f64(f64r(&mut r, Ext::from_f64(20.0), Ext::from_f64(130.0)));
            }
        }
        if (131..=1600).contains(&e) {
            g.business_assets[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5000.0));
            g.working_capital[e] = f64r(&mut r, -Ext::from_f64(100.0), Ext::from_f64(1000.0));
            g.goodwill[e] = if r.chance(Ext::from_f64(0.2)) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(300.0)) } else { Ext::from_f64(0.0) };
        }
        if (11..=85).contains(&e) {
            g.bank_demand_deposits[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(20000.0));
            g.bank_c_ds[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(20000.0));
            g.bank_cust_deposits[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5000.0));
            g.bad_debt_reserve[e] = f64r(&mut r, -Ext::from_f64(5.0), Ext::from_f64(300.0));
            g.bank_consumer_loans[e] = r.range(0, 20000);
            g.bank_mortgage_loans[e] = r.range(0, 20000);
        }
        if (11..=130).contains(&e) {
            g.subprime_secs[e] = if r.chance(Ext::from_f64(0.3)) { r.range(0, 3000) } else { 0 };
        }
        if (86..=130).contains(&e) {
            g.ins_reserves[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(10000.0));
        }
    }
    // a few physical commodity records ("CODE…|owner|units|cost")
    let codes = ["GOLD", "SILVER", "OIL", "WHEAT", "CORN", "BITCOIN", "ETH", "XYZ"];
    let n = cast::<i32, _>(r.range(0, 30));
    for i in 1..=n {
        let owner = if r.chance(Ext::from_f64(0.5)) { r.range(1, cast::<i64, _>(g.num_players)) } else { r.range(131, 1600) };
        let code = codes[cast::<usize, _>(r.range(0, 7))];
        g.phys_commod_rec[i] = s(&format!(" {code}|{owner}|{}|{:.2} ", r.range(1, 5000), f64r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(900.0)))));
    }
    g.phys_commod_rec[0] = s(&n.to_string());
    g
}

/// [`world`] plus the state CalcProjectedEarnings and friends read: misc data strings, dividends, margins,
/// growth, CEO players, fund advisers, rates. No options, futures, swaps or short positions (callees for
/// those are not ported / not exercised here).
fn earn_world(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut g = world(seed);
    let mut r = Rand(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) ^ 0x5bd1_e995);
    let f32r = |r: &mut Rand, lo: Ext, hi: Ext| cast::<f32, _>(lo + (hi - lo) * cast::<Ext, _>(r.range(0, 1_000_000)) / Ext::from_f64(1e6));
    let f64r = |r: &mut Rand, lo: Ext, hi: Ext| lo + (hi - lo) * cast::<Ext, _>(r.range(0, 1_000_000_000)) / Ext::from_f64(1e9);
    g.t_bill_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.5), Ext::from_f64(9.0)));
    g.borrow_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(3.0), Ext::from_f64(14.0)));
    g.entity_borrow_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(12.0)));
    g.rand_seed = cast::<i32, _>(r.range(0, 100_000));
    g.superfund_corp = if r.chance(Ext::from_f64(0.5)) { cast::<i16, _>(r.range(131, 1580)) } else { 0 };
    g.tmp_ext1 = f64r(&mut r, -Ext::from_f64(5.0), Ext::from_f64(15.0));
    g.tmp_player_advances = if r.chance(Ext::from_f64(0.3)) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(50.0)) } else { Ext::from_f64(0.0) };
    let months = ["", "January", __content.string(6796), "March", "April", "May", "June", "July", "August", __content.string(6797),
        "October", __content.string(6798), __content.string(6799), "January"];
    for (m, name) in months.iter().enumerate() {
        g.month_name[cast::<i32, _>(m)] = s(name);
    }
    for p in 1..=5 {
        g.player_name[p] = if r.chance(Ext::from_f64(0.15)) { s("...OUT...") } else { s(&format!("{__wsr_text_13556}{p}",__wsr_text_13556=__content.string(13556))) };
        g.player_ceo_since_year[p] = if r.chance(Ext::from_f64(0.2)) { 0 } else { g.cur_year - cast::<i32, _>(r.range(0, 6)) };
    }
    for e in 11..=1600 {
        g.ent_symbol[e] = s(&format!("S{e}"));
        g.dividend_per_share[e] = if r.chance(Ext::from_f64(0.5)) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(0.05)) } else { Ext::from_f64(0.0) };
        g.profit_margin[e] = cast::<i16, _>(r.range(-10, 30));
        g.disc_spend_pct[e] = cast::<i16, _>(r.range(0, 20));
        g.growth_rate[e] = cast::<i16, _>(r.range(-10, 60));
        g.equity_income_ytd[e] = if r.chance(Ext::from_f64(0.3)) { f64r(&mut r, -Ext::from_f64(5.0), Ext::from_f64(20.0)) } else { Ext::from_f64(0.0) };
        g.reserve_adj_pending[e] = if r.chance(Ext::from_f64(0.2)) { f64r(&mut r, -Ext::from_f64(5.0), Ext::from_f64(5.0)) } else { Ext::from_f64(0.0) };
        if r.chance(Ext::from_f64(0.08)) {
            g.country_idx[e] = 53;
        }
        if r.chance(Ext::from_f64(0.75)) {
            let fields: Vec<String> = (1..=32)
                .map(|k| match k {
                    2 | 3 => format!("{:.3}", f64r(&mut r, Ext::from(-Ext::from_f64(10.0)), Ext::from(Ext::from_f64(40.0)))),
                    29 => r.range(0, 30).to_string(),
                    30 => format!("{:.2}", f64r(&mut r, Ext::from(-Ext::from_f64(5.0)), Ext::from(Ext::from_f64(25.0)))),
                    _ => r.range(0, 99).to_string(),
                })
                .collect();
            g.ent_misc_data[e] = s(&fields.join("|"));
        }
        if (86..=130).contains(&e) {
            g.insur_uw_profit_ytd[e] = f64r(&mut r, -Ext::from_f64(10.0), Ext::from_f64(10.0));
        }
    }
    for f in 1581..=1600 {
        g.fund_adviser[f] = if r.chance(Ext::from_f64(0.7)) { cast::<i32, _>(r.range(86, 400)) } else { 0 };
        g.fund_fee_pct[f] = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.0), Ext::from_f64(1.3)));
        if f > 1583 && r.chance(Ext::from_f64(0.5)) {
            g.entity_status[f] = 1;
            g.industry_num[f] = 71;
            g.market_cap[f] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(500.0));
        }
    }
    // the fee test needs advisers in industry 37 / 2
    for f in 1581..=1600 {
        let a = g.fund_adviser[f];
        if a > 130 && r.chance(Ext::from_f64(0.5)) {
            g.industry_num[a] = 37;
        }
    }
    g
}

/// Runs the original routine with harness arg specs, RND state and skips.
fn original(addr: u32, args: &[String], g: &Globals, rng: u32, skips: &[(u32, u32)]) -> Option<diff::Original> {
    let opts = Opts { args: args.to_vec(), rng: Some(rng), skips: skips.to_vec(), ..Opts::default() };
    run_original_with(addr, &opts, g)
}

fn longs(a: &[i32]) -> Vec<String> {
    a.iter().map(|x| x.to_string()).collect()
}

/// TickerPauseUntil: PauseTicker (UI, behind PostNews popups) is not ported.
fn cmp() -> Compare {
    Compare::default().rel_tol(Ext::from_f64(1e-12)).ignore("TickerPauseUntil")
}

/// Relative float comparison of return values (EXT is 80-bit in the original).
#[track_caller]
fn close(port: Ext, orig: Ext, what: &str) {
    let d = (port - orig).abs();
    assert!(d <= Ext::from_f64(1e-12) * port.abs().max(orig.abs()) || d < Ext::from_f64(1e-300), "{what}: port {port:e} vs original {orig:e}");
}

fn active(g: &Globals, r: &mut Rand, lo: i32, hi: i32) -> i32 {
    loop {
        let e = cast::<i32, _>(r.range(cast::<i64, _>(lo), cast::<i64, _>(hi)));
        if g.entity_status[e] == 1 {
            return e;
        }
    }
}

// ------------------------------------------------------------------------------------- unit tests

#[test]
fn day_of_month_and_month_position() {
    let mut g = Globals::default();
    g.cur_year = 2023;
    // 30-day month, half way: 250 / 16.66667 -> ceil 15
    assert_eq!(calc_day_of_month(&mut g, 4, Ext::from(Ext::from_f64(0.5))), 15);
    assert_eq!(g.tmp_ext1, Ext::from_f64(30.0));
    assert_eq!(calc_day_of_month(&mut g, 2, Ext::from(Ext::from_f64(0.0))), 1); // day 0 -> 1
    assert_eq!(g.tmp_ext1, Ext::from_f64(28.0));
    g.cur_year = 2024;
    calc_day_of_month(&mut g, 2, Ext::from_f64(0.99));
    assert_eq!(g.tmp_ext1, Ext::from_f64(29.0));
    assert_eq!(calc_day_of_month(&mut g, 1, Ext::from(Ext::from_f64(1.0))), 31);
    // month position: 500 steps per month, quarter offset
    g.num_players = 2;
    g.cur_quarter = 0;
    assert_eq!(calc_cur_month_of_year(&mut g, 750), Ext::from_f64(1.5));
    assert_eq!(g.cur_quarter, 1);
    assert_eq!(calc_cur_month_of_year(&mut g, 1), Ext::from_f64(3.0)); // progress < NumPlayers + 1 -> 3
    assert_eq!(calc_cur_month_of_year(&mut g, 4), cast::<Ext, _>(0.01f32));
    g.cur_quarter = 3;
    assert_eq!(calc_cur_month_of_year(&mut g, 2000), Ext::from_f64(9.0));
    g.sim_co_count = 600;
    g.month_name[8] = s("August");
    g.month_name[10] = s("October");
    // corp 1000 at month 2.0 of Q3 = 8.0 > 7.2 -> this quarter, last day of August
    assert_eq!(get_next_earnings_date_str(&mut g, 1000).to_display(), "August 31, 2024");
    // corp 300 (0.6) already passed -> next quarter: 9.6 + ... = October
    assert_eq!(get_next_earnings_date_str(&mut g, 300).to_display(), "October 19, 2024");
}

#[test]
fn t_bills_and_fees() {
    let mut g = Globals::default();
    g.entity_status[200] = 1;
    g.t_bills[200] = Ext::from_f64(10.0);
    g.cash[200] = Ext::from_f64(1.0);
    sell_t_bills_to_cash(&mut g, 200, Ext::from_f64(4.0));
    assert_eq!((g.t_bills[200], g.cash[200]), (Ext::from_f64(6.0), Ext::from_f64(5.0)));
    sell_t_bills_to_cash(&mut g, 200, Ext::from_f64(40.0));
    assert_eq!((g.t_bills[200], g.cash[200]), (Ext::from_f64(0.0), Ext::from_f64(11.0)));
    // overdraft covered from T-bills; then 80% rule moves 15% back to cash (no managing player)
    g.cash[200] = -Ext::from_f64(2.0);
    g.t_bills[200] = Ext::from_f64(12.0);
    balance_cash_t_bills(&mut g, 200);
    assert_eq!(g.t_bills[200], Ext::from_f64(10.0) - Ext::from_f64(10.0) * (cast::<Ext, _>(0.15f32)));
    assert_eq!(g.cash[200], Ext::from_f64(10.0) * (cast::<Ext, _>(0.15f32)));
    // funds advised
    for f in [1585, 1590, 1591] {
        g.entity_status[f] = 1;
        g.industry_num[f] = 71;
        g.fund_adviser[f] = 300;
        g.market_cap[f] = Ext::from_f64(100.0);
    }
    g.fund_adviser[1591] = 301;
    assert_eq!(get_funds_advised_by(&g, 300), 2);
    assert_eq!(get_funds_advised_by(&g, 301), 1591);
    assert_eq!(get_funds_advised_by(&g, 302), 0);
    g.fund_fee_pct[1585] = cast::<f32, _>(Ext::from_f64(0.5));
    g.fund_fee_pct[1590] = cast::<f32, _>(Ext::from_f64(1.5)); // out of range: no fee
    assert_eq!(calc_adviser_fee_income(&g, 300), (cast::<Ext, _>(0.25f32) * Ext::from_f64(0.5)) * Ext::from_f64(100.0));
    // bank deposit cost: factor (50 - (10 - 5)) / 25 = 1.8
    g.profit_margin[20] = 10;
    g.disc_spend_pct[20] = 5;
    g.bank_demand_deposits[20] = Ext::from_f64(1000.0);
    g.bank_c_ds[20] = Ext::from_f64(500.0);
    g.bank_cust_deposits[20] = -Ext::from_f64(3.0);
    assert_eq!(calc_bank_deposit_cost(&g, 20), (cast::<Ext, _>(0.0025f32) * Ext::from_f64(1.8)) * Ext::from_f64(1500.0));
    // accrued interest on a gov bond: half way through the quarter
    g.sim_co_count = 750;
    g.long_bond_desc = s("2040 LONG T-BOND  6.00");
    let a = calc_accrued_bond_interest(&g, 1000, 1);
    assert_eq!(a, ((cast::<Ext, _>(0.0025f32)) * Ext::from_f64(6.0)) * Ext::from_f64(1000.0) * Ext::from_f64(0.5));
    g.credit_rating_num[400] = 1;
    assert_eq!(calc_accrued_bond_interest(&g, 1000, 400), Ext::from_f64(0.0));
}

#[test]
fn goodwill_small_and_negative() {
    let mut g = Globals::default();
    let mut ui = Scripted::default();
    let mut rng = Rng::default();
    g.exchange_rate = cast::<f64, _>(Ext::from_f64(1.0));
    g.goodwill[300] = Ext::from_f64(5.0); // < 10 * ExchangeRate: written off at once
    assert_eq!(goodwill_write_down(&mut g, &mut ui, &mut rng, 300, 0), Ext::from_f64(5.0));
    g.goodwill[300] = -Ext::from_f64(7.0);
    g.extraordinary_pl[300] = 3;
    assert_eq!(goodwill_write_down(&mut g, &mut ui, &mut rng, 300, 1), Ext::from_f64(0.0));
    assert_eq!(g.goodwill[300], -Ext::from_f64(7.0));
    assert_eq!(goodwill_write_down(&mut g, &mut ui, &mut rng, 300, 0), Ext::from_f64(0.0));
    assert_eq!((g.goodwill[300], g.extraordinary_pl[300]), (Ext::from_f64(0.0), 10));
    g.goodwill[300] = Ext::from_f64(100.0);
    g.business_assets[300] = Ext::from_f64(1000.0);
    assert_eq!(goodwill_write_down(&mut g, &mut ui, &mut rng, 300, 0), Ext::from_f64(100.0) * (cast::<Ext, _>(0.054f32)));
    assert_eq!(goodwill_write_down(&mut g, &mut ui, &mut rng, 100, 0), Ext::from_f64(0.0));
}

// ------------------------------------------------------------------------------------- differential

#[test]
fn diff_balance_and_sell_t_bills() {
    par_seeds(1..=4u64, |seed| {
        let mut g = earn_world(seed);
        let mut r = Rand(seed * 77 + 1);
        for k in 0..6 {
            let e = match k {
                0 => cast::<i32, _>(r.range(1, 10)),
                1 => active(&g, &mut r, 11, 130),
                _ => active(&g, &mut r, 131, 1580),
            };
            // overdrafts and T-bill-heavy / cash-heavy mixes
            match k % 3 {
                0 => g.cash[e] = -g.t_bills[e] * Ext::from_f64(0.5) - Ext::from_f64(1.0),
                1 => g.t_bills[e] = g.cash[e].abs() * Ext::from_f64(9.0) + Ext::from_f64(1.0),
                _ => g.t_bills[e] = Ext::from_f64(0.0),
            }
            let Some(o) = original(0x69bb51, &longs(&[e]), &g, 1, &[]) else { return };
            let mut port = g.clone();
            balance_cash_t_bills(&mut port, e);
            cmp().assert(&port, &o.globals);
            assert!(o.log.is_empty(), "{:?}", o.log);

            let amount = g.t_bills[e] * [Ext::from_f64(0.3), Ext::from_f64(1.5), Ext::from_f64(0.0)][k % 3];
            let args = vec![e.to_string(), format!("e:{amount}")];
            let Some(o) = original(0x69bf97, &args, &g, 1, &[]) else { return };
            let mut port = g.clone();
            sell_t_bills_to_cash(&mut port, e, amount);
            cmp().assert(&port, &o.globals);
        }
    });
}

#[test]
fn diff_fees_deposits_accrued_interest() {
    par_seeds(5..=8u64, |seed| {
        let g = earn_world(seed);
        let mut r = Rand(seed * 31 + 7);
        let mut advisers: Vec<i32> = (1581..=1600).map(|f| g.fund_adviser[f]).filter(|&a| a > 0).collect();
        advisers.push(0);
        advisers.push(999);
        for &a in advisers.iter().take(5) {
            let Some(o) = original(0x64d612, &longs(&[a]), &g, 1, &[]) else { return };
            close(calc_adviser_fee_income(&g, a), o.return_float.unwrap(), "CalcAdviserFeeIncome");
            cmp().assert(&g, &o.globals);
            let Some(o) = original(0x64d4fe, &longs(&[a]), &g, 1, &[]) else { return };
            assert_eq!(get_funds_advised_by(&g, a), cast::<i32, _>(o.return_value));
        }
        for _ in 0..3 {
            let b = cast::<i32, _>(r.range(11, 85));
            let Some(o) = original(0x4a34a1, &longs(&[b]), &g, 1, &[]) else { return };
            close(calc_bank_deposit_cost(&g, b), o.return_float.unwrap(), "CalcBankDepositCost");
        }
        for issuer in [1, 2, cast::<i32, _>(r.range(3, 130)), cast::<i32, _>(r.range(131, 1600)), 1500, 1600] {
            let face = r.range(1, 5_000_000);
            let args = vec![format!("q:{face}"), issuer.to_string()];
            let Some(o) = original(0x4a3616, &args, &g, 1, &[]) else { return };
            let port = calc_accrued_bond_interest(&g, face, issuer);
            close(port, o.return_float.unwrap_or(Ext::from_f64(0.0)), "CalcAccruedBondInterest");
        }
    });
}

#[test]
fn diff_calendar() {
    let mut g = earn_world(9);
    for (k, &(sim, q, corp)) in [(600, 3, 1000), (600, 3, 300), (1499, 4, 1500), (5, 1, 200), (1200, 2, 900),
        (100, 1, 1580), (1550, 4, 20), (3, 2, 4)].iter().enumerate()
    {
        g.sim_co_count = sim;
        g.cur_quarter = q;
        g.cur_year = 2020 + cast::<i32, _>(k);
        let Some(o) = original(0x4a07e4, &longs(&[corp]), &g, 1, &[]) else { return };
        let mut port = g.clone();
        let txt = get_next_earnings_date_str(&mut port, corp);
        assert_eq!(Some(txt), o.return_str, "GetNextEarningsDateStr({corp}) sim {sim} q {q}");
        cmp().assert(&port, &o.globals);
        let Some(o) = original(0x5f3697, &longs(&[corp]), &g, 1, &[]) else { return };
        let mut port = g.clone();
        close(calc_cur_month_of_year(&mut port, corp), o.return_float.unwrap(), "CalcCurMonthOfYear");
        cmp().assert(&port, &o.globals);
        for month in [2, 4, 7] {
            let frac = cast::<Ext, _>(corp % 997) / Ext::from_f64(997.0);
            let args = vec![month.to_string(), format!("d:{frac}")];
            let Some(o) = original(0x4a0383, &args, &g, 1, &[]) else { return };
            let mut port = g.clone();
            assert_eq!(calc_day_of_month(&mut port, month, frac), cast::<i32, _>(o.return_value));
            cmp().assert(&port, &o.globals);
        }
    }
}

#[test]
fn diff_goodwill_write_down() {
    par_seeds(10..=13u64, |seed| {
        let mut g = earn_world(seed);
        let mut r = Rand(seed * 13 + 3);
        for k in 0..6 {
            let c = active(&g, &mut r, 131, 1580);
            g.goodwill[c] = match k {
                0 => -(cast::<Ext, _>(r.range(1, 50))) / Ext::from_f64(3.0),
                1 => g.business_assets[c] * Ext::from_f64(1.5) + Ext::from_f64(20.0),
                2..=4 => g.business_assets[c] * Ext::from_f64(3.0) + Ext::from_f64(20.0),
                _ => Ext::from_f64(0.5),
            };
            if k == 3 {
                g.earnings_y1[c] = -Ext::from_f64(1.0);
                g.earnings_ytd[c] = -Ext::from_f64(1.0);
            }
            if k == 4 {
                g.credit_rating_num[c] = 9;
            }
            for preview in [0, 1] {
                let rng0 = 0x1234_5678u32.wrapping_mul(cast::<u32, _>(k) + 1);
                let Some(o) = original(0x5053f8, &longs(&[c, preview]), &g, rng0, &[]) else { return };
                let mut port = g.clone();
                let mut rng = Rng::from_state(rng0);
                let mut ui = Scripted::default();
                let v = goodwill_write_down(&mut port, &mut ui, &mut rng, c, preview);
                close(v, o.return_float.unwrap_or(Ext::from_f64(0.0)), "GoodwillWriteDown");
                cmp().assert(&port, &o.globals);
                assert_eq!(rng.state(), o.rng);
            }
        }
    });
}

#[test]
fn diff_ceo_pay() {
    par_seeds(14..=19u64, |seed| {
        let mut g = earn_world(seed);
        let mut r = Rand(seed * 5 + 11);
        for k in 0..4 {
            let c = active(&g, &mut r, 11, 1580);
            // make a player CEO of it in most runs
            if k < 3 {
                let p = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
                g.player_ceo_of[p] = c;
                if k == 1 {
                    // majority-owning player
                    g.hold_stock_id[(1, p)] = cast::<i16, _>(c);
                    g.hold_stock_pct[(1, p)] = 60;
                }
            }
            if k == 2 {
                g.earnings_ytd[c] = Ext::from_f64(0.0);
            }
            for mode in [0, 1] {
                let rng0 = 99 + cast::<u32, _>(k);
                let Some(o) = original(0x49287b, &longs(&[c, mode]), &g, rng0, &[]) else { return };
                let mut port = g.clone();
                let mut rng = Rng::from_state(rng0);
                let mut ui = Scripted::default();
                let v = calc_ceo_pay(&mut port, &mut ui, &mut rng, c, mode);
                close(v, o.return_float.unwrap_or(Ext::from_f64(0.0)), "CalcCeoPay");
                cmp().assert(&port, &o.globals);
                assert_eq!(rng.state(), o.rng);
            }
        }
    });
}

/// No skips: every callee is ported (the states have no options, futures, swaps or short positions,
/// which CalcEntityFinancials' own PORT-TODOs require).
const PROJ_SKIPS: &[(u32, u32)] = &[];

fn check_projected(g: &Globals, c: i32, rng0: u32) {
    let Some(o) = original(0x494edb, &longs(&[c]), g, rng0, PROJ_SKIPS) else { return };
    let mut port = g.clone();
    let mut rng = Rng::from_state(rng0);
    let mut ui = Scripted::default();
    let v = calc_projected_earnings(&mut port, &mut ui, &mut rng, c);
    close(v, o.return_float.unwrap_or(Ext::from_f64(0.0)), &format!("CalcProjectedEarnings({c})"));
    cmp().assert(&port, &o.globals);
    assert_eq!(rng.state(), o.rng, "RND state after CalcProjectedEarnings({c})");
}

#[test]
fn diff_projected_earnings_corps() {
    par_seeds(20..=25u64, |seed| {
        let g = earn_world(seed);
        let mut r = Rand(seed * 3 + 1);
        for k in 0..4 {
            let c = active(&g, &mut r, 131, 1580);
            check_projected(&g, c, 0x0bad_f00d ^ (cast::<u32, _>(seed) * 7 + k));
        }
    });
}

#[test]
fn diff_projected_earnings_banks_insurers() {
    par_seeds(30..=35u64, |seed| {
        let g = earn_world(seed);
        let mut r = Rand(seed * 3 + 2);
        for k in 0..3 {
            let c = if k == 0 { active(&g, &mut r, 86, 130) } else { active(&g, &mut r, 11, 85) };
            check_projected(&g, c, 0x5eed_0000 + cast::<u32, _>(seed) * 17 + k);
        }
    });
}

#[test]
fn diff_projected_earnings_special_cases() {
    let __content = wsr_core::content::current();
    let mut g = earn_world(40);
    let mut r = Rand(4040);
    // a player CEO in Q4 (salary + bonus), a superfund corp, a just-reported corp, a fund, a player
    g.cur_quarter = 4;
    let c = active(&g, &mut r, 131, 1580);
    g.player_ceo_of[1] = c;
    g.player_name[1] = s(__content.string(13759));
    g.superfund_corp = cast::<i16, _>(c);
    check_projected(&g, c, 7);
    g.sim_co_count = g.sim_co_count.min(1500);
    let c2 = g.sim_co_count + 5;
    g.entity_status[c2] = 1;
    check_projected(&g, c2, 8);
    check_projected(&g, 1582, 9);
    check_projected(&g, 1, 10);
    // tax credits and a tiny goodwill on a corp with no misc data (industry outlook path)
    let c3 = active(&g, &mut r, 131, 1580);
    g.ent_misc_data[c3] = PbStr::new();
    g.tax_credits[c3] = Ext::from_f64(5.0);
    g.goodwill[c3] = Ext::from_f64(2.0);
    g.industry_num[c3] = 12;
    check_projected(&g, c3, 11);
}


/// With interest rate swaps on the projected company (`long|short|fixed|type|start|end|notional`): the
/// projected swap P&L enters ProjReg, and large projected losses of weak companies cancel their swaps
/// (CancelEntitySwaps bankruptcy mode, posting news).
#[test]
fn diff_projected_earnings_with_swaps() {
    par_seeds(50..=55u64, |seed| {
        let mut g = earn_world(seed);
        let mut r = Rand(seed * 11 + 5);
        g.currency_symbol = s("$");
        g.million_prefix_lc = s("m");
        g.million_prefix_uc = s("M");
        g.suppress_popups = 1;
        g.long_bond_yield = cast::<f32, _>((cast::<f32, _>(r.range(200, 1100))) / Ext::from_f64(100.0) + Ext::from_f64(0.003));
        g.short_bond_yield = cast::<f32, _>((cast::<f32, _>(r.range(100, 900))) / Ext::from_f64(100.0) + Ext::from_f64(0.007));
        for i in 1..=60 {
            g.news_headline[i] = s(&format!("Old headline {i}."));
        }
        let c = if seed % 2 == 0 { active(&g, &mut r, 131, 1580) } else { active(&g, &mut r, 11, 130) };
        g.credit_rating_num[c] = [0, 1, 1, 3, 5, 0][cast::<usize, _>(seed - 50)];
        let n = cast::<i32, _>(r.range(2, 6));
        for i in 1..=n {
            let other = active(&g, &mut r, 11, 1580);
            let (a, b) = if r.chance(Ext::from_f64(0.5)) { (c, other) } else { (other, c) };
            let fixed = cast::<Ext, _>(r.range(100, 1500)) / Ext::from_f64(100.0);
            let ty = ["P", "L", "S"][cast::<usize, _>(r.range(0, 2))];
            let notional = r.range(1000, 60000);
            g.swap_contracts[i] = s(&format!(
                "{a}|{b}|{fixed}|{ty}|{}.{}|{}.{}|{notional}",
                g.cur_year - 1,
                r.range(1, 4),
                g.cur_year + cast::<i32, _>(r.range(1, 8)),
                r.range(1, 4)
            ));
        }
        g.swap_contracts[0] = s(&n.to_string());
        check_projected(&g, c, 0xabc0 + cast::<u32, _>(seed));
    });
}
