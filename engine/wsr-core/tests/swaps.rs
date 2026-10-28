//! Tests of `logic::swaps`: hand-checked unit tests plus differential tests against the original
//! routines run under emulation (tools/emu/game.py, see tests/diff/mod.rs).

#![allow(clippy::field_reassign_with_default)]
use wsr_core::pb::real::{Ext, cast};


mod diff;

use diff::{Compare, Opts, Original, Rand, run_original_with, par_seeds, unavailable};
use wsr_core::logic::swaps::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::pb::numeric::Cur;
use wsr_core::state::Globals;
use wsr_core::ui::Scripted;

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

// ------------------------------------------------------------------------------------------- states

/// A plausible mid-game world: players 1..NumPlayers, banks 11..85, insurers 86..130, operating corps
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
    g.exchange_rate = cast::<f64, _>(if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { Ext::from_f64(1.0) } else { f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(2.0)) });
    g.loan_limit_multiple = cast::<f64, _>(if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { Ext::from_f64(0.0) } else { (cast::<Ext, _>(r.range(20, 35))) * Ext::from_f64(0.1) });
    g.corp_tax_rate = cast::<i32, _>(r.range(15, 45));
    g.bank_cd_rate = cast::<f32, _>(if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { Ext::from_f64(0.0) } else { Ext::from(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(9.0))) });
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
    if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
        g.wealth_tax_rate = cast::<i32, _>(r.range(1, 3));
        g.carbon_tax_rate = cast::<i32, _>(r.range(1, 5));
        g.health_care_tax_rate = cast::<i32, _>(r.range(1, 5));
        g.oil_windfall_tax_rate = cast::<i32, _>(r.range(1, 20));
        g.holding_co_tax_rate = cast::<i32, _>(r.range(1, 10));
        g.corp_shares_tax_country = cast::<i32, _>(r.range(0, 3));
    }
    for p in 1..=5 {
        g.is_computer_player[p] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { 1 } else { 0 };
        g.player_ceo_of[p] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { cast::<i32, _>(r.range(131, 1580)) } else { 0 };
        g.player_prepaid_tax[p] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(50.0)) } else { Ext::from_f64(0.0) };
    }
    for e in 1..=1600 {
        let active = if e <= 10 { e <= g.num_players } else { r.chance(cast::<f64, _>(Ext::from_f64(0.93))) };
        g.entity_status[e] = if active { 1 } else if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { 0 } else { 2 };
        g.ent_name[e] = s(&format!("{__wsr_text_13758}{e}",__wsr_text_13758=__content.string(13758)));
        g.cash[e] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(3000.0));
        g.t_bills[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(1000.0)) } else { Ext::from_f64(0.0) };
        g.bank_debt[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { f64r(&mut r, -Ext::from_f64(1.0), Ext::from_f64(2000.0)) } else { Ext::from_f64(0.0) };
        g.lending_bank[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.9))) { cast::<i16, _>(r.range(11, 50)) } else { 0 };
        g.credit_rating_num[e] = cast::<i32, _>(r.range(0, 10));
        g.credit_frozen[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) { 1 } else { 0 };
        g.long_gov_bond_face[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { r.range(1, 500) } else { 0 };
        g.short_gov_bond_face[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { r.range(1, 500) } else { 0 };
        g.lt_gov_bond_cost[e] = cast::<Ext, _>(g.long_gov_bond_face[e]) * f64r(&mut r, Ext::from_f64(0.8), Ext::from_f64(1.2));
        g.st_gov_bond_cost[e] = cast::<Ext, _>(g.short_gov_bond_face[e]) * f64r(&mut r, Ext::from_f64(0.9), Ext::from_f64(1.1));
        g.corp_controller[e] = match r.range(0, 5) {
            0 => 0,
            1 | 2 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
            _ => cast::<i32, _>(r.range(11, 1600)),
        };
        g.commod_margin_bal[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(30.0)) } else { Ext::from_f64(0.0) };
        g.auto_pilot[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { 1 } else { 0 };
        for slot in 1..=15 {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.25))) {
                g.hold_stock_id[(slot, e)] = cast::<i16, _>(r.range(11, 1600));
                g.hold_stock_pct[(slot, e)] = cast::<i16, _>(r.range(1, 100));
            }
            if !(131..=1580).contains(&e) && r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
                g.hold_bond_id[(slot, e)] = cast::<i32, _>(r.range(11, 1600));
                g.hold_bond_face[(slot, e)] = r.range(1, 300);
                g.bond_hold_cost[(slot, e)] = cast::<Ext, _>(g.hold_bond_face[(slot, e)]) * f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(1.2));
            }
        }
        if e <= 5 {
            for c in 11..=1600 {
                if r.chance(cast::<f64, _>(Ext::from_f64(0.01))) {
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
            g.tax_loss_carry[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { f64r(&mut r, -Ext::from_f64(100.0), Ext::from_f64(0.0)) } else { Ext::from_f64(0.0) };
            g.extraordinary_pl[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) { r.range(-50, 50) } else { 0 };
            g.accrued_pl[e] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(200.0));
            g.div_recvd1[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(10.0)) } else { Ext::from_f64(0.0) };
            g.div_recvd2[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(10.0)) } else { Ext::from_f64(0.0) };
            g.consol_tax_inc_ytd[e] = f64r(&mut r, -Ext::from_f64(30.0), Ext::from_f64(100.0));
            g.tax_credits[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(20.0)) } else { Ext::from_f64(0.0) };
            g.contingency_reserve[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(50.0)) } else { Ext::from_f64(0.0) };
            g.earnings_y1[e] = f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(100.0));
            g.earnings_yr2_ago[e] = f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(100.0));
            g.earnings_yr3_ago[e] = f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(100.0));
            g.earnings_yr4_ago[e] = f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(100.0));
            g.earnings_ytd[e] = f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(60.0));
            g.proj_qtr_earnings[e] = f64r(&mut r, -Ext::from_f64(10.0), Ext::from_f64(30.0));
            g.country_idx[e] = cast::<i32, _>(r.range(0, 25));
            if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
                g.bonds_issued[e] = r.range(1, 1500);
                let mat = g.cur_year + cast::<i32, _>(r.range(1, 25));
                let coupon = f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(15.0));
                g.bond_issue_info[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
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
            g.goodwill[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(300.0)) } else { Ext::from_f64(0.0) };
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
            g.subprime_secs[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { r.range(0, 3000) } else { 0 };
        }
        if (86..=130).contains(&e) {
            g.ins_reserves[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(10000.0));
        }
    }
    // a few physical commodity records ("CODE…|owner|units|cost")
    let codes = ["GOLD", "SILVER", "OIL", "WHEAT", "CORN", "BITCOIN", "ETH", "XYZ"];
    let n = cast::<i32, _>(r.range(0, 30));
    for i in 1..=n {
        let owner = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { r.range(1, cast::<i64, _>(g.num_players)) } else { r.range(131, 1600) };
        let code = codes[cast::<usize, _>(r.range(0, 7))];
        g.phys_commod_rec[i] = s(&format!(" {code}|{owner}|{}|{:.2} ", r.range(1, 5000), f64r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(900.0)))));
    }
    g.phys_commod_rec[0] = s(&n.to_string());
    g
}

