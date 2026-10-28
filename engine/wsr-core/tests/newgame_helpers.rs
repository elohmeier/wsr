//! Tests of `logic::newgame_helpers`: hand-checked unit tests plus differential tests against the original
//! routines run under emulation (tools/emu/game.py, see tests/diff/mod.rs).
//!
//! The world builder is copied from tests/valuation.rs (PB_Pow harness limitation: `pow_safe`).
//! CheckPriceAlerts (0x6830fc) is skipped in the original; `PriceAlerts` are empty, so the port's call has
//! no effect either.

#![allow(clippy::field_reassign_with_default)]
use wsr_core::pb::real::{Ext, cast};


mod diff;

use diff::{par_seeds, run_original_with, Compare, Opts, Original, Rand};
use wsr_core::logic::newgame_helpers::*;
use wsr_core::logic::sim_entity::bonds::issue_bonds;
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

// ------------------------------------------------------------------------------------- unit tests

#[test]
fn country_names_from_data_table() {
    let __content = wsr_core::content::current();
    let mut g = Globals::default();
    load_country_names(&mut g);
    assert_eq!(g.country_name[0], __content.string(13334));
    assert_eq!(g.country_display_name[0], __content.string(13921));
    assert_eq!(g.country_name[11], __content.string(13922));
    assert_eq!(g.country_display_name[11], "PRAGUE");
    assert_eq!(g.country_name[53], "VIETNAM");
    assert_eq!(g.country_display_name[53], "HANOI");
    assert_eq!(g.country_name[54], "");
}

#[test]
fn month_names() {
    let __content = wsr_core::content::current();
    let mut g = Globals::default();
    init_month_names(&mut g);
    assert_eq!(g.month_name[1], "January");
    assert_eq!(g.month_name[12], __content.string(6799));
    assert_eq!(g.month_name[0], "");
}

#[test]
fn famous_corp_tweaks_unit() {
    let mut g = Globals::default();
    g.exchange_rate = cast::<f64, _>(Ext::from_f64(1.0));
    g.shares_outstanding[140] = Ext::from_f64(10.0);
    g.ent_symbol[140] = s("TSLA");
    g.cash[140] = Ext::from_f64(100.0);
    let mut ui = Scripted::default();
    let mut rng = Rng::from_state(12345);
    apply_famous_corp_tweaks(&mut g, &mut ui, &mut rng, 140);
    assert_eq!(g.cash[140], Ext::from_f64(50100.0));
    assert!((10..=21).contains(&g.profit_margin[140]));
    // not a famous id: nothing happens
    let before = g.clone();
    g.shares_outstanding[141] = Ext::from_f64(10.0);
    g.cash[141] = Ext::from_f64(1.0);
    apply_famous_corp_tweaks(&mut g, &mut ui, &mut rng, 141);
    assert_eq!(g.cash[141], Ext::from_f64(1.0));
    assert_eq!(g.valuation_mode, before.valuation_mode);
}

#[test]
fn pick_etf_holding_stock_checks_given_stock() {
    let mut g = Globals::default();
    let mut ui = Scripted::default();
    let mut rng = Rng::from_state(1);
    g.industry_num[200] = 3;
    g.industry_num[201] = 70;
    g.industry_num[202] = 4;
    assert_eq!(pick_etf_holding_stock(&mut g, &mut ui, &mut rng, 1586, 200), 200);
    assert_eq!(pick_etf_holding_stock(&mut g, &mut ui, &mut rng, 1586, 201), 201);
    assert_eq!(pick_etf_holding_stock(&mut g, &mut ui, &mut rng, 1586, 202), 0);
    assert_eq!(pick_etf_holding_stock(&mut g, &mut ui, &mut rng, 1585, 200), 0);
    g.country_idx[202] = 27;
    assert_eq!(pick_etf_holding_stock(&mut g, &mut ui, &mut rng, 1593, 202), 202);
}

// ------------------------------------------------------------------------------- differential tests

