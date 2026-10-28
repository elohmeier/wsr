//! Tests of logic::market: unit tests and differential tests against the original (tests/diff).
//!
//! Harness notes:
//! * `TIME$` is fixed by the harness (10:00:00), so the port gets `time_secs = 0`.
//! * CalcEntityFinancials prices bonds with PB_Pow, whose integer-exponent path Unicorn mis-detects; the
//!   states are restricted like tests/finance.rs (`pow_safe`).
//! * `TickerPauseUntil` (PauseTicker, UI timing) is ignored; message boxes are compared by count.

#![allow(clippy::field_reassign_with_default)]
use wsr_core::pb::real::{Ext, cast};


mod diff;

#[path = "market/commodities.rs"]
mod commodities;

use diff::{Compare, Opts, Original, Rand, run_original_with};
use wsr_core::logic::market::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::pb::numeric::Cur;
use wsr_core::state::Globals;
use wsr_core::ui::Scripted;

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

fn f64r(r: &mut Rand, lo: Ext, hi: Ext) -> Ext {
    lo + (hi - lo) * cast::<Ext, _>(r.range(0, 1_000_000_000)) / Ext::from_f64(1e9)
}

fn f32r(r: &mut Rand, lo: Ext, hi: Ext) -> f32 {
    cast::<f32, _>(f64r(r, lo, hi))
}