/// `world` plus names/symbols, news state and a list of swap records
/// `long|short|fixed|type|start|end|notional` (some malformed, expired, future, orphaned or "S" swaps
/// outliving the short bond), `n` records in slots 1..n with a few gaps; `SwapContracts(0)` = n.
fn swap_world(seed: u64, n: i32) -> Globals {
    let mut g = world(seed);
    let mut r = Rand(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) | 1);
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    g.sel_entity = cast::<i32, _>(r.range(11, 1600));
    g.currency_symbol = s("$");
    g.million_prefix_lc = s("m");
    g.million_prefix_uc = s("M");
    g.suppress_popups = 1;
    g.prime_rate = cast::<f32, _>((cast::<f32, _>(r.range(300, 1200))) / Ext::from_f64(100.0));
    g.long_bond_yield = cast::<f32, _>((cast::<f32, _>(r.range(200, 1100))) / Ext::from_f64(100.0) + Ext::from_f64(0.003));
    g.short_bond_yield = cast::<f32, _>((cast::<f32, _>(r.range(100, 900))) / Ext::from_f64(100.0) + Ext::from_f64(0.007));
    for e in 1..=1600 {
        g.ent_symbol[e] = s(&format!("S{e}"));
        g.ent_name[e] = s(&format!("Entity Number {e}"));
        g.swap_pn_l_qtr[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { r.range(-500, 500) } else { 0 };
    }
    for i in 1..=60 {
        g.news_headline[i] = s(&format!("Old headline {i}."));
    }
    let party = |r: &mut Rand, g: &Globals| -> i32 {
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))) } else { cast::<i32, _>(r.range(11, 1600)) }
    };
    let types = ["P", "L", "S", "S", "X"];
    for i in 1..=n {
        if r.chance(cast::<f64, _>(Ext::from_f64(0.05))) {
            continue; // gap
        }
        let a = party(&mut r, &g);
        let mut b = party(&mut r, &g);
        if b == a {
            b = a % 1500 + 11;
        }
        let fixed = cast::<Ext, _>(r.range(100, 1300)) / Ext::from_f64(100.0);
        let ty = types[cast::<usize, _>(r.range(0, 4))];
        let (sy, sq) = (g.cur_year + cast::<i32, _>(r.range(-5, 1)), r.range(1, 4));
        let (ey, eq) = if r.chance(cast::<f64, _>(Ext::from_f64(0.15))) {
            (g.cur_year, cast::<i64, _>(g.cur_quarter)) // ends this quarter
        } else if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) {
            (g.cur_year - 1, r.range(1, 4))
        } else {
            (g.cur_year + cast::<i32, _>(r.range(0, 12)), r.range(1, 4))
        };
        let notional = r.range(1, 600);
        g.swap_contracts[i] = if r.chance(cast::<f64, _>(Ext::from_f64(0.04))) {
            s(&format!("{a}|{b}|{fixed}"))
        } else {
            s(&format!("{a}|{b}|{fixed}|{ty}|{sy}.{sq}|{ey}.{eq}|{notional}"))
        };
    }
    g.swap_contracts[0] = s(&n.to_string());
    g
}