/// Run `f` on the port and compare state, RND state and message-box count with the original.
#[track_caller]
fn check(addr: u32, args: &[String], g: &Globals, rng0: u32, f: impl FnOnce(&mut Globals, &mut Scripted, &mut Rng) -> i32, ret: bool) -> Option<Original> {
    let __content = wsr_core::content::current();
    let o = run(addr, args, g, rng0, &[SKIP_ALERTS])?;
    let mut port = g.clone();
    let mut rng = Rng::from_state(rng0);
    let mut ui = Scripted::default();
    let rv = f(&mut port, &mut ui, &mut rng);
    assert!(unexpected(&o.log).is_empty(), "{__wsr_text_13638}{addr:x}: {:?}", unexpected(&o.log),__wsr_text_13638=__content.string(13638));
    let port_msgs = ui.log.iter().filter(|l| l.starts_with("MSGBOX")).count();
    assert_eq!(port_msgs, orig_msgs(&o.log), "message boxes 0x{addr:x}: {:?} / {:?}", ui.log, o.log);
    cmp().assert(&port, &o.globals);
    assert_eq!(rng.state(), o.rng, "RND sequence 0x{addr:x}");
    if ret {
        assert_eq!(rv, cast::<i32, _>(o.return_value), "result of 0x{addr:x}");
    }
    eprintln!("{__wsr_text_13638}{addr:x}: {:.2}{__wsr_text_13748}", o.elapsed,__wsr_text_13638=__content.string(13638), __wsr_text_13748=__content.string(13748));
    Some(o)
}

#[test]
fn diff_name_tables() {
    let g = Globals::default();
    check(0x47ceaf, &[], &g, 1, |g, _, _| { load_country_names(g); 0 }, false);
    check(0x4a010e, &[], &g, 1, |g, _, _| { init_month_names(g); 0 }, false);
}

#[test]
fn diff_apply_famous_corp_tweaks() {
    par_seeds(1..=2u64, |seed| {
        let mut g = world(seed);
        pow_safe(&mut g, seed);
        let mut r = Rand(seed * 31);
        let famous = [(140, "TSLA"), (423, "SPCX"), (621, "AAPL"), (954, "GOOG"), (1499, "BKH"), (620, "IBM"), (1431, "TOY")];
        let extra = cast::<i32, _>(r.range(131, 1500));
        for (corp, sym) in famous.iter().copied().chain([(extra, "XYZ")]) {
            g.entity_status[corp] = 1;
            if r.chance(Ext::from_f64(0.8)) {
                g.ent_symbol[corp] = s(sym);
            }
            if r.chance(Ext::from_f64(0.6)) {
                g.cash[corp] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(3000.0));
            } else {
                g.cash[corp] = f64r(&mut r, Ext::from_f64(3000.0), Ext::from_f64(200000.0));
            }
            if r.chance(Ext::from_f64(0.1)) {
                g.shares_outstanding[corp] = Ext::from_f64(0.0);
            }
            let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
            let Some(o) = check(0x47c2c5, &longs(&[corp]), &g, rng0, |g, ui, rng| { apply_famous_corp_tweaks(g, ui, rng, corp); 0 }, false) else {
                return;
            };
            g = o.globals;
        }
    });
}

#[test]
fn diff_init_convertible_bond_issues() {
    let __content = wsr_core::content::current();
    par_seeds(11..=16u64, |seed| {
        let mut g = world(seed);
        pow_safe(&mut g, seed);
        let mut r = Rand(seed * 7);
        g.borrow_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(15.0)));
        g.start_year = g.cur_year;
        for e in 11..=1500 {
            g.credit_rating_num[e] = cast::<i32, _>(r.range(0, 10));
            if r.chance(Ext::from_f64(0.3)) {
                g.bonds_issued[e] = r.range(1, 1500);
                let mat = g.cur_year + cast::<i32, _>(r.range(1, 25));
                let coupon = f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(15.0));
                g.bond_issue_info[e] = if r.chance(Ext::from_f64(0.3)) { s(&format!("{mat}cv|123.45|@{coupon:05.2}")) } else { s(&format!("{mat}|0||{coupon:5.2}")) };
            } else {
                g.bonds_issued[e] = 0;
            }
        }
        let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
        let Some(o) = check(0x47d87a, &[], &g, rng0, |g, ui, rng| { init_convertible_bond_issues(g, ui, rng); 0 }, false) else {
            return;
        };
        let changed = (11..=1500).filter(|&e| o.globals.bond_issue_info[e] != g.bond_issue_info[e]).count();
        eprintln!("seed {seed}: {changed}{__wsr_text_13923}",__wsr_text_13923=__content.string(13923));
    });
}

