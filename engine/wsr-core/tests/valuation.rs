//! Tests of `logic::valuation`: hand-checked unit tests plus differential tests against the original
//! routines run under emulation (tools/emu/game.py, see tests/diff/mod.rs).
//!
//! Harness notes:
//! * CheckPriceAlerts (0x6830fc) is skipped in the original (`--skip`). ShowCashFlowProjection (0x6a20e0,
//!   logic::cashflow) runs on both sides; its message boxes are compared by count, `TickerPauseUntil`
//!   (UI only) is ignored.
//! * `TIME$` is fixed by the harness (10:00:00), so the port gets `time_secs = 0`.
//! * CalcEntityFinancials prices bonds with PB_Pow, whose integer-exponent path Unicorn mis-detects; the
//!   states are restricted like tests/finance.rs (`pow_safe`).

#![allow(clippy::field_reassign_with_default)]
use wsr_core::pb::real::{Ext, cast};


mod diff;

use diff::{Compare, Opts, Original, Rand, run_original_with, par_seeds};
use wsr_core::logic::valuation::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::pb::numeric::Cur;
use wsr_core::state::Globals;
use wsr_core::ui::Scripted;

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

const SKIP_ALERTS: (u32, u32) = (0x6830fc, 1);

// ------------------------------------------------------------------------------------------- states

fn f64r(r: &mut Rand, lo: Ext, hi: Ext) -> Ext {
    lo + (hi - lo) * cast::<Ext, _>(r.range(0, 1_000_000_000)) / Ext::from_f64(1e9)
}

fn f32r(r: &mut Rand, lo: Ext, hi: Ext) -> f32 {
    cast::<f32, _>(f64r(r, lo, hi))
}