/// Compare for float state: EXT is f64 in the port (80-bit in the original). TickerPauseUntil is only
/// set by PauseTicker inside ShowQuickMessage (UI, not ported).
fn cmp() -> Compare {
    Compare::default().rel_tol(Ext::from_f64(1e-12)).ignore("TickerPauseUntil")
}

fn run(addr: u32, args: &[&str], g: &Globals, rng: u32, answers: &[&str], skips: &[(u32, u32)]) -> Option<Original> {
    let opts = Opts {
        args: args.iter().map(|a| a.to_string()).collect(),
        answers,
        rng: Some(rng),
        skips: skips.to_vec(),
        ..Opts::default()
    };
    run_original_with(addr, &opts, g)
}

fn close(a: Ext, b: Ext) -> bool {
    (a - b).abs() <= Ext::from_f64(1e-9) * a.abs().max(b.abs()).max(Ext::from_f64(1.0))
}

/// Parties appearing in the swap list (plus a few uninvolved ids).
fn parties(g: &Globals, r: &mut Rand, n: usize) -> Vec<i32> {
    let mut out = Vec::new();
    while out.len() < n {
        let i = cast::<i32, _>(r.range(1, 1000));
        let rec = g.swap_contracts[i].to_display();
        if rec.is_empty() || out.len() % 4 == 3 {
            out.push(cast::<i32, _>(r.range(1, 1600)));
            continue;
        }
        let f: Vec<&str> = rec.split('|').collect();
        out.push(f[out.len() % 2].parse().unwrap());
    }
    out
}

// ------------------------------------------------------------------------------------- unit tests

#[test]
fn pnl_accumulator() {
    let mut g = Globals::default();
    assert_eq!(swap_pnl_accumulator(&mut g, 20, 1, Ext::from(Ext::from_f64(10.5))), Ext::from_f64(0.0));
    assert_eq!(g.swap_pn_l_qtr[20], 10); // half-even
    swap_pnl_accumulator(&mut g, 20, 1, Ext::from_f64(1.5));
    assert_eq!(swap_pnl_accumulator(&mut g, 20, 2, Ext::from(Ext::from_f64(0.0))), Ext::from_f64(12.0));
    swap_pnl_accumulator(&mut g, 0, 0, Ext::from_f64(0.0));
    assert_eq!(g.swap_pn_l_qtr[20], 0);
}