/// A world with only `n` active companies in 86..1500 (each emulated valuation is expensive).
fn sparse(g: &mut Globals, r: &mut Rand, n: usize) {
    let keep: Vec<i32> = (0..n).map(|_| cast::<i32, _>(r.range(86, 1500))).collect();
    for e in 86..=1500 {
        if !keep.contains(&e) {
            g.entity_status[e] = 0;
        }
    }
}

#[test]
fn diff_init_earnings_history() {
    par_seeds(21..=24u64, |seed| {
        let mut g = world(seed);
        pow_safe(&mut g, seed);
        let mut r = Rand(seed * 13);
        sparse(&mut g, &mut r, 12);
        g.game_in_progress = 0;
        let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
        check(0x47d0c7, &[], &g, rng0, |g, ui, rng| { init_earnings_history(g, ui, rng, 0); 0 }, false);
    });
}

/// Every industry 3..70 among the operating companies and fewer cross-holdings (so enough companies are
/// at least 11% publicly held): otherwise some sector funds find no stock and InitETFs never ends.
fn etf_world(g: &mut Globals, r: &mut Rand) {
    for e in 131..=1580 {
        g.industry_num[e] = cast::<i16, _>(r.range(3, 70));
        g.country_idx[e] = cast::<i32, _>(r.range(0, 53));
    }
    for e in 1..=1600 {
        for slot in 1..=15 {
            if g.hold_stock_id[(slot, e)] != 0 && r.chance(Ext::from_f64(0.8)) {
                g.hold_stock_id[(slot, e)] = 0;
                g.hold_stock_pct[(slot, e)] = 0;
            }
        }
    }
}

/// Fund slots: 1581..1583 as in the world, the rest free (status 0 or 2) or partly in use.
fn free_funds(g: &mut Globals, r: &mut Rand, lo: i32) {
    for e in lo..=1600 {
        g.entity_status[e] = if r.chance(Ext::from_f64(0.7)) { 0 } else { -1 };
        g.industry_num[e] = 0;
        for slot in 0..=15 {
            g.hold_stock_id[(slot, e)] = 0;
            g.hold_stock_pct[(slot, e)] = 0;
        }
    }
}

#[test]
fn diff_init_etfs() {
    par_seeds(31..=34u64, |seed| {
        let mut g = world(seed);
        pow_safe(&mut g, seed);
        let mut r = Rand(seed * 17);
        etf_world(&mut g, &mut r);
        free_funds(&mut g, &mut r, if seed % 2 == 0 { 1581 } else { 1584 });
        g.game_in_progress = cast::<i32, _>(seed % 3 == 0);
        let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
        let Some(o) = check(0x648a05, &[], &g, rng0, |g, ui, rng| { init_etfs(g, ui, rng); 0 }, false) else {
            return;
        };
        let held: i32 = (1586..=1600).map(|e| cast::<i32, _>(o.globals.hold_stock_id[(0, e)])).sum();
        eprintln!("seed {seed}: {held} fund holdings");
    });
}