/// A plausible mid-game world (players 1..NumPlayers, banks 11..85, insurers 86..130, operating corps
/// 131..1580, funds 1581..1583) with balance sheets, earnings history, industry data, the economy and
/// 60-month price histories of every series.
fn world(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut r = Rand(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut g = Globals::default();
    g.world_generated = 1;
    g.game_end_flag = s("N");
    g.num_players = cast::<i32, _>(r.range(1, 4));
    g.difficulty_level = cast::<i32, _>(r.range(1, 4));
    g.cur_year = cast::<i32, _>(r.range(2000, 2030));
    g.start_year = if r.chance(Ext::from_f64(0.2)) { g.cur_year } else { g.cur_year - cast::<i32, _>(r.range(1, 10)) };
    g.cur_quarter = cast::<i32, _>(r.range(1, 4));
    g.sim_co_count = [300, 450][cast::<usize, _>(r.range(0, 1))];
    g.base_interest_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(16.0)));
    g.prime_rate = cast::<f32, _>(g.base_interest_rate + f32r(&mut r, Ext::from_f64(0.0), Ext::from_f64(3.0)));
    g.long_bond_yield = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(12.0)));
    g.short_bond_yield = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(10.0)));
    g.sofr_rate = cast::<f32, _>(g.base_interest_rate + f32r(&mut r, -Ext::from_f64(0.5), Ext::from_f64(0.5)));
    g.gdp_growth = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(25.0), Ext::from_f64(11.0)));
    g.gdp_growth_target = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(15.0), Ext::from_f64(8.0)));
    g.monetary_policy = cast::<i16, _>(r.range(0, 2));
    g.monetary_stance = cast::<i32, _>(r.range(0, 2));
    g.exchange_rate = cast::<f64, _>(if r.chance(Ext::from_f64(0.5)) { Ext::from_f64(1.0) } else { f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(2.0)) });
    g.loan_limit_multiple = cast::<f64, _>((cast::<Ext, _>(r.range(20, 35))) * Ext::from_f64(0.1));
    g.corp_tax_rate = cast::<i32, _>(r.range(15, 45));
    g.bank_cd_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(9.0)));
    g.long_bond_desc = s(&format!("{} LONG T-BOND {:5.2}", g.cur_year + cast::<i32, _>(r.range(8, 30)), f32r(&mut r, Ext::from(Ext::from_f64(2.0)), Ext::from(Ext::from_f64(12.0)))));
    g.short_bond_desc = s(&format!("{} SHORT T-NOTE {:5.2}", g.cur_year + cast::<i32, _>(r.range(0, 9)), f32r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(10.0)))));
    g.long_bond_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(70.0), Ext::from_f64(130.0)));
    g.short_bond_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(85.0), Ext::from_f64(110.0)));
    g.oil_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.1), Ext::from_f64(3.0)));
    g.gold_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(300.0), Ext::from_f64(2500.0)));
    g.silver_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(4.0), Ext::from_f64(40.0)));
    g.wheat_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(12.0)));
    g.corn_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(9.0)));
    g.bitcoin_price = cast::<i32, _>(r.range(100, 90000));
    g.ethereum_price = cast::<i32, _>(r.range(10, 5000));
    g.stock_index = f64r(&mut r, Ext::from_f64(500.0), Ext::from_f64(20000.0));
    g.stock_index_divisor = f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(5.0));
    g.market_valuation = cast::<f32, _>([Ext::from_f64(0.0), Ext::from_f64(0.3), Ext::from_f64(0.8), Ext::from_f64(1.0), Ext::from_f64(1.2), Ext::from_f64(1.9)][cast::<usize, _>(r.range(0, 5))]);
    g.bubble_state = [0, 0, 50, 101, 103, 104, 105, 106, 203, 205][cast::<usize, _>(r.range(0, 9))];
    g.fad_industry = if r.chance(Ext::from_f64(0.5)) { cast::<i32, _>(r.range(3, 69)) } else { 0 };
    g.fad_valuation_pct = if r.chance(Ext::from_f64(0.7)) { cast::<i32, _>(r.range(50, 200)) } else { 0 };
    g.hot_stock_corp = cast::<i16, _>(r.range(131, 1580));
    g.last_fraud_corp = cast::<i32, _>(r.range(131, 1580));
    g.million_prefix_uc = s(["M", "B", "M"][cast::<usize, _>(r.range(0, 2))]);
    g.rand_seed = cast::<i32, _>(r.range(0, 32600));
    g.game_in_progress = if r.chance(Ext::from_f64(0.7)) { 1 } else { 0 };
    g.active_corp_count = cast::<i32, _>(r.range(700, 1300));
    g.cur_player = 1;
    for p in 1..=5 {
        g.player_name[p] = if r.chance(Ext::from_f64(0.1)) { s("...OUT...") } else { s(&format!("{__wsr_text_13556}{p}",__wsr_text_13556=__content.string(13556))) };
    }
    for ind in 1..=71 {
        g.ind_growth_forecast[ind] = cast::<i16, _>(r.range(-20, 60));
        g.ind_long_term_growth[ind] = cast::<i16, _>(r.range(0, 20));
        g.ind_growth_rate[ind] = cast::<i16, _>(r.range(-10, 30));
        g.ind_demand[ind] = r.range(1000, 300_000);
        if (3..=69).contains(&ind) {
            g.ind_demand_gap[ind] = cast::<f64, _>(f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(45.0)));
        }
    }
    g.ind_mgmt_midpoint.as_mut_slice().iter_mut().for_each(|x| *x = cast::<i32, _>(r.range(-5, 20)));
    g.shortage_industry = cast::<i16, _>(r.range(0, 69));
    g.crowded_industry = cast::<i32, _>(r.range(0, 69));
    for e in 1..=1600 {
        let active = if e <= 10 { e <= g.num_players } else { r.chance(Ext::from_f64(0.93)) };
        g.entity_status[e] = if active { 1 } else if r.chance(Ext::from_f64(0.5)) { 0 } else { 2 };
        g.ent_name[e] = s(&format!("{__wsr_text_13758}{e}",__wsr_text_13758=__content.string(13758)));
        g.ent_symbol[e] = s(["XYZ", "BKH", "GOOG", "AAPL", "AMZN", "ABC", "QRS"][cast::<usize, _>(r.range(0, 6))]);
        g.cash[e] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(3000.0));
        g.t_bills[e] = if r.chance(Ext::from_f64(0.5)) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(1000.0)) } else { Ext::from_f64(0.0) };
        g.bank_debt[e] = if r.chance(Ext::from_f64(0.6)) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(2000.0)) } else { Ext::from_f64(0.0) };
        g.lending_bank[e] = cast::<i16, _>(r.range(11, 50));
        g.credit_rating_num[e] = cast::<i32, _>(r.range(0, 10));
        g.corp_controller[e] = match r.range(0, 5) {
            0 => 0,
            1 | 2 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
            _ => cast::<i32, _>(r.range(11, 1600)),
        };
        for slot in 1..=15 {
            if r.chance(Ext::from_f64(0.2)) {
                g.hold_stock_id[(slot, e)] = cast::<i16, _>(r.range(11, 1600));
                g.hold_stock_pct[(slot, e)] = cast::<i16, _>(r.range(1, 100));
            }
        }
        if e > 10 {
            g.industry_num[e] = match e {
                11..=85 => 1,
                86..=130 => 2,
                1581..=1583 => 71,
                _ => {
                    if r.chance(Ext::from_f64(0.04)) {
                        [69, 70][cast::<usize, _>(r.range(0, 1))]
                    } else {
                        cast::<i16, _>(r.range(3, 40))
                    }
                }
            };
            g.market_cap[e] = f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(300.0));
            g.shares_outstanding[e] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(50.0));
            if r.chance(Ext::from_f64(0.03)) {
                g.shares_outstanding[e] = Ext::from_f64(0.0);
            }
            if r.chance(Ext::from_f64(0.03)) {
                g.market_cap[e] = -Ext::from_f64(1.0);
            }
            g.dividend_per_share[e] = if r.chance(Ext::from_f64(0.5)) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(3.0)) } else { Ext::from_f64(0.0) };
            g.tax_loss_carry[e] = if r.chance(Ext::from_f64(0.2)) { f64r(&mut r, -Ext::from_f64(100.0), Ext::from_f64(0.0)) } else { Ext::from_f64(0.0) };
            g.accrued_pl[e] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(200.0));
            g.consol_tax_inc_ytd[e] = f64r(&mut r, -Ext::from_f64(30.0), Ext::from_f64(100.0));
            g.tax_credits[e] = if r.chance(Ext::from_f64(0.2)) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(20.0)) } else { Ext::from_f64(0.0) };
            g.contingency_reserve[e] = if r.chance(Ext::from_f64(0.3)) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(50.0)) } else { Ext::from_f64(0.0) };
            let zero = |r: &mut Rand| r.chance(Ext::from_f64(0.08));
            g.earnings_y1[e] = if zero(&mut r) { Ext::from_f64(0.0) } else { f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(100.0)) };
            g.earnings_yr2_ago[e] = if zero(&mut r) { Ext::from_f64(0.0) } else { f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(100.0)) };
            g.earnings_yr3_ago[e] = if zero(&mut r) { Ext::from_f64(0.0) } else { f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(100.0)) };
            g.earnings_yr4_ago[e] = if zero(&mut r) { Ext::from_f64(0.0) } else { f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(100.0)) };
            g.earnings_ytd[e] = f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(60.0));
            g.proj_qtr_earnings[e] = if zero(&mut r) { Ext::from_f64(0.0) } else { f64r(&mut r, -Ext::from_f64(10.0), Ext::from_f64(30.0)) };
            g.corp_prior_net_worth[e] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(3000.0));
            g.profit_margin[e] = cast::<i16, _>(r.range(-25, 40));
            g.growth_rate[e] = cast::<i16, _>(r.range(-10, 60));
            g.disc_spend_pct[e] = cast::<i16, _>(r.range(0, 15));
            g.investor_sentiment[e] = cast::<i32, _>(r.range(-60, 70));
            g.country_idx[e] = cast::<i32, _>(r.range(0, 25));
            g.ent_misc_data[e] = if r.chance(Ext::from_f64(0.7)) {
                let n = if r.chance(Ext::from_f64(0.6)) { 30 } else { 12 };
                let fields: Vec<String> = (1..=n)
                    .map(|k| match k {
                        1 | 3 => format!("{:.2}", f64r(&mut r, Ext::from(-Ext::from_f64(10.0)), Ext::from(Ext::from_f64(40.0)))),
                        12 => format!("{:.3}", f64r(&mut r, Ext::from(Ext::from_f64(0.0)), Ext::from(Ext::from_f64(5.0)))),
                        29 => r.range(0, 25).to_string(),
                        _ => r.range(0, 99).to_string(),
                    })
                    .collect();
                s(&fields.join("|"))
            } else {
                s(&format!("{:.2}", f64r(&mut r, Ext::from(-Ext::from_f64(5.0)), Ext::from(Ext::from_f64(20.0)))))
            };
            if e >= g.sim_co_count && r.chance(Ext::from_f64(0.3)) {
                g.bonds_issued[e] = r.range(1, 1500);
                let mat = g.cur_year + cast::<i32, _>(r.range(1, 25));
                let coupon = f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(15.0));
                g.bond_issue_info[e] = s(&format!("{mat}|0||{coupon:5.2}"));
                g.bond_price[e] = Cur::from_f64(f64r(&mut r, Ext::from_f64(20.0), Ext::from_f64(130.0)));
            }
        }
        if (131..=1600).contains(&e) {
            g.business_assets[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5000.0));
            g.working_capital[e] = f64r(&mut r, -Ext::from_f64(100.0), Ext::from_f64(1000.0));
        }
        if (11..=85).contains(&e) {
            g.bank_demand_deposits[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(20000.0));
            g.bank_c_ds[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(20000.0));
            g.bank_cust_deposits[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5000.0));
            g.bad_debt_reserve[e] = f64r(&mut r, -Ext::from_f64(5.0), Ext::from_f64(300.0));
            g.bank_consumer_loans[e] = r.range(0, 20000);
            g.bank_mortgage_loans[e] = r.range(0, 20000);
        }
        if (86..=130).contains(&e) {
            g.ins_reserves[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(10000.0));
        }
    }
    // price histories
    for series in 0..=1606 {
        if r.chance(Ext::from_f64(0.05)) {
            continue; // no history yet
        }
        let cur = cast::<i32, _>(r.range(1, 60));
        g.cap_hist_close[(0, series)] = cast::<Ext, _>(cur);
        let months = if r.chance(Ext::from_f64(0.2)) { cast::<i32, _>(r.range(1, 30)) } else { 60 };
        let mut v = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(500.0));
        let trend = f64r(&mut r, -Ext::from_f64(0.06), Ext::from_f64(0.06));
        for k in 0..months {
            let mut slot = cur - k;
            if slot < 1 {
                slot += 60;
            }
            g.cap_hist_close[(slot, series)] = v;
            g.price_hist_high[(slot, series)] = v * f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(1.15));
            g.price_hist_low[(slot, series)] = v * f64r(&mut r, Ext::from_f64(0.85), Ext::from_f64(1.0));
            if r.chance(Ext::from_f64(0.05)) {
                // flat month
            } else {
                v *= Ext::from_f64(1.0) - trend + f64r(&mut r, -Ext::from_f64(0.05), Ext::from_f64(0.05));
            }
        }
    }
    g
}