#[test]
fn counting_and_notional() {
    let mut g = Globals::default();
    g.swap_contracts[1] = s("20|300|5|P|2020.1|2030.1|100");
    g.swap_contracts[2] = s("300|21|5|L|2020.1|2030.1|40");
    g.swap_contracts[0] = s("2");
    assert_eq!(count_entity_swaps(&g, 300, 0), 2);
    assert_eq!(count_entity_swaps(&g, 20, 0), 1);
    // single swap on the other side than the filter → 0
    assert_eq!(count_entity_swaps(&g, 20, 2), 0);
    assert_eq!(count_entity_swaps(&g, 20, 1), 1);
    assert_eq!(get_sole_swap_counterparty(&g, 20), 300);
    assert_eq!(get_sole_swap_counterparty(&g, 300), 0);
    assert_eq!(sum_swap_notional(&g, 300, 0), Ext::from_f64(140.0));
    assert_eq!(sum_swap_notional(&g, 300, 1), -Ext::from_f64(60.0));
}

#[test]
fn est_qtr_profit_and_sort() {
    let mut g = Globals::default();
    g.prime_rate = cast::<f32, _>(Ext::from_f64(6.0));
    let t = calc_swap_est_qtr_profit(&mut g, &s("P"), Cur::from_f64(Ext::from_f64(5.0)), &s("LONG"), 100);
    assert_eq!(t, s("6.00"));
    // receive 5% fixed, pay 6%: 100 * (5 - 6) * 0.0025
    assert!((g.tmp_ext1 + Ext::from_f64(0.25)).abs() < Ext::from_f64(1e-8), "{}", g.tmp_ext1);
    g.swap_contracts[1] = s("1|2|5|P|2021.2|2030.1|1");
    g.swap_contracts[2] = s("1|2|5|P|2020.1|2031.1|1");
    g.swap_contracts[3] = s("1|2|5|P|2020.1|2030.1|1");
    sort_swap_contracts(&mut g, &mut Scripted::default());
    assert_eq!(g.swap_contracts[0], s("3"));
    assert_eq!(g.swap_contracts[1], s("1|2|5|P|2020.1|2030.1|1"));
    assert_eq!(g.swap_contracts[3], s("1|2|5|P|2021.2|2030.1|1"));
}

// ------------------------------------------------------------------------------ differential tests

#[test]
fn diff_pnl_accumulator() {
    par_seeds(1..4u64, |seed| {
        let g = swap_world(seed, 50);
        for (e, op, amt) in [(25, 1, Ext::from_f64(12.5)), (300, 1, -Ext::from_f64(7.25)), (300, 2, Ext::from_f64(0.0)), (1600, 2, Ext::from_f64(0.0)), (0, 0, Ext::from_f64(0.0)), (0, 1, Ext::from_f64(3.0))] {
            let mut port = g.clone();
            let rv = swap_pnl_accumulator(&mut port, e, op, amt);
            let Some(o) = run(0x6477eb, &[&e.to_string(), &op.to_string(), &format!("e:{amt}")], &g, 1, &[], &[]) else {
                return;
            };
            assert_eq!(o.return_float.unwrap_or(Ext::from(Ext::from_f64(0.0))), rv, "({e},{op},{amt})");
            cmp().assert(&port, &o.globals);
        }
    });
}

#[test]
fn diff_find_free_slot_and_sort() {
    for (seed, n) in [(5u64, 30), (6, 200), (7, 1000), (8, 2)] {
        let mut g = swap_world(seed, n);
        if n == 1000 {
            for i in 1..=1000 {
                if g.swap_contracts[i].is_empty() {
                    g.swap_contracts[i] = s("11|12|5|P|2000.1|2099.1|1");
                }
            }
        }
        let mut port = g.clone();
        let mut ui = Scripted::default();
        let rv = find_free_swap_slot(&mut port, &mut ui);
        let Some(o) = run(0x647639, &[], &g, 1, &["IDOK"], &[]) else { return };
        assert_eq!(cast::<i32, _>(o.return_value), rv);
        assert_eq!(ui.log.len(), o.log.iter().filter(|l| l.starts_with("MSGBOX")).count(), "{:?}", o.log);
        cmp().assert(&port, &o.globals);

        let mut port = g.clone();
        sort_swap_contracts(&mut port, &mut Scripted::default());
        let Some(o) = run(0x6479fe, &[], &g, 1, &[], &[]) else { return };
        cmp().assert(&port, &o.globals);
    }
}