#[test]
fn diff_pick_etf_holding_stock() {
    par_seeds(41..=44u64, |seed| {
        let mut g = world(seed);
        pow_safe(&mut g, seed);
        let mut r = Rand(seed * 19);
        etf_world(&mut g, &mut r);
        for k in 0..8 {
            let etf = cast::<i32, _>(r.range(1580, 1600));
            let stock = match k % 3 {
                0 => 0,
                1 => cast::<i32, _>(r.range(1, 10)),
                _ => cast::<i32, _>(r.range(11, 1580)),
            };
            let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
            let Some(o) = check(0x649382, &longs(&[etf, stock]), &g, rng0, |g, ui, rng| pick_etf_holding_stock(g, ui, rng, etf, stock), true) else {
                return;
            };
            eprintln!("PickEtfHoldingStock({etf}, {stock}) = {}", o.return_value);
            g = o.globals;
        }
    });
}

#[test]
fn diff_upgrade_save_add_etfs() {
    par_seeds(51..=53u64, |seed| {
        let mut g = world(seed);
        pow_safe(&mut g, seed);
        let mut r = Rand(seed * 23);
        etf_world(&mut g, &mut r);
        match seed {
            51 => free_funds(&mut g, &mut r, 1581), // pre-6.30 save: all 20 funds are created
            52 => {
                free_funds(&mut g, &mut r, 1581); // 15-fund save: 1586..1600 exist
                for e in 1586..=1600 {
                    g.entity_status[e] = 1;
                    g.industry_num[e] = 71;
                }
                g.ent_symbol[300] = s("JUNK");
            }
            _ => {} // current save: nothing to do
        }
        g.game_in_progress = 1;
        let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
        check(0x649fc9, &[], &g, rng0, |g, ui, rng| { upgrade_save_add_etfs(g, ui, rng); 0 }, false);
    });
}

// ------------------------------------------------------------------------------- config file tests

fn cfg_files(cfg: Option<&str>) -> impl Fn(&str) -> Option<Vec<u8>> + '_ {
    move |name: &str| match name.to_ascii_uppercase().as_str() {
        "CFIG.WSR" => cfg.map(|c| c.as_bytes().to_vec()),
        _ => Some(Vec::new()),
    }
}

#[test]
fn config_round_trip() {
    let mut g = Globals::default();
    g.pref_player_names[2] = s("Alice");
    g.chart_size = 3;
    let data = save_config(&mut g);
    let text = String::from_utf8(data.clone()).unwrap();
    assert!(text.starts_with("USA| 50|Wally Raider|Alice|Oddlot Roger|Gordon Gekko|Tony Stark| 2| 500| 2| 2| 0|"), "{text}");
    assert!(text.ends_with("|\r\n"));
    let mut h = Globals::default();
    let mut ui = Scripted::default();
    let files = |name: &str| if name == "cfig.wsr" { Some(data.clone()) } else { Some(Vec::new()) };
    load_config_and_check_files(&mut h, &mut ui, &files);
    assert_eq!(h.config_loaded, "Y");
    assert_eq!(h.pref_player_names[2], "Alice");
    assert_eq!(h.chart_size, 3);
    assert_eq!(h.ticker_speed, 50);
    assert_eq!(h.difficulty_level, 2);
    assert!(ui.log.is_empty());
}

#[test]
fn config_missing_files() {
    let mut g = Globals::default();
    let mut ui = Scripted::default();
    let files = |name: &str| if name == "MSCNEWS.DAT" { Some(Vec::new()) } else { None };
    load_config_and_check_files(&mut g, &mut ui, &files);
    assert_eq!(g.game_end_flag, "Y");
    assert_eq!(ui.log.len(), 2, "{:?}", ui.log);
    let mut g = Globals::default();
    let mut ui = Scripted::default();
    load_config_and_check_files(&mut g, &mut ui, &cfg_files(None));
    assert_eq!(g.config_loaded, "N");
    assert_eq!(g.ticker_speed, 50);
}

#[test]
fn default_industry_for_slot() {
    assert_eq!(get_default_industry_for_slot(5), 0);
    assert_eq!(get_default_industry_for_slot(11), 1);
    assert_eq!(get_default_industry_for_slot(85), 1);
    assert_eq!(get_default_industry_for_slot(86), 2);
    assert_eq!(get_default_industry_for_slot(131), 3);
    assert_eq!(get_default_industry_for_slot(1449), 68);
    assert_eq!(get_default_industry_for_slot(1450), 69);
    assert_eq!(get_default_industry_for_slot(1471), 70);
    assert_eq!(get_default_industry_for_slot(1580), 70);
    assert_eq!(get_default_industry_for_slot(1581), 71);
}