/// See tests/finance.rs: keep every PB_Pow exponent of the bond pricing integral (harness limitation).
fn pow_safe(g: &mut Globals, seed: u64) {
    g.cur_quarter = if seed.is_multiple_of(2) { 1 } else { 3 };
    g.sim_co_count = if seed.is_multiple_of(3) { 300 } else { 450 };
    for e in 11..g.sim_co_count {
        g.bonds_issued[e] = 0;
    }
}

fn run(addr: u32, args: &[String], g: &Globals, rng: u32, skips: &[(u32, u32)]) -> Option<Original> {
    let opts = Opts { args: args.to_vec(), rng: Some(rng), skips: skips.to_vec(), ..Opts::default() };
    run_original_with(addr, &opts, g)
}

fn longs(a: &[i32]) -> Vec<String> {
    a.iter().map(|x| x.to_string()).collect()
}

/// Float results: EXT in the original, f64 in the port.
#[track_caller]
fn assert_close(port: Ext, orig: Option<Ext>, what: &str) {
    let o = orig.unwrap_or_else(|| panic!("{what}: no float result"));
    assert!((port - o).abs() <= Ext::from_f64(1e-12) * port.abs().max(o.abs()), "{what}: {port} vs {o}");
}

fn cmp() -> Compare {
    Compare::default().rel_tol(Ext::from_f64(1e-12)).ignore("TickerPauseUntil")
}