#[test]
fn diff_count_sole_sum() {
    par_seeds(10..13u64, |seed| {
        let mut g = swap_world(seed, 120);
        // make some entities appear in exactly one swap
        g.swap_contracts[121] = s("1581|1582|4|L|2000.1|2099.4|77");
        g.swap_contracts[0] = s("121");
        let mut r = Rand(seed);
        let mut ids = parties(&g, &mut r, 4);
        ids.push(1581);
        for e in ids {
            for f in [0, 1, 2] {
                let Some(o) = run(0x648575, &[&e.to_string(), &f.to_string()], &g, 1, &[], &[]) else { return };
                assert_eq!(cast::<i32, _>(o.return_value), count_entity_swaps(&g, e, f), "count({e},{f})");
                cmp().assert(&g, &o.globals);
            }
            let Some(o) = run(0x6487c6, &[&e.to_string()], &g, 1, &[], &[]) else { return };
            assert_eq!(cast::<i32, _>(o.return_value), get_sole_swap_counterparty(&g, e), "sole({e})");
            for m in [0, 1] {
                let Some(o) = run(0x646745, &[&e.to_string(), &m.to_string()], &g, 1, &[], &[]) else { return };
                assert_eq!(o.return_float.unwrap(), sum_swap_notional(&g, e, m), "sum({e},{m})");
            }
        }
    });
}

#[test]
fn diff_est_qtr_profit() {
    let __content = wsr_core::content::current();
    let mut g = swap_world(20, 10);
    for (ty, fixed, pos, notional) in
        [("P", Ext::from_f64(5.25), "LONG", 100i64), ("L", Ext::from_f64(7.5), "SHORT", 333), ("S", Ext::from_f64(1.0), "LONG", 1), ("X", Ext::from_f64(4.0), "SHORT", 50), ("S", Ext::from_f64(12.3456), "SHORT", 999)]
    {
        g.tmp_ext1 = Ext::from_f64(99.0);
        let fixed = Cur::from_f64(fixed);
        let mut port = g.clone();
        let t = calc_swap_est_qtr_profit(&mut port, &s(ty), fixed, &s(pos), notional);
        let args = [format!("s:{ty}"), format!("q:{}", fixed.0), format!("s:{pos}"), format!("{__wsr_text_13664}{notional}",__wsr_text_13664=__content.string(13664))];
        let a: Vec<&str> = args.iter().map(|x| x.as_str()).collect();
        let Some(o) = run(0x645d1c, &a, &g, 1, &[], &[]) else { return };
        assert_eq!(o.return_str.unwrap_or_default(), t, "{ty} {pos}"); // "" comes back as a null BSTR
        cmp().assert(&port, &o.globals);
    }
}

#[test]
fn diff_termination_fee() {
    let __content = wsr_core::content::current();
    par_seeds(30..34u64, |seed| {
        let mut g = swap_world(seed, 60);
        g.sim_co_count = [300, 600, 800, 1200][cast::<usize, _>(seed % 4)];
        // a few swaps ending in the current quarter (formatted like FORMAT$(curDate, "####.#"))
        for i in [3, 4, 5] {
            g.swap_contracts[i] = s(&format!("{}|{}|{}|{}|2000.1|{}.{}|{}", 11 + i, 200 + i, 3 + i, ["P", "L", "S"][cast::<usize, _>(i) - 3], g.cur_year, g.cur_quarter, 100 * i));
        }
        for slot in 1..=20 {
            let rec = g.swap_contracts[slot].to_display();
            if rec.split('|').count() < 7 {
                continue;
            }
            let f: Vec<&str> = rec.split('|').collect();
            for e in [f[0].parse::<i32>().unwrap(), f[1].parse().unwrap()] {
                let mut port = g.clone();
                let fee = calc_swap_termination_fee(&mut port, e, slot);
                let Some(o) = run(0x643336, &[&e.to_string(), &slot.to_string()], &g, 1, &[], &[]) else { return };
                assert!(close(o.return_float.unwrap(), fee), "{__wsr_text_14172}{slot}{__wsr_text_13775}{e}: {} vs {fee}", o.return_float.unwrap(),__wsr_text_14172=__content.string(14172), __wsr_text_13775=__content.string(13775));
                cmp().assert(&port, &o.globals);
            }
        }
    });
}