#[test]
fn diff_load_config_and_check_files() {
    let __content = wsr_core::content::current();
    let cases: Vec<(Option<&str>, bool)> = vec![
        (None, true),
        (Some("EUR| 30|A|B|C|D|E|\r\n"), true),
        (Some("EUR| 30|A|B|C|D|E| 3 | 800 | 4 | 4 |\r\n"), true),
        (Some("UK|75|P1|P2|P3|P4|P5|1|200|10|1|1|0|1|1|2|1|1|0|7|1|1|9|8|\r\n"), true),
        (Some("UK|75|P1|P2|P3|P4|P5|1|200|10|1|1|0|1|1|2|1|1|0|7|1|\r\n"), true),
        (Some("USA|50|x|y\r\n"), true),
        (Some(""), true),
        (None, false),
    ];
    par_seeds(0..cast::<u64, _>(cases.len()), |k| {
        let (cfg, all) = cases[cast::<usize, _>(k)];
        let mut g = Globals::default();
        g.exe_name = s("WSR.EXE");
        g.app_title = s(__content.string(13661));
        g.ticker_speed = 7;
        let mut files: Vec<(String, Vec<u8>)> = Vec::new();
        if let Some(c) = cfg {
            files.push(("cfig.wsr".into(), c.as_bytes().to_vec()));
        }
        if all {
            for n in ["SCORES.WSR", "WSR101.DAT", "MSCNEWS.DAT"] {
                files.push((n.into(), b"x".to_vec()));
            }
        } else {
            files.push(("MSCNEWS.DAT".into(), b"x".to_vec()));
        }
        let opts = Opts { files: files.clone(), no_default_files: true, ..Opts::default() };
        let Some(o) = run_original_with(0x46bfff, &opts, &g) else { return };
        let mut port = g.clone();
        let mut ui = Scripted::default();
        let lookup = |name: &str| files.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, d)| d.clone());
        load_config_and_check_files(&mut port, &mut ui, &lookup);
        let port_msgs = ui.log.iter().filter(|l| l.starts_with("MSGBOX")).count();
        assert_eq!(port_msgs, orig_msgs(&o.log), "{__wsr_text_13924}{k}: {:?} / {:?}", ui.log, o.log,__wsr_text_13924=__content.string(13924));
        cmp().assert(&port, &o.globals);
    });
}

#[test]
fn diff_save_config() {
    let __content = wsr_core::content::current();
    par_seeds(1..=4u64, |seed| {
        let mut r = Rand(seed);
        let mut g = Globals::default();
        g.data_path = s("C:\\WSR\\");
        if seed > 1 {
            g.pref_currency = s(["", "EUR", "UK"][cast::<usize, _>(r.range(0, 2))]);
            g.currency_code = s(["", "JAP"][cast::<usize, _>(r.range(0, 1))]);
            g.num_players = cast::<i32, _>(r.range(0, 4));
            g.start_net_worth_cfg = cast::<i32, _>(r.range(0, 2000));
            g.difficulty_level = cast::<i32, _>(r.range(0, 3));
            g.cfg_difficulty_raw = cast::<i32, _>(r.range(0, 3));
            g.ticker_speed = cast::<i32, _>(r.range(0, 99));
            g.chart_size = cast::<i32, _>(r.range(0, 3));
            g.cfg_field24 = -(cast::<i32, _>(r.range(0, 3)));
            for i in 1..=5 {
                if r.chance(Ext::from_f64(0.5)) {
                    g.pref_player_names[i] = s(&format!("{__wsr_text_13925}{i}",__wsr_text_13925=__content.string(13925)));
                }
            }
        }
        let opts = Opts::default();
        let Some(o) = run_original_with(0x46cdb0, &Opts { args: vec![format!("s:{}", g.pref_currency)], ..opts }, &g) else {
            return;
        };
        let mut port = g.clone();
        let data = save_config(&mut port);
        let orig = o.files.get("CFIG.WSR").cloned().flatten().expect(__content.string(13719));
        assert_eq!(String::from_utf8_lossy(&data), String::from_utf8_lossy(&orig));
        // the harness passes a copy of PrefCurrency, not the global itself
        let mut cmp_g = o.globals.clone();
        cmp_g.pref_currency = port.pref_currency.clone();
        cmp().assert(&port, &cmp_g);
    });
}