/// Log lines other than the expected SKIP lines, message boxes and API calls (message boxes are compared
/// by count, see [`orig_msgs`]).
fn unexpected(log: &[String]) -> Vec<&String> {
    log.iter().filter(|l| !l.starts_with("SKIP ") && !l.starts_with("API ") && !l.starts_with("MSGBOX")).collect()
}

/// Message boxes of the original: MSGBOXes and ShowQuickMessage pop-ups (qm.exe).
fn orig_msgs(log: &[String]) -> usize {
    log.iter().filter(|l| l.starts_with("MSGBOX") || l.contains("CreateProcessA(0x0, 'qm.exe")).count()
}

fn active_corps(g: &Globals, r: &mut Rand, n: usize, lo: i64, hi: i64) -> Vec<i32> {
    let mut out = Vec::new();
    let mut tries = 0;
    while out.len() < n && tries < 10000 {
        tries += 1;
        let e = cast::<i32, _>(r.range(lo, hi));
        if g.entity_status[e] == 1 {
            out.push(e);
        }
    }
    out
}

// ------------------------------------------------------------------------------------- unit tests

#[test]
fn count_active_corps_counts_status_1() {
    let mut g = Globals::default();
    g.entity_status[5] = 1; // players are not counted
    g.entity_status[11] = 1;
    g.entity_status[12] = 2;
    g.entity_status[1600] = 1;
    count_active_corps(&mut g);
    assert_eq!(g.active_corp_count, 2);
}

#[test]
fn price_history_basics() {
    let mut g = Globals::default();
    // series 200: month slots 1..5 rising 10, 20, 30, 40, 50, current slot 5
    g.cap_hist_close[(0, 200)] = Ext::from_f64(5.0);
    for m in 1..=5 {
        g.cap_hist_close[(m, 200)] = Ext::from_f64(10.0) * cast::<Ext, _>(m);
    }
    assert_eq!(price_pct_change_months(&g, 200, 1), 25); // 40 -> 50
    assert_eq!(price_pct_change_months(&g, 200, 4), 400); // 10 -> 50
    assert_eq!(price_pct_change_months(&g, 200, 5), 0); // slot 60 empty
    assert_eq!(price_trend_direction(&g, 200, 3, 1), 1);
    assert_eq!(price_trend_direction(&g, 200, 4, 4), 1);
    assert_eq!(price_trend_streak(&g, 200), 4); // 3, then lookback 4 ok, lookback 5 hits slot 60 = 0
    scale_price_history(&mut g, 200, -50);
    assert_eq!(g.cap_hist_close[(5, 200)], (cast::<Ext, _>(0.01f32) * Ext::from_f64(50.0)) * Ext::from_f64(50.0));
    clear_price_history(&mut g, 200);
    assert_eq!(g.cap_hist_close[(0, 200)], Ext::from_f64(0.0));
    assert_eq!(g.cap_hist_close[(5, 200)], Ext::from_f64(0.0));
}

