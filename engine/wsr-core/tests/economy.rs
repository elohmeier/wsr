//! Tests of logic::economy: unit tests and differential tests against the original (tests/diff).
//! One submodule per source submodule (tests/economy/*.rs); the shared plausible world is [`world`].
//!
//! Harness notes (from tests/market.rs):
//! * `TIME$` is fixed by the harness (10:00:00), so the port gets `time_secs = 0`.
//! * CalcEntityFinancials prices bonds with PB_Pow, whose integer-exponent path Unicorn mis-detects; use
//!   `pow_safe` on states that reach it.
//! * `TickerPauseUntil` (PauseTicker, UI timing) is ignored; message boxes are compared by count.
//! * MSCNEWS.DAT (ShowMiscNewsStory) is given to the port from extracted/app ([`init_files`]).

#![allow(clippy::field_reassign_with_default, dead_code, unused_imports)]
use wsr_core::pb::real::{Ext, cast};


mod diff;

#[path = "economy/shared.rs"]
mod shared;
#[path = "economy/macro_update.rs"]
mod macro_update;
#[path = "economy/quarterly.rs"]
mod quarterly;
#[path = "economy/crises.rs"]
mod crises;
#[path = "economy/crises2.rs"]
mod crises2;
#[path = "economy/cycle.rs"]
mod cycle;
#[path = "economy/industry.rs"]
mod industry;
#[path = "economy/commodities.rs"]
mod commodities;
#[path = "economy/world_events.rs"]
mod world_events;

use diff::{Compare, Opts, Original, Rand, run_original_with};
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::pb::numeric::Cur;
use wsr_core::state::Globals;
use wsr_core::ui::Scripted;

/// Gives the port the data files the original reads (MSCNEWS.DAT from extracted/app, if present).
pub fn init_files() {
    let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../extracted/app/Mscnews.dat");
    wsr_core::logic::economy::set_mscnews_dat(std::fs::read(p).ok());
}

pub fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

pub fn f64r(r: &mut Rand, lo: Ext, hi: Ext) -> Ext {
    lo + (hi - lo) * cast::<Ext, _>(r.range(0, 1_000_000_000)) / Ext::from_f64(1e9)
}

pub fn f32r(r: &mut Rand, lo: Ext, hi: Ext) -> f32 {
    cast::<f32, _>(f64r(r, lo, hi))
}

/// A plausible mid-game world (as tests/valuation.rs) plus commodity ranges, crypto, ticker and the
/// streaming-quote lists.
pub fn world(seed: u64) -> Globals {
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
pub fn pow_safe(g: &mut Globals, seed: u64) {
    g.cur_quarter = if seed.is_multiple_of(2) { 1 } else { 3 };
    g.sim_co_count = if seed.is_multiple_of(3) { 300 } else { 450 };
    for e in 11..g.sim_co_count {
        g.bonds_issued[e] = 0;
    }
}

/// `StockIndex` the original would compute from the current caps (before re-valuations).
pub fn index_estimate(g: &Globals) -> Ext {
    let mut sum = Ext::from_f64(0.0);
    for e in 11..=1600 {
        if g.entity_status[e] == 1 && g.industry_num[e] != 71 && g.market_cap[e] > Ext::from_f64(0.0) && g.market_cap[e] < Ext::from_f64(1e9) {
            sum += g.market_cap[e];
        }
    }
    sum / g.stock_index_divisor
}

pub fn run(addr: u32, args: &[i32], g: &Globals, rng: u32) -> Option<Original> {
    let opts = Opts {
        args: args.iter().map(|a| a.to_string()).collect(),
        rng: Some(rng),
        ..Opts::default()
    };
    run_original_with(addr, &opts, g)
}

pub fn cmp() -> Compare {
    Compare::default().rel_tol(Ext::from_f64(1e-12)).ignore("TickerPauseUntil").ignore("hReportDlg")
}

/// Log lines other than SKIP lines, message boxes and API calls.
pub fn unexpected(log: &[String]) -> Vec<&String> {
    log.iter()
        .filter(|l| !["SKIP ", "API ", "MSGBOX", "DIALOG", "CONTROL"].iter().any(|p| l.starts_with(p)))
        .collect()
}

/// Message boxes of the original: MSGBOXes and ShowQuickMessage pop-ups (qm.exe).
pub fn orig_msgs(log: &[String]) -> usize {
    log.iter().filter(|l| l.starts_with("MSGBOX") || l.contains("CreateProcessA(0x0, 'qm.exe")).count()
}

#[track_caller]
pub fn check(addr: u32, args: &[i32], g: &Globals, rng0: u32, port_fn: &dyn Fn(&mut Globals, &mut Scripted, &mut Rng)) -> Option<Globals> {
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