// --------------------------------------------------------------------- entity slots, banks, insurers

/// Free slots (status 0, default industry) here and there, so FindFreeEntitySlot finds some.
fn free_slots(g: &mut Globals, r: &mut Rand) {
    for e in 11..=1580 {
        if r.chance(Ext::from_f64(0.15)) {
            g.entity_status[e] = 0;
            g.industry_num[e] = get_default_industry_for_slot(e);
            g.country_idx[e] = cast::<i32, _>(r.range(0, 53));
        }
    }
}

#[test]
fn diff_find_free_entity_slot() {
    par_seeds(61..=62u64, |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed * 29);
        free_slots(&mut g, &mut r);
        for k in 0..12 {
            let (id, ind) = match k % 4 {
                0 => (cast::<i32, _>(r.range(11, 85)), 1i16),
                1 => (cast::<i32, _>(r.range(86, 130)), 2),
                2 => (cast::<i32, _>(r.range(131, 1500)), cast::<i16, _>(r.range(3, 70))),
                _ => (cast::<i32, _>(r.range(1490, 1580)), 70),
            };
            let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
            let args = vec![id.to_string(), format!("i:{ind}")];
            let Some(o) = check(0x504a99, &args, &g, rng0, |g, _, rng| find_free_entity_slot(g, rng, id, ind), true) else { return };
            g = o.globals;
        }
    });
}

/// Entities to clear: banks, insurers, companies, funds; some are a player's CEO post, have lawsuits,
/// options, cheat targets or recent offerings.
fn clear_setup(g: &mut Globals, r: &mut Rand, ids: &[i32]) {
    let __content = wsr_core::content::current();
    for (k, &id) in ids.iter().enumerate() {
        if k % 3 == 0 {
            g.player_ceo_of[1 + cast::<i32, _>(k) % 5] = id;
            g.player_ceo_since_year[1 + cast::<i32, _>(k) % 5] = 1999;
        }
        g.lawsuit_rec[1 + cast::<i32, _>(k) % 10] = s(&format!("{}|{}|{}|40|", if k % 4 == 0 { 2 } else { 1 }, id, r.range(11, 1600)));
        g.recent_offerings[1 + cast::<i32, _>(k) % 10] = if k % 2 == 0 { -id } else { id };
        g.player_advances[(1, id)] = Ext::from_f64(5.0);
    }
    g.cheat_eq_fund_target = cast::<i16, _>(ids[0]);
    g.adv_offering_corp = cast::<i16, _>(ids[ids.len() - 1]);
    let mut n = 0;
    for i in 1..=40 {
        let corp = if i % 3 == 0 { ids[cast::<usize, _>(i) % ids.len()] } else { cast::<i32, _>(r.range(11, 1600)) };
        g.option_recs[i] = s(&format!("1|CALL|{corp}|50|2025|3|100|2.5"));
        n += 1;
    }
    g.option_recs[0] = s(&n.to_string());
    for e in 1581..=1600 {
        if g.entity_status[e] == 1 {
            g.industry_num[e] = 71;
            g.fund_adviser[e] = cast::<i32, _>(r.range(86, 1580));
        }
    }
    for &id in ids {
        if (86..=1580).contains(&id) {
            g.fund_adviser[1581 + id % 3] = id; // advises an ETF
        }
    }
    for i in 1..=5 {
        g.ent_name[i] = s(&format!("{__wsr_text_13556}{i}",__wsr_text_13556=__content.string(13556)));
    }
}