#[test]
fn stock_split_and_index_divisor() {
    let mut g = Globals::default();
    g.entity_status[300] = 1;
    g.market_cap[300] = Ext::from_f64(1000.0);
    g.shares_outstanding[300] = Ext::from_f64(1.0); // price 1000 -> 250 -> 62.5
    g.dividend_per_share[300] = Ext::from_f64(8.0);
    check_stock_split(&mut g, 300);
    assert_eq!(g.shares_outstanding[300], Ext::from_f64(16.0));
    assert_eq!(g.dividend_per_share[300], Ext::from_f64(0.5));

    g.stock_index_divisor = Ext::from_f64(2.0);
    g.entity_status[301] = 1;
    g.market_cap[301] = Ext::from_f64(3000.0); // total 4000; corp 300 +10 % = 100 -> divisor * (1 + 100/3000)
    adjust_index_divisor(&mut g, 300, 10);
    let expect = Ext::from_f64(2.0) * ((Ext::from_f64(10.0) * cast::<Ext, _>(0.01f32)) * Ext::from_f64(1000.0) / Ext::from_f64(3000.0)) + Ext::from_f64(2.0);
    assert_eq!(g.stock_index_divisor, expect);
}

#[test]
fn stock_price_mode_0_formats_price() {
    let mut g = Globals::default();
    g.entity_status[500] = 1;
    g.market_cap[500] = Ext::from_f64(12.345);
    g.shares_outstanding[500] = Ext::from_f64(2.0);
    g.valuation_mode = 0;
    let mut ui = Scripted::default();
    let mut rng = Rng::default();
    calc_stock_price(&mut g, &mut ui, &mut rng, 500, 0);
    assert_eq!(g.stock_price_str, "6.17");
    assert_eq!(g.valuation_mode, 1);
    g.market_cap[500] = Ext::from_f64(0.2);
    g.valuation_mode = 0;
    calc_stock_price(&mut g, &mut ui, &mut rng, 500, 0);
    assert_eq!(g.stock_price_str, "0.25");
    assert_eq!(g.market_cap[500], Ext::from_f64(0.5));
}

// ----------------------------------------------------------------------------- differential tests

#[test]
fn diff_small_statistics() {
    let __content = wsr_core::content::current();
    par_seeds(1..=3u64, |seed| {
        let g = world(seed);
        let Some(o) = run(0x4baa56, &[], &g, 1, &[]) else { return };
        let mut port = g.clone();
        count_active_corps(&mut port);
        cmp().assert(&port, &o.globals);

        let mut r = Rand(seed);
        for corp in active_corps(&g, &mut r, 4, 11, 1600) {
            // CalcDividendYield
            let o = run(0x49b447, &longs(&[corp]), &g, 1, &[]).unwrap();
            let mut port = g.clone();
            let y = calc_dividend_yield(&mut port, corp);
            cmp().assert(&port, &o.globals);
            assert_close(y, o.return_float, &format!("CalcDividendYield({corp})"));
            // GetMgmtMarginVsIndustry modes 0 and 1
            for mode in [0, 1] {
                let o = run(0x46e595, &longs(&[corp, mode]), &g, 1, &[]).unwrap();
                let mut port = g.clone();
                let d = get_mgmt_margin_vs_industry(&mut port, corp, mode);
                cmp().assert(&port, &o.globals);
                assert_eq!(d, cast::<i32, _>(o.return_value), "GetMgmtMarginVsIndustry({corp}, {mode})");
            }
            // CalcWorkingCapitalRatio
            let o = run(0x49b2e3, &longs(&[corp]), &g, 1, &[]).unwrap();
            let mut port = g.clone();
            let w = calc_working_capital_ratio(&mut port, corp);
            cmp().assert(&port, &o.globals);
            assert_close(w, o.return_float, &format!("CalcWorkingCapitalRatio({corp})"));
            // CheckStockSplit (cap scaled up to get splits)
            let mut g2 = g.clone();
            g2.market_cap[corp] *= [Ext::from_f64(1.0), Ext::from_f64(50.0), Ext::from_f64(400.0)][cast::<usize, _>(corp % 3)];
            let o = run(0x494a8e, &longs(&[corp]), &g2, 1, &[]).unwrap();
            let mut port = g2.clone();
            check_stock_split(&mut port, corp);
            cmp().assert(&port, &o.globals);
            // AdjustIndexDivisor
            for pct in [0, [7, -30][cast::<usize, _>(corp % 2)]] {
                let mut g3 = g.clone();
                g3.tmp_ext3 = Ext::from_f64(123.5);
                let o = run(0x480e07, &[format!("{__wsr_text_13741}{corp}",__wsr_text_13741=__content.string(13741)), format!("v:{pct}")], &g3, 1, &[]).unwrap();
                let mut port = g3.clone();
                adjust_index_divisor(&mut port, corp, pct);
                cmp().assert(&port, &o.globals);
            }
        }
    });
}