/// UI that answers only real MSGBOXes (style <> 0): ShowQuickMessage popups (`msgbox(text, 0, "")` in
/// the port) are not MessageBoxA calls in the original and must not consume answers.
#[derive(Default)]
struct Answers {
    answers: std::collections::VecDeque<i32>,
    boxes: usize,
}

impl wsr_core::ui::Ui for Answers {
    fn msgbox(&mut self, _text: &PbStr, style: i32, _title: &PbStr) -> i32 {
        if style == 0 {
            return 1;
        }
        self.boxes += 1;
        self.answers.pop_front().unwrap_or(1)
    }
    fn inputbox(&mut self, _p: &PbStr, _t: &PbStr, default: &PbStr) -> PbStr {
        default.clone()
    }
    fn report(&mut self, _t: &PbStr, _l: &[PbStr], _y: bool) -> wsr_core::ui::ReportAction {
        wsr_core::ui::ReportAction::Ok
    }
    fn news(&mut self, _h: &PbStr) {}
}

/// BorrowToCoverPurchase (a termination fee above cash and T-bills): hooked, its calls are counted.
const BORROW_TO_COVER: u32 = 0x5246b6;

#[test]
fn diff_cancel_entity_swaps() {
    static BORROWED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    par_seeds(40..46u64, |seed| {
        let g0 = swap_world(seed, 150);
        let mut r = Rand(seed);
        for (k, e) in parties(&g0, &mut r, 4).into_iter().enumerate() {
            let mut g = g0.clone();
            if k == 1 {
                // a common-controller counterparty
                let rec = g.swap_contracts[1].to_display();
                if let Some(cp) = rec.split('|').nth(1).and_then(|x| x.parse::<i32>().ok()) {
                    g.swap_contracts[1] = s(&rec.replacen(rec.split('|').next().unwrap(), &e.to_string(), 1));
                    g.corp_controller[cp] = g.corp_controller[e];
                }
            }
            for (mode, check, answers, broke) in [
                (-1, 2, vec![], false),
                (1, 0, vec![], false),
                (0, 1, vec![], false),
                (0, 0, vec!["IDYES"; 40], false),
                (0, 0, vec!["IDYES", "IDNO"], false),
                (0, 0, vec!["IDYES"; 40], true),
                (0, 0, vec!["IDYES", "IDNO", "IDYES", "IDYES", "IDNO"], true),
            ] {
                let seed_rng = 12345 + cast::<u32, _>(seed);
                let mut g = g.clone();
                if broke {
                    // no cash: a termination fee is financed (BorrowToCoverPurchase)
                    g.cash[e] = Ext::from_f64(0.0);
                    g.t_bills[e] = Ext::from_f64(0.0);
                    g.active_entity = e;
                }
                let mut port = g.clone();
                let mut rng = Rng::from_state(seed_rng);
                let mut ui = Answers::default();
                for a in &answers {
                    ui.answers.push_back(if *a == "IDNO" { 7 } else { 6 });
                }
                let rv = cancel_entity_swaps(&mut port, &mut ui, &mut rng, e, mode, check);
                let opts = Opts {
                    args: vec![e.to_string(), mode.to_string(), check.to_string()],
                    answers: &answers,
                    rng: Some(seed_rng),
                    hooks: vec![BORROW_TO_COVER],
                    ..Opts::default()
                };
                let Some(o) = run_original_with(0x6428b0, &opts, &g) else {
                    return;
                };
                if o.log.iter().any(|l| l.starts_with("CALL BorrowToCoverPurchase(")) {
                    BORROWED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                assert_eq!(cast::<i32, _>(o.return_value), rv, "e {e} mode {mode} check {check}");
                assert_eq!(o.rng, rng.state());
                let n_box = o.log.iter().filter(|l| l.starts_with("MSGBOX")).count();
                assert_eq!(n_box, ui.boxes, "{:?}", o.log);
                cmp().assert(&port, &o.globals);
            }
        }
    });
    if unavailable().is_none() {
        assert!(BORROWED.load(std::sync::atomic::Ordering::Relaxed) > 0, "BorrowToCoverPurchase not reached");
    }
}

#[test]
fn diff_transfer_entity_swaps() {
    let __content = wsr_core::content::current();
    par_seeds(50..54u64, |seed| {
        let g = swap_world(seed, 200);
        let mut r = Rand(seed);
        let ids = parties(&g, &mut r, 6);
        for w in ids.chunks(2) {
            let (from, to) = (w[0], w[1]);
            let mut port = g.clone();
            let mut ui = Scripted::default();
            transfer_entity_swaps(&mut port, &mut ui, from, to);
            let Some(o) = run(0x6460e0, &[&format!("{__wsr_text_13741}{from}",__wsr_text_13741=__content.string(13741)), &format!("v:{to}")], &g, 1, &[], &[]) else { return };
            cmp().assert(&port, &o.globals);
        }
    });
}

#[test]
fn diff_process_swap_payments_settlement() {
    let __content = wsr_core::content::current();
    let (mut defaults, mut canceled, mut reports) = (0, 0, 0);
    for seed in 60..68u64 {
        let mut g = swap_world(seed, [20, 100, 300, 600][cast::<usize, _>(seed % 4)]);
        if seed % 2 == 0 {
            g.news_msg = s("A stale message");
        }
        if seed % 3 == 0 {
            g.suppress_popups = 0;
        }
        if seed % 2 == 1 {
            // huge notionals: payers that cannot pay (partial defaults, cancellation of weak payers)
            let n = g.swap_contracts[0].to_display().parse::<i32>().unwrap();
            for k in 1..=8 {
                let (a, b) = (131 + k * 7, 400 + k * 13);
                let ty = ["P", "L", "S", "P"][cast::<usize, _>(k) % 4];
                g.swap_contracts[n + k] = s(&format!("{a}|{b}|{}|{ty}|2000.1|{}.4|{}", if k % 2 == 0 { 1 } else { 20 }, g.cur_year + 3, 200_000 * k));
                g.credit_rating_num[a] = if k % 3 == 0 { 1 } else { 5 };
                g.credit_rating_num[b] = if k % 3 == 1 { 0 } else { 6 };
            }
            g.swap_contracts[0] = s(&(n + 8).to_string());
        }
        let seed_rng = 777 + cast::<u32, _>(seed) * 31;
        let mut port = g.clone();
        let mut rng = Rng::from_state(seed_rng);
        let mut ui = Scripted::default();
        let rv = process_swap_payments(&mut port, &mut ui, &mut rng, 0, &s("P"));
        for l in &ui.log {
            defaults += cast::<i32, _>(l.contains(__content.string(14173)));
            canceled += cast::<i32, _>(l.contains(__content.string(14071)));
            reports += cast::<i32, _>(l.contains(__content.string(14174)));
        }
        let Some(o) = run(0x63ddc0, &["0", "s:P"], &g, seed_rng, &[], &[]) else { return };
        assert_eq!(o.return_float.unwrap(), rv);
        assert_eq!(o.rng, rng.state(), "rng");
        assert!(!cmp().diffs(&g, &o.globals).is_empty());
        cmp().assert(&port, &o.globals);
    }
    // the states exercise the default, cancellation and P&L-report paths
    assert!(defaults > 0 && canceled > 0 && reports > 0, "{defaults} {canceled} {reports}");
}

#[test]
fn diff_process_swap_payments_estimate() {
    let __content = wsr_core::content::current();
    par_seeds(70..74u64, |seed| {
        let g = swap_world(seed, 150);
        let mut r = Rand(seed);
        for e in parties(&g, &mut r, 4) {
            for m in ["P", "R"] {
                let seed_rng = 99 + cast::<u32, _>(e);
                let mut port = g.clone();
                let mut rng = Rng::from_state(seed_rng);
                let mut ui = Scripted::default();
                let rv = process_swap_payments(&mut port, &mut ui, &mut rng, e, &s(m));
                let Some(o) = run(0x63ddc0, &[&e.to_string(), &format!("s:{m}")], &g, seed_rng, &[], &[]) else { return };
                assert_eq!(o.return_float.unwrap(), rv, "{__wsr_text_14175}{e} {m}",__wsr_text_14175=__content.string(14175));
                assert_eq!(o.rng, rng.state());
                cmp().assert(&port, &o.globals);
            }
        }
    });
}