#[test]
fn diff_clear_entity_record() {
    par_seeds(71..=73u64, |seed| {
        let mut g = world(seed);
        pow_safe(&mut g, seed);
        let mut r = Rand(seed * 37);
        let ids: Vec<i32> = vec![cast::<i32, _>(r.range(11, 85)), cast::<i32, _>(r.range(86, 130)), cast::<i32, _>(r.range(131, 1580)), cast::<i32, _>(r.range(1581, 1583)), cast::<i32, _>(r.range(131, 1580))];
        clear_setup(&mut g, &mut r, &ids);
        for &id in &ids {
            let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
            let Some(o) = check(0x49321d, &longs(&[id]), &g, rng0, |g, ui, rng| { clear_entity_record(g, ui, rng, id); 0 }, false) else { return };
            g = o.globals;
        }
    });
}

/// PurgeLawsuitsForEntity raises error 9 for an entity listed in RecentAntitrustFilers; the original
/// carries on (no handler in PurgeLawsuitsForEntity, ClearEntityRecord's handler does not fire).
#[test]
fn diff_clear_entity_record_error_path() {
    let mut g = world(75);
    pow_safe(&mut g, 75);
    let mut r = Rand(75);
    let id = 300;
    clear_setup(&mut g, &mut r, &[id]);
    g.recent_antitrust_filers[4] = id;
    let rng0 = 12345;
    let Some(o) = run(0x49321d, &longs(&[id]), &g, rng0, &[SKIP_ALERTS]) else { return };
    let mut port = g.clone();
    let mut rng = Rng::from_state(rng0);
    let mut ui = Scripted::default();
    clear_entity_record(&mut port, &mut ui, &mut rng, id);
    assert!(o.log.iter().any(|l| l.starts_with("PBERR 9")), "{:?}", o.log);
    assert!(!o.log.iter().any(|l| l.starts_with("MSGBOX")), "{:?}", o.log);
    cmp().assert(&port, &o.globals);
    assert_eq!(rng.state(), o.rng);
}

/// A world where banks 61..85 have free slots and CreateBank's loan purchases can happen: weak borrowers
/// of established banks with many borrowers.
fn bank_world(seed: u64) -> (Globals, Rand) {
    let mut g = world(seed);
    pow_safe(&mut g, seed);
    let mut r = Rand(seed * 41);
    free_slots(&mut g, &mut r);
    for e in 86..=1600 {
        if r.chance(Ext::from_f64(0.5)) {
            g.lending_bank[e] = cast::<i16, _>(r.range(11, 20));
            g.bank_debt[e] = f64r(&mut r, Ext::from_f64(260.0), Ext::from_f64(3000.0));
            g.cash[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(200.0));
            if r.chance(Ext::from_f64(0.3)) {
                // a loan the new bank can afford to buy (7..15k of debt) from a well-rated borrower
                g.bank_debt[e] = f64r(&mut r, Ext::from_f64(7000.0), Ext::from_f64(15000.0));
                g.business_assets[e] = f64r(&mut r, Ext::from_f64(20000.0), Ext::from_f64(80000.0));
                g.working_capital[e] = f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(5000.0));
                g.corp_controller[e] = if r.chance(Ext::from_f64(0.5)) { 0 } else { cast::<i32, _>(r.range(11, 1600)) };
            }
        }
    }
    for b in 11..=20 {
        g.entity_status[b] = 1;
        g.corp_controller[b] = b;
        g.bank_consumer_loans[b] = 20000;
    }
    g.max_bond_issue = 1_000_000;
    g.news_msg = s("");
    (g, r)
}