#[test]
fn diff_industry_statistics() {
    par_seeds(11..=14u64, |seed| {
        let g = world(seed);
        let mut r = Rand(seed);
        for k in 0..5 {
            let ind = cast::<i16, _>([r.range(3, 40), r.range(3, 69), 1, 70, r.range(3, 40)][k]);
            let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
            // CalcIndustryOutlook (BYREF INTEGER)
            let Some(o) = run(0x458261, &[format!("i:{ind}")], &g, rng0, &[]) else { return };
            let mut port = g.clone();
            let mut rng = Rng::from_state(rng0);
            let leader = calc_industry_outlook(&mut port, &mut rng, ind);
            cmp().assert(&port, &o.globals);
            assert_eq!(leader, cast::<i32, _>(o.return_value), "CalcIndustryOutlook({ind}) leader");
            assert_eq!(rng.state(), o.rng, "RND, CalcIndustryOutlook({ind})");
            // CalcIndustryAvgNetMargin (BYVAL INTEGER)
            let o = run(0x4a174b, &[format!("v:{ind}")], &g, rng0, &[]).unwrap();
            let mut port = g.clone();
            let mut rng = Rng::from_state(rng0);
            let m = calc_industry_avg_net_margin(&mut port, &mut rng, ind);
            cmp().assert(&port, &o.globals);
            assert_eq!(Some(cast::<Ext, _>(m)), o.return_float, "CalcIndustryAvgNetMargin({ind})");
            assert_eq!(rng.state(), o.rng);
            // CalcIndustryMarketShare (owner in TmpExt1)
            let mut g2 = g.clone();
            g2.tmp_ext1 = [Ext::from_f64(1.0), Ext::from_f64(2.0), cast::<Ext, _>(r.range(11, 1600))][k % 3];
            let o = run(0x53ecb3, &[format!("i:{ind}")], &g2, rng0, &[]).unwrap();
            let mut port = g2.clone();
            let share = calc_industry_market_share(&mut port, ind);
            cmp().assert(&port, &o.globals);
            assert_eq!(share, cast::<i32, _>(o.return_value), "CalcIndustryMarketShare({ind})");
        }
    });
}

/// A world where some corps control most of their industry (for CalcIndustryMarketShare > 0).
#[test]
fn diff_industry_market_share_owner() {
    let __content = wsr_core::content::current();
    let mut g = world(21);
    for c in 131..=1600 {
        if g.industry_num[c] == 5 && c % 2 == 0 {
            g.corp_controller[c] = 2;
        }
    }
    g.tmp_ext1 = Ext::from_f64(2.0);
    let Some(o) = run(0x53ecb3, &["i:5".to_string()], &g, 1, &[]) else { return };
    let mut port = g.clone();
    let share = calc_industry_market_share(&mut port, 5);
    assert!(share > 10, "{__wsr_text_14365}{share}",__wsr_text_14365=__content.string(14365));
    cmp().assert(&port, &o.globals);
    assert_eq!(share, cast::<i32, _>(o.return_value));
}