/// A plausible mid-game world (as tests/valuation.rs) plus commodity ranges, crypto, ticker and the
/// streaming-quote lists.
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
    g.interest_rate_target = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(16.0)));
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
    g.oil_price = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(0.5), Ext::from_f64(3.0)));
    g.gold_price = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(100.0), Ext::from_f64(2500.0)));
    g.silver_price = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(2.0), Ext::from_f64(40.0)));
    g.wheat_price = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(1.0), Ext::from_f64(12.0)));
    g.corn_price = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(1.0), Ext::from_f64(9.0)));
    g.oil_target_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.2), Ext::from_f64(3.0)));
    g.gold_target_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(300.0), Ext::from_f64(2500.0)));
    g.silver_target_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(4.0), Ext::from_f64(40.0)));
    g.wheat_target_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(12.0)));
    g.corn_target_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(9.0)));
    g.gold_price_min = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2000.0), Ext::from_f64(4000.0)));
    g.gold_price_max = cast::<f32, _>(g.gold_price_min * Ext::from_f64(3.0));
    g.wheat_price_min = cast::<f32, _>(Ext::from_f64(2.0));
    g.wheat_price_max = cast::<f32, _>(Ext::from_f64(15.0));
    g.corn_price_min = cast::<f32, _>(Ext::from_f64(1.8));
    g.corn_price_max = cast::<f32, _>(Ext::from_f64(14.0));
    g.bitcoin_price = cast::<i32, _>(r.range(100, 90000));
    g.ethereum_price = cast::<i32, _>(r.range(10, 5000));
    g.btc_price_last_news = if r.chance(Ext::from_f64(0.3)) { 0 } else { cast::<i32, _>(cast::<Ext, _>(g.bitcoin_price) * f64r(&mut r, Ext::from_f64(0.9), Ext::from_f64(1.1))) };
    g.eth_price_last_news = if r.chance(Ext::from_f64(0.3)) { 0 } else { cast::<i32, _>(cast::<Ext, _>(g.ethereum_price) * f64r(&mut r, Ext::from_f64(0.9), Ext::from_f64(1.1))) };
    g.ticker_text = match r.range(0, 3) {
        0 => PbStr::new(),
        1 => s("   ACME (XYZ) 12.50"),
        2 => s("   BITCOIN 40000_+900"),
        _ => s(&"X".repeat(160)),
    };
    g.stock_index = f64r(&mut r, Ext::from_f64(500.0), Ext::from_f64(20000.0));
    g.stock_index_divisor = f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(5.0));
    g.stock_index_base = if r.chance(Ext::from_f64(0.2)) { 0 } else { 5000 };
    g.market_valuation = cast::<f32, _>([Ext::from_f64(0.0), Ext::from_f64(0.3), Ext::from_f64(0.8), Ext::from_f64(1.0), Ext::from_f64(1.2), Ext::from_f64(1.9)][cast::<usize, _>(r.range(0, 5))]);
    g.bubble_state = [0, 0, 50, 101, 103, 104, 105, 106, 203, 205][cast::<usize, _>(r.range(0, 9))];
    g.fad_industry = if r.chance(Ext::from_f64(0.5)) { cast::<i32, _>(r.range(3, 69)) } else { 0 };
    g.fad_valuation_pct = if r.chance(Ext::from_f64(0.7)) { cast::<i32, _>(r.range(50, 200)) } else { 0 };
    g.hot_stock_corp = cast::<i16, _>(r.range(131, 1580));
    g.last_fraud_corp = cast::<i32, _>(r.range(131, 1580));
    g.million_prefix_uc = s(["M", "B", "M"][cast::<usize, _>(r.range(0, 2))]);
    g.rand_seed = cast::<i32, _>(r.range(0, 32600));
    g.game_in_progress = 1;
    g.active_corp_count = cast::<i32, _>(r.range(700, 1300));
    g.cur_player = 1;
    g.app_title = s(__content.string(13661));
    for p in 1..=5 {
        g.player_name[p] = s(&format!("{__wsr_text_13556}{p}",__wsr_text_13556=__content.string(13556)));
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
            g.ent_misc_data[e] = s(&format!("{:.2}", f64r(&mut r, Ext::from(-Ext::from_f64(5.0)), Ext::from(Ext::from_f64(20.0)))));
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
            continue;
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
            if !r.chance(Ext::from_f64(0.05)) {
                v *= Ext::from_f64(1.0) - trend + f64r(&mut r, -Ext::from_f64(0.05), Ext::from_f64(0.05));
            }
        }
    }
    // streaming quotes of the players
    for p in 1..=5 {
        for slot in 1..=15 {
            if r.chance(Ext::from_f64(0.5)) {
                let c = cast::<i32, _>(r.range(1, 1600));
                g.watch_list[(slot, p)] = c;
                g.stream_cur_price[(slot, p)] = s(&format!("{:.2}", f64r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(50.0)))));
                g.stream_prev_price_str[(slot, p)] = g.stream_cur_price[(slot, p)].clone();
                g.stream_price_dir[(slot, p)] = cast::<i32, _>(r.range(-1, 1));
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

/// `StockIndex` the original would compute from the current caps (before re-valuations).
fn index_estimate(g: &Globals) -> Ext {
    let mut sum = Ext::from_f64(0.0);
    for e in 11..=1600 {
        if g.entity_status[e] == 1 && g.industry_num[e] != 71 && g.market_cap[e] > Ext::from_f64(0.0) && g.market_cap[e] < Ext::from_f64(1e9) {
            sum += g.market_cap[e];
        }
    }
    sum / g.stock_index_divisor
}

fn run(addr: u32, args: &[i32], g: &Globals, rng: u32) -> Option<Original> {
    let opts = Opts {
        args: args.iter().map(|a| a.to_string()).collect(),
        rng: Some(rng),
        ..Opts::default()
    };
    run_original_with(addr, &opts, g)
}

fn cmp() -> Compare {
    Compare::default().rel_tol(Ext::from_f64(1e-12)).ignore("TickerPauseUntil")
}

/// Log lines other than SKIP lines, message boxes and API calls.
fn unexpected(log: &[String]) -> Vec<&String> {
    log.iter().filter(|l| !l.starts_with("SKIP ") && !l.starts_with("API ") && !l.starts_with("MSGBOX")).collect()
}

/// Message boxes of the original: MSGBOXes and ShowQuickMessage pop-ups (qm.exe).
fn orig_msgs(log: &[String]) -> usize {
    log.iter().filter(|l| l.starts_with("MSGBOX") || l.contains("CreateProcessA(0x0, 'qm.exe")).count()
}

#[track_caller]
fn check(addr: u32, args: &[i32], g: &Globals, rng0: u32, port_fn: &dyn Fn(&mut Globals, &mut Scripted, &mut Rng)) -> Option<Globals> {
    let o = run(addr, args, g, rng0)?;
    let mut port = g.clone();
    let mut rng = Rng::from_state(rng0);
    let mut ui = Scripted::default();
    port_fn(&mut port, &mut ui, &mut rng);
    assert!(unexpected(&o.log).is_empty(), "{:?}", unexpected(&o.log));
    let port_msgs = ui.log.iter().filter(|l| l.starts_with("MSGBOX")).count();
    assert_eq!(port_msgs, orig_msgs(&o.log), "message boxes: port {:?}\norig {:?}", ui.log, o.log);
    cmp().assert(&port, &o.globals);
    assert_eq!(rng.state(), o.rng, "RND sequence");
    Some(o.globals)
}

// ------------------------------------------------------------------------------------- unit tests

#[test]
fn rebase_stock_index_high_and_low() {
    let mut g = Globals::default();
    g.stock_index = Ext::from_f64(95000.0);
    g.stock_index_divisor = Ext::from_f64(10.0);
    g.stock_index_base = 0;
    g.futures_recs[3] = s("STOCKS|2|100|6|2030|1234.5|90000");
    g.futures_recs[4] = s("GOLD|2|100|6|2030|1|2");
    g.price_alerts[1] = s("3009|1|2|95000.00|5|6|7");
    g.price_alerts[2] = s("17|1|2|95000.00|5|6|7");
    let mut ui = Scripted::default();
    rebase_stock_index(&mut g, &mut ui);
    assert_eq!(g.stock_index_divisor, Ext::from_f64(50.0));
    assert!((g.stock_index - Ext::from_f64(19000.0)).abs() < Ext::from_f64(1e-3));
    assert_eq!(g.stock_index_base, 1000);
    assert_eq!(g.futures_recs[3], "STOCKS|2|500|6|2030|1234.5|18000.0002682209");
    assert_eq!(g.futures_recs[4], "GOLD|2|100|6|2030|1|2");
    assert_eq!(g.price_alerts[1], "3009|1|2|19000.00|5|6|7");
    assert_eq!(g.price_alerts[2], "17|1|2|95000.00|5|6|7");
    assert_eq!(ui.log.len(), 1);

    let mut g = Globals::default();
    g.stock_index = Ext::from_f64(900.0);
    g.stock_index_divisor = Ext::from_f64(10.0);
    g.stock_index_base = 5000;
    rebase_stock_index(&mut g, &mut ui);
    assert_eq!(g.stock_index, Ext::from_f64(4500.0));
    assert_eq!(g.stock_index_base, 25000);
    assert!((g.stock_index_divisor - Ext::from_f64(2.0)).abs() < Ext::from_f64(1e-6));

    let mut g = Globals::default();
    g.stock_index = Ext::from_f64(5000.0);
    let before = g.clone();
    rebase_stock_index(&mut g, &mut ui);
    assert_eq!(g.stock_index, before.stock_index);
}

#[test]
fn rebuild_streaming_quotes_compaction() {
    let mut g = Globals::default();
    g.cur_player = 1;
    g.player_name[1] = s("Me");
    for e in [20, 21, 22] {
        g.entity_status[e] = 1;
    }
    // slots: 20, 0, 0, 21, 99 (inactive), 22
    g.watch_list[(1, 1)] = 20;
    g.watch_list[(4, 1)] = 21;
    g.watch_list[(5, 1)] = 99;
    g.watch_list[(6, 1)] = 22;
    let mut ui = Scripted::default();
    rebuild_streaming_quotes_panel(&mut g, &mut ui, 2);
    assert_eq!(g.holdings_list_plr, 1);
    let list: Vec<i32> = (1..=6).map(|k| g.watch_list[(k, 1)]).collect();
    // after clearing 99: 20, 0, 0, 21, 0, 22. Slot 2 empty -> shift once (20, 0, 21, 0, 22), the next
    // slot is 21; slot 4 empty -> shift (20, 0, 21, 22): the gap in slot 2 stays.
    assert_eq!(list, vec![20, 0, 21, 22, 0, 0]);
    assert_eq!(g.watch_list[(0, 1)], 4);
}

#[test]
fn update_controller_label_state() {
    let mut g = Globals::default();
    g.cur_player = 2;
    g.ent_name[2] = s("Two");
    g.sel_entity = 0;
    g.active_entity = 0;
    g.corp_controller[2] = 7;
    update_controller_label(&mut g);
    assert_eq!(g.sel_entity, 2);
    assert_eq!(g.voting_controller, 7);
    assert_eq!(g.active_entity, 2);
    assert_eq!(g.cur_player_name, "Two");
    assert_eq!(g.sel_corp_name, "Two");
}

// ------------------------------------------------------------------------------- differential tests

/// UpdateMarketDisplay on varied states: selections (none / player / company / defunct company with an
/// active company to fall back to), game over, first-quarter calibration, index news, rebase,
/// popups, ticker and crypto quotes.
#[test]
fn diff_update_market_display() {
    let __content = wsr_core::content::current();
    let mut news = 0;
    for seed in 1..=28u64 {
        let mut g = world(seed);
        pow_safe(&mut g, seed);
        let mut r = Rand(seed * 31 + 7);
        let pick = |g: &Globals, r: &mut Rand| loop {
            let e = cast::<i32, _>(r.range(11, 1580));
            if g.entity_status[e] == 1 {
                break e;
            }
        };
        match seed % 4 {
            0 => g.sel_entity = 0,
            1 => g.sel_entity = g.cur_player,
            2 => g.sel_entity = pick(&g, &mut r),
            _ => {
                let dead = (11..1600).find(|&e| g.entity_status[e] != 1).unwrap();
                g.sel_entity = dead;
                g.active_entity = if seed % 8 == 3 { pick(&g, &mut r) } else { 0 };
                g.active_entity_name = s("Active");
                g.auto_add_stream_list = 1;
            }
        }
        if seed % 4 == 2 {
            g.active_entity = if r.chance(Ext::from_f64(0.5)) { g.sel_entity } else { g.cur_player };
        }
        g.sel_corp_name = if r.chance(Ext::from_f64(0.5)) { PbStr::new() } else { s("Sel") };
        g.cur_player_name = s(__content.string(13759));
        g.sel_industry = if r.chance(Ext::from_f64(0.5)) { 0 } else { 5 };
        g.suppress_popups = if seed % 5 == 0 { 1 } else { 0 };
        g.skip_index_move_news = if seed % 7 == 0 { 1 } else { 0 };
        if seed == 5 {
            g.game_end_flag = s("Y");
        }
        if seed == 6 {
            // first update of a new game: divisor calibration
            g.stock_index_divisor = Ext::from_f64(0.0);
            g.sim_co_count = 6;
            g.cur_quarter = 1;
            g.start_year = g.cur_year;
        }
        // index news: a previous level 5..40 % away from the new one (index scaled to ~20,000)
        if seed != 6 {
            g.stock_index_divisor = index_estimate(&g) * g.stock_index_divisor / Ext::from_f64(20000.0);
        }
        let est = index_estimate(&g);
        let factor = [Ext::from_f64(1.0), Ext::from_f64(1.08), Ext::from_f64(0.9), Ext::from_f64(1.3), Ext::from_f64(0.7), Ext::from_f64(1.02), Ext::from_f64(0.8)][cast::<usize, _>(seed % 7)];
        g.stock_index = est * factor;
        if seed == 9 {
            g.stock_index = est * Ext::from_f64(0.97); // within 8 %: the RND decides
        }
        if seed == 10 {
            g.stock_index_divisor = est * g.stock_index_divisor / Ext::from_f64(95000.0); // rebase above 90,000
            g.stock_index = Ext::from_f64(92000.0);
            g.futures_recs[1] = s("STOCKS|1|100|6|2031|1500.25|91000.5");
            g.price_alerts[1] = s("3009|1|0|91500.00|1|0|x");
        }
        if seed == 11 {
            g.stock_index_divisor = est * g.stock_index_divisor / Ext::from_f64(900.0); // rebase below 1,000
            g.stock_index = Ext::from_f64(950.0);
        }
        if seed == 12 {
            g.stock_index = Ext::from_f64(200_000.0); // previous level out of range
        }
        if seed == 13 {
            g.stock_index_divisor = est * g.stock_index_divisor / Ext::from_f64(200_000.0); // implausible sum: drift
        }
        if seed % 3 == 0 {
            // a repeated headline: trend phrases (the RandSeed MOD 4 variant for the expected direction)
            let k = cast::<usize, _>(g.rand_seed % 4);
            g.last_market_headline = if factor > Ext::from_f64(1.0) {
                s([__content.string(6597), __content.string(6598), __content.string(6599), __content.string(6600)][k])
            } else {
                s([__content.string(6590), __content.string(6591), __content.string(6592), __content.string(6593)][k])
            };
            if seed % 2 == 0 {
                // steady trends of the index series 0 over 6 months
                let cur = cast::<i32, _>(g.cap_hist_close[(0, 0)]);
                for k in 0..=6 {
                    let mut slot = cur - k;
                    if slot < 1 {
                        slot += 60;
                    }
                    let step = if factor > Ext::from_f64(1.0) { Ext::from_f64(1.0) + cast::<Ext, _>(k) } else { Ext::from_f64(10.0) - cast::<Ext, _>(k) };
                    g.cap_hist_close[(slot, 0)] = Ext::from_f64(1000.0) * step;
                }
            }
        }
        let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
        let Some(o) = check(0x4400d7, &[], &g, rng0, &|p, ui, rng| update_market_display(p, ui, rng, 0)) else {
            return;
        };
        if o.news_headline[60] != g.news_headline[60] {
            news += 1;
        }
        eprintln!(
            "seed {seed}: index {:.2} -> {:.2} (prev {:.2}) news {:?}{__wsr_text_13910}{:?}",
            g.stock_index,
            o.stock_index,
            o.prev_market_index,
            o.news_headline[60],
            o.ticker_text
        ,__wsr_text_13910=__content.string(13910));
    }
    eprintln!("{news} runs posted news");
}

/// Two consecutive refreshes (ticker and crypto state carried over).
#[test]
fn diff_update_market_display_chained() {
    let mut g = world(40);
    pow_safe(&mut g, 40);
    g.sel_entity = 0;
    let mut r = Rand(40);
    for _ in 0..3 {
        let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
        let Some(o) = check(0x4400d7, &[], &g, rng0, &|p, ui, rng| update_market_display(p, ui, rng, 0)) else {
            return;
        };
        g = o;
    }
}

#[test]
fn diff_rebase_stock_index() {
    for seed in 50..=53u64 {
        let mut g = world(seed);
        g.stock_index = [Ext::from_f64(95000.0), Ext::from_f64(500.0), Ext::from_f64(1099.9), Ext::from_f64(0.0)][cast::<usize, _>(seed - 50)];
        g.stock_index_base = if seed == 51 { 0 } else { 4321 };
        g.futures_recs[2] = s("STOCKS|3|-250|9|2030|-12.5|1234.567");
        g.futures_recs[7] = s("OIL|3|250|9|2030|12.5|1.23");
        g.futures_recs[9] = s("STOCKS|1|7|12|2029|100|99999.99");
        g.price_alerts[4] = s("3009|2|1|1000.50|a|b|c");
        g.price_alerts[5] = s("3010|2|1|1000.50|a|b|c");
        check(0x61a2fc, &[], &g, 99, &|p, ui, _| rebase_stock_index(p, ui));
    }
}

#[test]
fn diff_select_target_corp() {
    for seed in 60..=67u64 {
        let mut g = world(seed);
        pow_safe(&mut g, seed);
        let mut r = Rand(seed);
        let corp = loop {
            let e = cast::<i32, _>(r.range(11, 1580));
            if g.entity_status[e] == 1 {
                break e;
            }
        };
        g.target_corp = match seed % 4 {
            0 => corp,
            1 => g.cur_player,
            2 => 0,
            _ => corp,
        };
        g.sel_entity = if seed % 2 == 0 { cast::<i32, _>(r.range(11, 1600)) } else { g.cur_player };
        g.auto_add_stream_list = if seed % 3 == 0 { 0 } else { 1 };
        g.last_auto_added_quote = if seed == 63 { corp } else { 0 };
        if seed == 65 {
            for slot in 1..=15 {
                g.watch_list[(slot, 1)] = corp + 1; // full list
            }
        }
        if seed == 66 {
            g.watch_list[(4, 1)] = corp; // already listed
        }
        let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
        check(0x4227d5, &[], &g, rng0, &|p, ui, rng| select_target_corp(p, ui, rng, 0));
    }
}

#[test]
fn diff_rebuild_streaming_quotes_panel() {
    for seed in 70..=75u64 {
        let mut g = world(seed);
        g.cur_player = [1, 2, 1, 1, 3, 1][cast::<usize, _>(seed - 70)];
        g.holdings_list_plr = if seed == 72 { g.cur_player } else { 0 };
        if seed == 73 {
            g.is_computer_player[1] = 1;
        }
        let mode = if seed == 72 { 1 } else { 2 };
        let args = [mode];
        let o = run_original_with(
            0x5dcee5,
            &Opts { args: vec![format!("v:{mode}")], rng: Some(5), ..Opts::default() },
            &g,
        );
        let Some(o) = o else { return };
        let mut port = g.clone();
        let mut ui = Scripted::default();
        rebuild_streaming_quotes_panel(&mut port, &mut ui, args[0]);
        assert!(unexpected(&o.log).is_empty(), "{:?}", unexpected(&o.log));
        cmp().assert(&port, &o.globals);
    }
}

#[test]
fn quote_rebuild_pauses_at_the_host_clock_even_before_an_early_return() {
    struct Clock;
    impl wsr_core::ui::Ui for Clock {
        fn msgbox(&mut self, _: &PbStr, _: i32, _: &PbStr) -> i32 { 1 }
        fn inputbox(&mut self, _: &PbStr, _: &PbStr, default: &PbStr) -> PbStr { default.clone() }
        fn report(&mut self, _: &PbStr, _: &[PbStr], _: bool) -> wsr_core::ui::ReportAction { wsr_core::ui::ReportAction::Ok }
        fn news(&mut self, _: &PbStr) {}
        fn local_time(&self) -> wsr_core::logic::newgame::LocalTime {
            wsr_core::logic::newgame::LocalTime {
                hour:10, minute:2, second:18, millis:0,
                ..wsr_core::logic::newgame::LocalTime::HARNESS
            }
        }
    }
    let mut g=Globals::default();
    g.cur_player=0;
    rebuild_streaming_quotes_panel(&mut g,&mut Clock,0);
    assert_eq!(g.ticker_pause_until,36138);
}