#[test]
fn diff_create_bank() {
    let __content = wsr_core::content::current();
    par_seeds(81..=84u64, |seed| {
        let (mut g, mut r) = bank_world(seed);
        let mode = [0, 1, 2, 2][cast::<usize, _>(seed % 4)];
        let id = cast::<i32, _>(r.range(61, 85));
        g.entity_status[id] = 0;
        g.industry_num[id] = 1;
        g.tmp_ext2 = f64r(&mut r, Ext::from_f64(3000.0), Ext::from_f64(12000.0));
        g.base_interest_rate = cast::<f32, _>(if seed % 2 == 0 { Ext::from_f64(9.0) } else { Ext::from_f64(5.0) });
        let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
        let args = vec![id.to_string(), mode.to_string()];
        let Some(o) = check(0x4e3588, &args, &g, rng0, |g, ui, rng| { create_bank(g, ui, rng, id, mode); 0 }, false) else { return };
        let bought = (86..=1600).filter(|&e| cast::<i32, _>(o.globals.lending_bank[e]) == id && cast::<i32, _>(g.lending_bank[e]) != id).count();
        eprintln!("{__wsr_text_13926}{id}, {mode}) rate {}: {bought} loans bought, bonds {:?}", g.base_interest_rate, o.globals.bond_issue_info[id],__wsr_text_13926=__content.string(13926));
    });
}

#[test]
fn diff_create_new_bank_in_free_slot() {
    par_seeds(91..=92u64, |seed| {
        let (mut g, mut r) = bank_world(seed);
        for e in 61..=85 {
            g.entity_status[e] = if r.chance(Ext::from_f64(0.7)) { 1 } else { 0 };
        }
        let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
        check(0x4e3408, &[], &g, rng0, |g, ui, rng| { create_new_bank_in_free_slot(g, ui, rng); 0 }, false);
    });
}

#[test]
fn diff_create_insurer() {
    par_seeds(101..=106u64, |seed| {
        let (mut g, mut r) = bank_world(seed);
        let id = cast::<i32, _>(r.range(86, 130));
        g.entity_status[id] = 0;
        g.tmp_ext1 = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(3000.0));
        g.tmp_ext2 = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(500.0));
        g.country_idx[id] = [22, 48, 14, 47, 35, 3, 0, 23][cast::<usize, _>(r.range(0, 7))];
        for b in 11..=85 {
            g.country_idx[b] = cast::<i32, _>(r.range(0, 50));
        }
        let announce = cast::<i32, _>(seed % 3);
        let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
        let args = vec![id.to_string(), announce.to_string()];
        check(0x4f1893, &args, &g, rng0, |g, ui, rng| { create_insurer(g, ui, rng, id, announce); 0 }, false);
    });
}

#[test]
fn diff_issue_bonds() {
    let __content = wsr_core::content::current();
    par_seeds(111..=116u64, |seed| {
        let (mut g, mut r) = bank_world(seed);
        for k in 0..6 {
            let id = cast::<i32, _>(r.range(11, 1580));
            g.entity_status[id] = 1;
            g.bonds_issued[id] = if k == 5 { 100 } else { 0 };
            g.tmp_ext1 = if r.chance(Ext::from_f64(0.5)) { Ext::from_f64(0.0) } else { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5000.0)) };
            g.ent_name[id] = s(["ACME", "BIGNAME'S", "Long Company Name", "SHORTCO", "Q"][cast::<usize, _>(r.range(0, 4))]);
            let rate = if r.chance(Ext::from_f64(0.6)) { Ext::from_f64(0.0) } else { f64r(&mut r, Ext::from_f64(2.0), Ext::from_f64(16.0)) };
            let mat = if r.chance(Ext::from_f64(0.7)) { 0 } else { g.cur_year + cast::<i32, _>(r.range(1, 20)) };
            if r.chance(Ext::from_f64(0.3)) {
                g.hold_stock_id[(1, g.cur_player)] = cast::<i16, _>(id);
            }
            let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
            let args = vec![id.to_string(), format!("d:{rate}"), mat.to_string()];
            let Some(o) = check(0x5231bc, &args, &g, rng0, |g, ui, rng| issue_bonds(g, ui, rng, id, rate, mat), true) else { return };
            eprintln!("{__wsr_text_13927}{id}, {rate}, {mat}): {:?} {}", o.globals.bond_issue_info[id], o.globals.news_msg,__wsr_text_13927=__content.string(13927));
            g = o.globals;
        }
    });
}