#[test]
fn diff_price_history_tick_and_indicators() {
    let mut n = 0;
    let mut nonzero = [0; 4];
    for seed in 31..=33u64 {
        let mut g = world(seed);
        pow_safe(&mut g, seed);
        let mut r = Rand(seed);
        let extra = [[1, 7, 1602, 1606], [2, 9, 1603, 1607], [3, 10, 1601, 1605], [4, 8, 1604, 1606]][cast::<usize, _>(seed % 4)];
        let mut series: Vec<i32> = vec![0, 6, 1604];
        series.extend(extra);
        series.extend(active_corps(&g, &mut r, 4, 11, 1600));
        series.push(cast::<i32, _>(r.range(11, 1600))); // maybe inactive
        for (k, &sid) in series.iter().enumerate() {
            let mut g = g.clone();
            if k % 4 == 1 && sid > 10 && sid <= 1600 {
                g.market_cap[sid] = [Ext::from_f64(0.1), -Ext::from_f64(2.0), Ext::from_f64(50_000.0)][cast::<usize, _>(sid % 3)];
            }
            if k % 5 == 2 {
                g.stock_index = Ext::from_f64(0.0);
                g.oil_price = cast::<f32, _>(Ext::from_f64(0.0));
            }
            let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
            // UpdatePriceHistoryTick
            let Some(o) = run(0x618e2a, &longs(&[sid]), &g, rng0, &[]) else { return };
            let mut port = g.clone();
            let mut rng = Rng::from_state(rng0);
            let mut ui = Scripted::default();
            update_price_history_tick(&mut port, &mut ui, &mut rng, sid);
            assert!(unexpected(&o.log).is_empty(), "{:?}", o.log);
            cmp().assert(&port, &o.globals);
            assert_eq!(rng.state(), o.rng, "RND, UpdatePriceHistoryTick({sid})");

            // pure indicators
            for (lookback, step) in [(3, 1), (12, 3), (6, 6), (60, 1)] {
                let o = run(0x61a9f1, &longs(&[sid, lookback, step]), &g, rng0, &[]).unwrap();
                let d = price_trend_direction(&g, sid, lookback, step);
                nonzero[0] += cast::<usize, _>(d != 0);
                assert_eq!(d, cast::<i32, _>(o.return_value), "PriceTrendDirection({sid}, {lookback}, {step})");
                cmp().assert(&g, &o.globals);
            }
            let o = run(0x61ad2a, &longs(&[sid]), &g, rng0, &[]).unwrap();
            let streak = price_trend_streak(&g, sid);
            nonzero[1] += cast::<usize, _>(streak != 0);
            assert_eq!(streak, cast::<i32, _>(o.return_value), "PriceTrendStreak({sid})");
            for months in [1, 12, 61] {
                let o = run(0x61ae3a, &longs(&[sid, months]), &g, rng0, &[]).unwrap();
                let p = price_pct_change_months(&g, sid, months);
                nonzero[2] += cast::<usize, _>(p != 0);
                assert_eq!(p, cast::<i32, _>(o.return_value), "PricePctChangeMonths({sid}, {months})");
            }
            // CalcTechnicalTrendSignal
            let o = run(0x61b01e, &longs(&[sid]), &g, rng0, &[]).unwrap();
            let mut port = g.clone();
            let mut rng = Rng::from_state(rng0);
            let sig = calc_technical_trend_signal(&mut port, &mut ui, &mut rng, sid);
            cmp().assert(&port, &o.globals);
            assert_eq!(sig, cast::<i32, _>(o.return_value), "CalcTechnicalTrendSignal({sid})");
            nonzero[3] += cast::<usize, _>(sig != 0);
            assert_eq!(rng.state(), o.rng);
            // ScalePriceHistory, ClearPriceHistory
            let o = run(0x619d7d, &longs(&[sid, (sid % 50) - 20]), &g, rng0, &[]).unwrap();
            let mut port = g.clone();
            scale_price_history(&mut port, sid, (sid % 50) - 20);
            cmp().assert(&port, &o.globals);
            let o = run(0x4a389b, &longs(&[sid]), &g, rng0, &[]).unwrap();
            let mut port = g.clone();
            clear_price_history(&mut port, sid);
            cmp().assert(&port, &o.globals);
            n += 1;
        }
    }
    eprintln!("{n} series compared; non-zero direction/streak/pct/signal: {nonzero:?}");
    assert!(n == 0 || nonzero.iter().all(|&k| k > 0));
}

#[test]
fn diff_stock_volatility() {
    par_seeds(41..=44u64, |seed| {
        let mut g = world(seed);
        pow_safe(&mut g, seed);
        let mut r = Rand(seed);
        for (k, corp) in active_corps(&g, &mut r, 6, 11, 1600).into_iter().enumerate() {
            let save = cast::<i32, _>(k % 2);
            let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
            let Some(o) = run(0x605fbc, &longs(&[corp, save]), &g, rng0, &[]) else { return };
            let mut port = g.clone();
            let mut rng = Rng::from_state(rng0);
            let mut ui = Scripted::default();
            let v = calc_stock_volatility(&mut port, &mut ui, &mut rng, corp, save);
            cmp().assert(&port, &o.globals);
            assert_eq!(Some(cast::<Ext, _>(v)), o.return_float, "CalcStockVolatility({corp}, {save})");
        }
    });
}

#[test]
fn diff_roll_price_history_month() {
    par_seeds(51..=54u64, |seed| {
        let mut g = world(seed);
        pow_safe(&mut g, seed);
        if seed == 54 {
            g.stock_index = -Ext::from_f64(1.0); // returns at once
        }
        // Corps > 85 with a non-positive cap would be re-valued by CalcStockPrice, whose skipped
        // ShowCashFlowProjection leaves a stale x87 register as its result in this nested context
        // (harness artifact); keep that case to banks (CalcStockPrice is compared on its own).
        for c in 86..=1600 {
            if g.market_cap[c] <= Ext::from_f64(0.0) {
                g.market_cap[c] = Ext::from_f64(5.0);
            }
        }
        for c in [12, 20, 33] {
            g.entity_status[c] = 1;
            g.market_cap[c] = -Ext::from_f64(1.0);
        }
        let rng0 = 1234 + cast::<u32, _>(seed);
        let Some(o) = run(0x618537, &[], &g, rng0, &[SKIP_ALERTS]) else { return };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0);
        let mut ui = Scripted::default();
        roll_price_history_month(&mut port, &mut ui, &mut rng);
        assert!(unexpected(&o.log).is_empty(), "{:?}", unexpected(&o.log));
        cmp().assert(&port, &o.globals);
        assert_eq!(rng.state(), o.rng, "RND sequence");
    });
}

/// CalcStockPrice in every mode on banks, insurers, corps and funds.
#[test]
fn diff_calc_stock_price() {
    let __content = wsr_core::content::current();
    let mut n = 0;
    let mut moved = 0;
    for seed in 61..=72u64 {
        let mut g = world(seed);
        pow_safe(&mut g, seed);
        let mut r = Rand(seed * 7);
        let mut corps = active_corps(&g, &mut r, 2, 11, 85);
        corps.extend(active_corps(&g, &mut r, 2, 86, 130));
        corps.extend(active_corps(&g, &mut r, 5, 131, 1580));
        corps.extend(active_corps(&g, &mut r, 1, 1581, 1583));
        for (k, corp) in corps.into_iter().enumerate() {
            let mode = [1, 1, 2, 3, 4, 0, 1][(k + cast::<usize, _>(seed)) % 7];
            g.valuation_mode = mode;
            if k % 5 == 3 {
                g.game_end_flag = s("Y");
            } else {
                g.game_end_flag = s("N");
            }
            if k % 3 == 0 {
                g.sel_entity = corp;
            }
            if k % 4 == 0 {
                g.tmp_ext1 = -Ext::from_f64(99.0);
            }
            let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
            let Some(o) = run(0x42c72a, &longs(&[corp]), &g, rng0, &[SKIP_ALERTS]) else {
                return;
            };
            let mut port = g.clone();
            let mut rng = Rng::from_state(rng0);
            let mut ui = Scripted::default();
            calc_stock_price(&mut port, &mut ui, &mut rng, corp, 0);
            assert!(unexpected(&o.log).is_empty(), "corp {corp}: {:?}", unexpected(&o.log));
            let port_msgs = ui.log.iter().filter(|l| l.starts_with("MSGBOX")).count();
            assert_eq!(port_msgs, orig_msgs(&o.log), "message boxes, corp {corp}: {:?}", ui.log);
            cmp().assert(&port, &o.globals);
            assert_eq!(rng.state(), o.rng, "RND sequence, corp {corp}{__wsr_text_13548}{mode}",__wsr_text_13548=__content.string(13548));
            if port.market_cap[corp] != g.market_cap[corp] {
                moved += 1;
            }
            eprintln!(
                "corp {corp} ind {}{__wsr_text_13548}{mode}: cap {:.4} -> {:.4} ({:?} {:?})",
                g.industry_num[corp], g.market_cap[corp], port.market_cap[corp], port.stock_price_str, port.ticker_change_str
            ,__wsr_text_13548=__content.string(13548));
            n += 1;
            g = o.globals; // chain, like the game
        }
    }
    eprintln!("{n}{__wsr_text_14366}{moved} moved the cap",__wsr_text_14366=__content.string(14366));
    assert!(n == 0 || moved > n / 3);
}

#[test]
fn diff_calc_analyst_rating() {
    par_seeds(81..=86u64, |seed| {
        let mut g = world(seed);
        pow_safe(&mut g, seed);
        let mut r = Rand(seed);
        let mut corps = active_corps(&g, &mut r, 4, 11, 1580);
        corps.extend(active_corps(&g, &mut r, 2, 1581, 1583));
        for corp in corps {
            let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
            let Some(o) = run(0x494007, &longs(&[corp]), &g, rng0, &[SKIP_ALERTS]) else {
                return;
            };
            let mut port = g.clone();
            let mut rng = Rng::from_state(rng0);
            let mut ui = Scripted::default();
            let rating = calc_analyst_rating(&mut port, &mut ui, &mut rng, corp);
            cmp().assert(&port, &o.globals);
            assert_eq!(rating, cast::<i32, _>(o.return_value), "CalcAnalystRating({corp})");
            assert_eq!(rng.state(), o.rng);
            g = o.globals;
        }
    });
}
