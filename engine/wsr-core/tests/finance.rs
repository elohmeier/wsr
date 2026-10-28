//! Tests of `logic::finance`: hand-checked unit tests plus differential tests against the original
//! routines run under emulation (tools/emu/game.py, see tests/diff/mod.rs).

#![allow(clippy::field_reassign_with_default)]
use wsr_core::pb::real::{Ext, cast};


mod diff;

use diff::{Compare, Opts, Rand, run_original_with};
use wsr_core::logic::finance::*;
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

/// Restricts a world to states where every bond-pricing exponent (number of half-year periods) is an
/// integer. HARNESS LIMITATION: PB_Pow (0x6bcabc) chooses its integer-exponent path from the x87
/// precision flag after `frndint`, which Unicorn does not set, so under emulation every exponent is
/// rounded to an integer (half-even). The port follows the real CPU; differential states avoid the case.
///
/// Gov bonds: periods = 2 * ((maturity - year) - (quarter - 1) / 4 [- 0.125 if SimCoCount > 760]);
/// corp bonds additionally - 0.25 years for corp < SimCoCount and + 0.083 for SimCoCount > 500. So:
/// quarter 1 or 3, SimCoCount <= 500, and no bonds issued by corps below SimCoCount.
fn pow_safe(g: &mut Globals, seed: u64) {
    g.cur_quarter = if seed.is_multiple_of(2) { 1 } else { 3 };
    g.sim_co_count = if seed.is_multiple_of(3) { 300 } else { 450 };
    for e in 11..g.sim_co_count {
        g.bonds_issued[e] = 0;
    }
}

/// Runs the original routine at `addr` with harness arg specs `args` and RND state `rng`; `None` if the
/// harness is unavailable. Returns (globals, rng state after, UI/API log, EAX).
fn original_spec(addr: u32, args: &[String], g: &Globals, rng: u32) -> Option<(Globals, u32, Vec<String>, u32)> {
    let opts = Opts { args: args.to_vec(), rng: Some(rng), ..Opts::default() };
    let o = run_original_with(addr, &opts, g)?;
    Some((o.globals, o.rng, o.log, o.return_value))
}

/// Like [`original_spec`] with BYREF LONG args.
fn original(addr: u32, args: &[i32], g: &Globals, rng: u32) -> Option<(Globals, u32, Vec<String>, u32)> {
    let specs: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    original_spec(addr, &specs, g, rng)
}

/// Compare for float state: EXT is f64 in the port (80-bit in the original).
fn cmp() -> Compare {
    Compare::default().rel_tol(Ext::from_f64(1e-12))
}

/// Random entity ids of the kinds a routine is called for: players, banks, insurers, corps, funds.
fn some_entities(g: &Globals, r: &mut Rand, n: usize) -> Vec<i32> {
    let mut out = Vec::new();
    while out.len() < n {
        let e = match out.len() % 5 {
            0 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
            1 => cast::<i32, _>(r.range(11, 85)),
            2 => cast::<i32, _>(r.range(86, 130)),
            3 => cast::<i32, _>(r.range(131, 1580)),
            _ => cast::<i32, _>(r.range(1581, 1583)),
        };
        if g.entity_status[e] == 1 {
            out.push(e);
        }
    }
    out
}

// ------------------------------------------------------------------------------------- unit tests

#[test]
fn industry_category() {
    assert_eq!(get_industry_category(3), 1);
    assert_eq!(get_industry_category(9), 1);
    assert_eq!(get_industry_category(69), 1);
    assert_eq!(get_industry_category(1), 2);
    assert_eq!(get_industry_category(48), 2);
    assert_eq!(get_industry_category(66), 2);
    assert_eq!(get_industry_category(70), 0);
    assert_eq!(get_industry_category(71), 0);
    assert_eq!(get_industry_category(0), 0);
}

#[test]
fn controller_type_and_managing_player() {
    let mut g = Globals::default();
    g.is_computer_player[2] = 1;
    g.corp_controller[200] = 1;
    g.corp_controller[201] = 2;
    g.corp_controller[202] = 300;
    assert_eq!(get_controller_type(&mut g, 200), 1);
    assert_eq!(get_controller_type(&mut g, 201), 2);
    assert_eq!(get_controller_type(&mut g, 202), 0);
    // players control themselves
    assert_eq!(get_controller_type(&mut g, 3), 1);
    assert_eq!(g.corp_controller[3], 3);
    // human-controlled corp on autopilot: no manager unless the controller is its CEO
    g.auto_pilot[200] = 1;
    assert_eq!(get_managing_player(&mut g, 200), 0);
    assert_eq!(g.auto_pilot[200], 1);
    g.player_ceo_of[1] = 200;
    assert_eq!(get_managing_player(&mut g, 200), 1);
    assert_eq!(g.auto_pilot[200], 0);
    // computer-controlled corp: autopilot cleared, no manager
    g.auto_pilot[201] = 1;
    assert_eq!(get_managing_player(&mut g, 201), 0);
    assert_eq!(g.auto_pilot[201], 0);
    assert_eq!(get_managing_player(&mut g, 5), 0);
}

#[test]
fn bank_cd_rate() {
    let mut g = Globals::default();
    g.credit_rating_num[20] = 9;
    assert_eq!(get_bank_cd_rate(&mut g, 20), Ext::from_f64(5.0) * (cast::<Ext, _>(0.98f32)));
    assert_eq!(g.bank_cd_rate, Ext::from_f64(5.0));
    g.bank_cd_rate = cast::<f32, _>(Ext::from_f64(4.0));
    g.credit_rating_num[21] = 3;
    assert_eq!(get_bank_cd_rate(&mut g, 21), Ext::from_f64(4.0) * (cast::<Ext, _>(1.05f32)));
    g.credit_rating_num[22] = 1;
    assert_eq!(get_bank_cd_rate(&mut g, 22), Ext::from_f64(4.0) * (cast::<Ext, _>(1.1f32)));
    g.credit_rating_num[23] = 6;
    assert_eq!(get_bank_cd_rate(&mut g, 23), Ext::from_f64(4.0));
    // non-banks use rating slot 0 (out of range of CreditRatingNum(1 TO 1600): dummy 0 -> * 1.1)
    assert_eq!(get_bank_cd_rate(&mut g, 200), Ext::from_f64(4.0) * (cast::<Ext, _>(1.1f32)));
}

#[test]
fn market_interest_rates() {
    let mut g = Globals::default();
    let mut rng = Rng::default();
    g.long_bond_yield = cast::<f32, _>(Ext::from_f64(6.0));
    g.cur_quarter = 2;
    g.base_interest_rate = cast::<f32, _>(Ext::from_f64(5.0));
    let r2 = get_market_interest_rate(&mut g, &mut rng, 2);
    assert_eq!(r2, (cast::<Ext, _>(1.2f32) + Ext::from_f64(6.0)) - cast::<Ext, _>(0.03f32) * Ext::from_f64(2.0));
    // consumer: 5 * 0.3 + 10 = 11.5 >= 5 + 3, no RND
    let st = rng.state();
    assert_eq!(get_market_interest_rate(&mut g, &mut rng, 1), Ext::from_f64(5.0) * (cast::<Ext, _>(0.3f32)) + Ext::from_f64(10.0));
    assert_eq!(rng.state(), st);
    // subprime: 3 + 6 + 0.75 = 9.75 (> 5 + 2), crisis +4.3
    g.subprime_crisis_state = 1;
    let r3 = get_market_interest_rate(&mut g, &mut rng, 3);
    assert!((r3 - (Ext::from_f64(9.75) + Ext::from_f64(4.3))).abs() < Ext::from_f64(1e-6), "{r3}");
    assert_eq!(get_market_interest_rate(&mut g, &mut rng, 4), Ext::from_f64(0.0));
}

#[test]
fn phys_commodities() {
    let mut g = Globals::default();
    g.exchange_rate = cast::<f64, _>(Ext::from_f64(2.0));
    g.gold_price = cast::<f32, _>(Ext::from_f64(1000.0));
    g.oil_price = cast::<f32, _>(Ext::from_f64(1.5));
    g.entity_status[300] = 1;
    g.phys_commod_rec[0] = s("3");
    g.phys_commod_rec[1] = s(" GOLD|300|500|420.5");
    g.phys_commod_rec[2] = s("OIL|300|10|12");
    g.phys_commod_rec[3] = s("GOLD|301|7|1");
    g.entity_status[301] = 1;
    // all: 500 oz * 0.001 * 1000 + 10 * 1.5 = 515, times ExchangeRate
    let v = get_phys_commod_value(&mut g, 300, &PbStr::new());
    assert!((v - Ext::from_f64(1030.0)).abs() < Ext::from_f64(1e-3), "{v}"); // 0.001 is a SINGLE constant
    assert!((g.ci_phys_commod_cost - Ext::from_f64(432.5)).abs() < Ext::from_f64(1e-12));
    let v = get_phys_commod_value(&mut g, 300, &s("  OILY"));
    assert!((v - Ext::from_f64(30.0)).abs() < Ext::from_f64(1e-9), "{v}");
    assert_eq!(g.ci_phys_commod_cost, Ext::from_f64(12.0));
    assert_eq!(find_phys_commod_rec(&g, 300, &s("OIL")), 2);
    assert_eq!(find_phys_commod_rec(&g, 301, &s("GOLD")), 3);
    assert_eq!(find_phys_commod_rec(&g, 301, &s("OIL")), 0);
}

#[test]
fn capital_taxes_carbon_and_threshold() {
    let mut g = Globals::default();
    g.exchange_rate = cast::<f64, _>(Ext::from_f64(1.0));
    g.carbon_tax_rate = 4;
    g.entity_status[500] = 1;
    g.industry_num[500] = 7;
    g.country_idx[500] = 3;
    g.business_assets[500] = Ext::from_f64(2000.0);
    // 4 * 2000 * 0.0025 = 20 (> 10)
    let t = calc_capital_taxes(&mut g, 500);
    assert!((t - Ext::from_f64(20.0)).abs() < Ext::from_f64(1e-6), "{t}");
    assert_eq!(g.capital_tax_count, 1);
    // below ExchangeRate * 10: nothing owed, components cleared (count stays)
    g.business_assets[500] = Ext::from_f64(800.0);
    assert_eq!(calc_capital_taxes(&mut g, 500), Ext::from_f64(0.0));
    assert_eq!(g.carbon_tax_qtr, Ext::from_f64(0.0));
    assert_eq!(g.capital_tax_count, 1);
    // exempt countries 8 and 21
    g.business_assets[500] = Ext::from_f64(2000.0);
    g.country_idx[500] = 8;
    assert_eq!(calc_capital_taxes(&mut g, 500), Ext::from_f64(0.0));
}

#[test]
fn credit_rating_corp_and_bank() {
    let mut g = Globals::default();
    g.entity_status[300] = 1;
    g.industry_num[300] = 20;
    g.tmp_credit_net_worth = Ext::from_f64(100.0);
    g.tmp_total_debt = Ext::from_f64(60.0);
    g.total_assets = Ext::from_f64(160.0);
    g.prime_rate = cast::<f32, _>(Ext::from_f64(6.0));
    g.gdp_growth = cast::<f32, _>(Ext::from_f64(2.0));
    g.difficulty_level = 2;
    for e in [g.earnings_y1.as_mut_slice(), g.earnings_yr2_ago.as_mut_slice()] {
        e.fill(Ext::from_f64(1.0));
    }
    calc_credit_rating(&mut g, 300);
    assert_eq!(g.credit_rating_str, s("BBB"));
    assert_eq!(g.credit_rating_num[300], 7);
    // ((1.5 + 6) + 0 * 0.2) + 2 * 0.15 = 7.8 -> 31.2 quarters -> 31 -> 7.75
    assert_eq!(g.borrow_rate, Ext::from_f64(7.75));
    // negative credit net worth: D, and 0 when assets < 75% of debt
    g.tmp_credit_net_worth = -Ext::from_f64(1.0);
    g.total_assets = Ext::from_f64(40.0);
    calc_credit_rating(&mut g, 300);
    assert_eq!(g.credit_rating_str, s("D"));
    assert_eq!(g.credit_rating_num[300], 0);
    assert_eq!(g.borrow_rate, Ext::from_f64(11.0));
    // AAA needs four profitable years
    g.tmp_credit_net_worth = Ext::from_f64(100.0);
    g.tmp_total_debt = Ext::from_f64(1.0);
    calc_credit_rating(&mut g, 300);
    assert_eq!(g.credit_rating_num[300], 9);
    // bank: capital ratio 100 * (NW + reserve) / liabilities
    g.entity_status[20] = 1;
    g.industry_num[20] = 1;
    g.sofr_rate = cast::<f32, _>(Ext::from_f64(3.0));
    g.net_worth = Ext::from_f64(90.0);
    g.bad_debt_reserve[20] = Ext::from_f64(10.0);
    g.bank_demand_deposits[20] = Ext::from_f64(900.0);
    g.bank_c_ds[20] = Ext::from_f64(50.0);
    g.bank_cust_deposits[20] = Ext::from_f64(50.0);
    g.tmp_total_debt = Ext::from_f64(0.0);
    g.total_assets = Ext::from_f64(1100.0);
    g.long_options_value = Ext::from_f64(0.0);
    calc_credit_rating(&mut g, 20);
    assert_eq!(g.credit_rating_str, s("A")); // 10.0 is not > 10
    assert_eq!(g.credit_rating_num[20], 8);
    assert_eq!(g.entity_borrow_rate, cast::<f32, _>(cast::<Ext, _>(0.6f32) * Ext::from_f64(1.5) + Ext::from_f64(3.0)));
}

#[test]
fn gov_bonds_at_par_when_coupon_equals_yield() {
    let mut g = Globals::default();
    g.cur_year = 2020;
    g.cur_quarter = 1;
    g.sim_co_count = 300;
    g.base_interest_rate = cast::<f32, _>(Ext::from_f64(5.0));
    g.gdp_growth = cast::<f32, _>(Ext::from_f64(0.0));
    g.monetary_policy = 1;
    // 30 years: LongBondYield = 5 * 0.9 * (0.01 * 100) = 4.5 (as SINGLE)
    g.long_bond_desc = s("2050 LONG  4.50");
    g.short_bond_desc = s("2025 SHORT 3.00");
    calc_gov_bond_yields(&mut g);
    assert!((cast::<Ext, _>(g.long_bond_yield) - Ext::from_f64(4.5)).abs() < Ext::from_f64(1e-5));
    assert!((cast::<Ext, _>(g.long_bond_price) - Ext::from_f64(100.0)).abs() < Ext::from_f64(0.001), "{}", g.long_bond_price);
    // the price is a CURRENCY value stored as SINGLE
    let c = Cur::from_f64(cast::<Ext, _>(g.long_bond_price));
    assert_eq!(cast::<f32, _>(c.get()), g.long_bond_price);
    // 5 years: short yield = 4.5 * 0.88 * 0.82 (base 5.0 in [4.3, 5.1) -> 0.52?) -> base 5.0 < 5.1: 0.52
    let sby = cast::<f32, _>(cast::<Ext, _>(cast::<f32, _>(cast::<Ext, _>(4.5f32) * cast::<Ext, _>(0.88f32))) * cast::<Ext, _>(0.52f32));
    assert_eq!(g.short_bond_yield, sby);
    assert!(g.short_bond_price > Ext::from_f64(100.0));
    assert_eq!(g.t_bill_rate, cast::<f32, _>(cast::<Ext, _>(cast::<f32, _>((Ext::from_f64(97.0) - Ext::from_f64(5.0) * Ext::from_f64(2.0)) * cast::<Ext, _>(0.01f32))) * cast::<Ext, _>(sby)));
}

#[test]
fn corp_bond_matured_at_par_and_holders() {
    let mut g = Globals::default();
    let mut rng = Rng::default();
    g.world_generated = 1;
    g.cur_year = 2020;
    g.cur_quarter = 1;
    g.sim_co_count = 300;
    g.entity_status[400] = 1;
    g.industry_num[400] = 20;
    g.credit_rating_num[400] = 6;
    g.bonds_issued[400] = 100;
    g.bond_issue_info[400] = s("2020|0|| 7.25");
    g.hold_bond_id[(1, 20)] = 400;
    g.hold_bond_face[(1, 20)] = 30;
    g.hold_bond_id[(2, 1581)] = 400;
    g.hold_bond_face[(2, 1581)] = 45;
    g.hold_bond_id[(3, 700)] = 400; // operating corps' bond slots are not scanned
    g.hold_bond_face[(3, 700)] = 99;
    let st = rng.state();
    calc_corp_bond_price(&mut g, &mut rng, 400);
    assert_ne!(rng.state(), st, "RND is drawn first");
    assert_eq!(g.bond_price[400].get(), Ext::from_f64(100.0));
    assert_eq!(g.bond_ytm, Ext::from_f64(7.25));
    assert_eq!(g.bonds_public_held, 25);
    assert_eq!(g.largest_bondholder, 1581);
    // no bonds: info and price cleared
    g.bonds_issued[400] = 0;
    calc_corp_bond_price(&mut g, &mut rng, 400);
    assert_eq!(g.bond_issue_info[400], PbStr::new());
    assert_eq!(g.bond_price[400], Cur(0));
}

#[test]
fn convertible_value_passthrough_and_default() {
    let mut g = Globals::default();
    let mut rng = Rng::default();
    g.bond_issue_info[400] = s("2030|0|| 7.25");
    let p = Cur::from_f64(Ext::from_f64(87.5));
    assert_eq!(calc_convertible_bond_value(&mut g, &mut rng, 400, p), p);
    // defaulted issuer whose conversion value (here 1000 / (500 / 10) * (2 / 10) * 0.1 = 2) is below
    // the price: 1.05 * price
    g.bond_issue_info[400] = s("2030|500|cv| 7.25");
    g.shares_outstanding[400] = Ext::from_f64(10.0);
    g.market_cap[400] = Ext::from_f64(2.0);
    g.credit_rating_num[400] = 1;
    assert_eq!(calc_convertible_bond_value(&mut g, &mut rng, 400, p), Cur::from_f64(cast::<Ext, _>(1.05f32) * Ext::from_f64(87.5)));
}

#[test]
fn entity_financials_of_a_player() {
    let mut g = Globals::default();
    let mut rng = Rng::default();
    g.num_players = 2;
    g.cur_year = 2020;
    g.cur_quarter = 1;
    g.sim_co_count = 300;
    g.base_interest_rate = cast::<f32, _>(Ext::from_f64(5.0));
    g.prime_rate = cast::<f32, _>(Ext::from_f64(6.0));
    g.exchange_rate = cast::<f64, _>(Ext::from_f64(1.0));
    g.loan_limit_multiple = cast::<f64, _>(Ext::from_f64(3.0));
    g.difficulty_level = 2;
    g.long_bond_desc = s("2050 LONG  4.50");
    g.short_bond_desc = s("2025 SHORT 3.00");
    g.entity_status[1] = 1;
    g.cash[1] = Ext::from_f64(100.0);
    g.t_bills[1] = Ext::from_f64(50.0);
    g.bank_debt[1] = Ext::from_f64(30.0);
    g.lending_bank[1] = 12;
    g.ent_name[12] = s("First Bank");
    g.entity_status[12] = 1;
    g.industry_num[12] = 1;
    g.entity_status[300] = 1;
    g.market_cap[300] = Ext::from_f64(2.0);
    g.hold_stock_id[(1, 1)] = 300;
    g.hold_stock_pct[(1, 1)] = 10;
    g.player_advances[(1, 300)] = Ext::from_f64(5.0);
    g.player_advances[(1, 301)] = Ext::from_f64(0.0005);
    calc_entity_financials(&mut g, &mut Scripted::default(), &mut rng, 1);
    assert_eq!(g.lender_id, 12);
    assert_eq!(g.lender_bank_name, s("First Bank"));
    assert_eq!(g.tmp_subsid_stock_value, Ext::from_f64(20.0));
    assert_eq!(g.tmp_player_advances, Ext::from_f64(5.0));
    assert_eq!(g.player_advances[(1, 301)], Ext::from_f64(0.0));
    // 50 + 100 + 20 + 5 = 175 assets, 30 debt
    assert_eq!(g.total_assets, Ext::from_f64(175.0));
    assert_eq!(g.tmp_total_debt, Ext::from_f64(30.0));
    assert_eq!(g.net_worth, Ext::from_f64(145.0));
    assert_eq!(g.tmp_credit_net_worth, Ext::from_f64(145.0));
    // player not controlling the lending bank: creditMult stays 1 -> limit 145 - 30 debt; the bank's
    // lending limit (at least ExchangeRate * 10000) does not bind
    assert_eq!(g.unused_credit_line, Ext::from_f64(115.0));
    assert_eq!(g.liquid_funds, Ext::from_f64(150.0) + Ext::from_f64(115.0));
    assert_eq!(g.credit_rating_str, s("A")); // 30 / 145 = 0.207 > 0.2
    assert_eq!(g.credit_rating_num[1], 8);
    assert_eq!(g.bank_largest_debtor[12], 0);

    // a corp with LoanLimitMultiple still 0: UpdateLoanLimitMultiple sets it from the prime rate
    // (6 < 6.75: adj = 1.7 * (7 - 6) = 1.7 -> ROUND(26.7) = 27 -> 2.7) and shows the pending NewsMsg
    g.entity_status[400] = 1;
    g.industry_num[400] = 20;
    g.lending_bank[400] = 12;
    g.cash[400] = Ext::from_f64(50.0);
    g.business_assets[400] = Ext::from_f64(500.0);
    g.loan_limit_multiple = cast::<f64, _>(Ext::from_f64(0.0));
    g.news_msg = s("Rates fall");
    let mut ui = Scripted::default();
    calc_entity_financials(&mut g, &mut ui, &mut rng, 400);
    assert_eq!(g.loan_limit_multiple, Ext::from_f64(27.0) * (cast::<Ext, _>(0.1f32)));
    assert_eq!(ui.log, vec!["MSGBOX [] Rates fall".to_string()]);
    // corp: creditMult = 2.7 - 0.5 = 2.2 (SINGLE) on 550 net worth
    assert_eq!(g.net_worth, Ext::from_f64(550.0));
    assert_eq!(g.unused_credit_line, Ext::from_f64(550.0) * cast::<Ext, _>(cast::<f32, _>(g.loan_limit_multiple - Ext::from_f64(0.5))));
}

// ------------------------------------------------------------------------------ differential tests

#[test]
fn diff_calc_gov_bond_yields() {
    for seed in 1..=8u64 {
        let mut g = world(seed);
        pow_safe(&mut g, seed);
        let Some((orig, _, log, _)) = original(0x428477, &[], &g, 1) else { return };
        let mut port = g.clone();
        calc_gov_bond_yields(&mut port);
        assert!(log.is_empty(), "{log:?}");
        cmp().assert(&port, &orig);
    }
}

#[test]
fn diff_get_controller_type_and_managing_player() {
    let g = world(31);
    let mut r = Rand(31);
    for k in 0..6 {
        let e = if k < 2 { cast::<i32, _>(r.range(1, 10)) } else { cast::<i32, _>(r.range(11, 1600)) };
        for (addr, f) in [(0x4a09b9u32, get_controller_type as fn(&mut Globals, i32) -> i32), (0x4a0ae3, get_managing_player)] {
            let Some((orig, _, log, ret)) = original(addr, &[e], &g, 1) else { return };
            let mut port = g.clone();
            assert_eq!(cast::<u32, _>(f(&mut port, e)), ret, "0x{addr:x}({e})");
            assert!(log.is_empty(), "{log:?}");
            cmp().assert(&port, &orig);
        }
    }
}

#[test]
fn diff_get_industry_category() {
    for ind in [1i16, 3, 11, 37, 56, 69, 70] {
        let g = Globals::default();
        let Some((_, _, _, ret)) = original_spec(0x45fb6a, &[format!("i:{ind}")], &g, 1) else { return };
        assert_eq!(cast::<u32, _>(get_industry_category(ind)), ret, "industry {ind}");
    }
}

#[test]
fn diff_phys_commodities() {
    let __content = wsr_core::content::current();
    for seed in 41..=44u64 {
        let g = world(seed);
        // owners of records, with and without a filter
        let owners: Vec<i32> = (1..=8)
            .filter_map(|i| {
                let rec = g.phys_commod_rec[i].to_display();
                rec.split('|').nth(1).map(|o| o.parse().unwrap())
            })
            .collect();
        for (k, &owner) in owners.iter().take(2).enumerate() {
            let filter = if k == 0 { "" } else { " GOLDEN" };
            // GetPhysCommodValue(BYREF entity, BYREF filter): the value is returned in ST0; compare the
            // cost total it leaves in CiPhysCommodCost (the value is checked via CalcEntityFinancials).
            let args = [owner.to_string(), format!("{__wsr_text_13622}{filter}",__wsr_text_13622=__content.string(13622))];
            let Some((orig, _, log, _)) = original_spec(0x65b912, &args, &g, 1) else { return };
            let mut port = g.clone();
            get_phys_commod_value(&mut port, owner, &s(filter));
            assert!(log.is_empty(), "{log:?}");
            cmp().assert(&port, &orig);
            for name in ["GOLD", "OIL", "ETH"] {
                let args = [owner.to_string(), format!("{__wsr_text_13622}{name}",__wsr_text_13622=__content.string(13622))];
                let Some((_, _, _, ret)) = original_spec(0x65be8a, &args, &g, 1) else { return };
                assert_eq!(cast::<u32, _>(find_phys_commod_rec(&g, owner, &s(name))), ret, "{owner} {name}");
            }
        }
    }
}

#[test]
fn diff_get_bank_cd_rate_and_market_rates() {
    let __content = wsr_core::content::current();
    // results are returned in ST0 (not captured by the harness): compare the state (BankCdRate default)
    // and the RND consumption
    for seed in 51..=53u64 {
        let mut g = world(seed);
        g.bank_cd_rate = cast::<f32, _>(Ext::from_f64(0.0));
        let Some((orig, _, _, _)) = original(0x4a333e, &[20], &g, 1) else { return };
        let mut port = g.clone();
        get_bank_cd_rate(&mut port, 20);
        cmp().assert(&port, &orig);
        for t in 1..=3 {
            let Some((orig, rng_after, _, _)) = original(0x4a2ad7, &[t], &g, 12345 + cast::<u32, _>(t)) else { return };
            let mut port = g.clone();
            let mut rng = Rng::from_state(12345 + cast::<u32, _>(t));
            get_market_interest_rate(&mut port, &mut rng, t);
            assert_eq!(rng.state(), rng_after, "{__wsr_text_13745}{t}",__wsr_text_13745=__content.string(13745));
            cmp().assert(&port, &orig);
        }
    }
}

/// Random figures as CalcEntityFinancials leaves them before CalcCreditRating / CalcCorpBondPrice.
fn random_figures(g: &mut Globals, r: &mut Rand) {
    let f = |r: &mut Rand, lo: Ext, hi: Ext| lo + (hi - lo) * cast::<Ext, _>(r.range(0, 1_000_000_000)) / Ext::from_f64(1e9);
    g.tmp_total_debt = f(r, Ext::from_f64(0.0), Ext::from_f64(3000.0));
    g.tmp_credit_net_worth = if r.chance(Ext::from_f64(0.1)) { Ext::from_f64(0.0) } else { f(r, -Ext::from_f64(500.0), Ext::from_f64(3000.0)) };
    g.net_worth = g.tmp_credit_net_worth + f(r, -Ext::from_f64(20.0), Ext::from_f64(20.0));
    g.total_assets = f(r, Ext::from_f64(0.0), Ext::from_f64(6000.0));
    g.long_options_value = if r.chance(Ext::from_f64(0.2)) { f(r, Ext::from_f64(0.0), Ext::from_f64(100.0)) } else { Ext::from_f64(0.0) };
    g.borrow_rate = cast::<f32, _>(cast::<f32, _>(f(r, Ext::from_f64(3.0), Ext::from_f64(20.0))));
    g.credit_rating_str = PbStr::from(["AAA", "AA", "A", "BBB", "BB", "B", "CCC", "CC", "C", "D"][cast::<usize, _>(r.range(0, 9))]);
}

#[test]
fn diff_calc_credit_rating() {
    for seed in 61..=66u64 {
        let mut g = world(seed);
        let mut r = Rand(seed);
        for e in some_entities(&g, &mut r, 5) {
            random_figures(&mut g, &mut r);
            let Some((orig, _, log, _)) = original(0x42b721, &[e], &g, 1) else { return };
            let mut port = g.clone();
            calc_credit_rating(&mut port, e);
            assert!(log.is_empty(), "{log:?}");
            cmp().assert(&port, &orig);
        }
    }
}

#[test]
fn diff_calc_corp_bond_price() {
    let __content = wsr_core::content::current();
    let mut n = 0;
    for seed in 71..=82u64 {
        let mut g = world(seed);
        pow_safe(&mut g, seed);
        let mut r = Rand(seed);
        let mut tried = 0;
        while tried < 4 {
            let e = cast::<i32, _>(r.range(cast::<i64, _>(g.sim_co_count), 1600));
            if g.entity_status[e] != 1 || g.bonds_issued[e] == 0 {
                continue;
            }
            tried += 1;
            random_figures(&mut g, &mut r);
            if r.chance(Ext::from_f64(0.5)) {
                // distressed issuer
                g.credit_rating_str = PbStr::from("D");
                g.credit_rating_num[e] = cast::<i32, _>(r.range(0, 1));
            }
            let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
            let Some((orig, rng_after, log, _)) = original(0x428bcf, &[e], &g, rng0) else { return };
            let mut port = g.clone();
            let mut rng = Rng::from_state(rng0);
            calc_corp_bond_price(&mut port, &mut rng, e);
            assert!(log.is_empty(), "{log:?}");
            cmp().assert(&port, &orig);
            assert_eq!(rng.state(), rng_after, "RND sequence, corp {e}");
            assert!(!cmp().diffs(&g, &orig).is_empty(), "original changed nothing");
            eprintln!(
                "corp {e} {:?} crn {} {:?}: price {} ytm {}{__wsr_text_13845}{}",
                g.bond_issue_info[e], g.credit_rating_num[e], g.credit_rating_str, port.bond_price[e].get(), port.bond_ytm, port.bonds_public_held
            ,__wsr_text_13845=__content.string(13845));
            n += 1;
        }
    }
    eprintln!("{n}{__wsr_text_13846}",__wsr_text_13846=__content.string(13846));
}

#[test]
fn diff_calc_capital_taxes() {
    // the total is returned in ST0 (not captured): compare the components and count it leaves
    for seed in 91..=94u64 {
        let mut g = world(seed);
        g.wealth_tax_rate = 2;
        g.carbon_tax_rate = 3;
        g.health_care_tax_rate = 2;
        g.oil_windfall_tax_rate = 10;
        g.holding_co_tax_rate = 5;
        g.corp_shares_tax_country = 2;
        g.cur_quarter = 4;
        let mut r = Rand(seed);
        for e in some_entities(&g, &mut r, 6) {
            g.net_worth = cast::<Ext, _>(r.range(0, 5000));
            g.total_assets = cast::<Ext, _>(r.range(0, 20000)) + Ext::from_f64(0.5);
            let Some((orig, _, log, _)) = original(0x5077a7, &[e], &g, 1) else { return };
            let mut port = g.clone();
            calc_capital_taxes(&mut port, e);
            assert!(log.is_empty(), "{log:?}");
            cmp().assert(&port, &orig);
        }
    }
}

/// Makes entity `e` small and indebted (so ratings, credit lines and bond prices take their distressed
/// paths), and sometimes ties it to its lending bank's controller.
fn distress(g: &mut Globals, e: i32, r: &mut Rand) {
    let f = |r: &mut Rand, lo: Ext, hi: Ext| lo + (hi - lo) * cast::<Ext, _>(r.range(0, 1_000_000_000)) / Ext::from_f64(1e9);
    for slot in 1..=15 {
        if r.chance(Ext::from_f64(0.8)) {
            g.hold_stock_id[(slot, e)] = 0;
        }
        if r.chance(Ext::from_f64(0.5)) {
            g.hold_bond_id[(slot, e)] = 0;
        }
    }
    g.cash[e] = f(r, -Ext::from_f64(100.0), Ext::from_f64(50.0));
    g.t_bills[e] = if r.chance(Ext::from_f64(0.3)) { f(r, Ext::from_f64(0.0), Ext::from_f64(30.0)) } else { Ext::from_f64(0.0) };
    g.bank_debt[e] = f(r, Ext::from_f64(0.0), Ext::from_f64(3000.0));
    if e > 130 {
        g.business_assets[e] = f(r, Ext::from_f64(0.0), Ext::from_f64(1500.0));
        g.working_capital[e] = f(r, -Ext::from_f64(300.0), Ext::from_f64(200.0));
    }
    if e > 10 && e >= g.sim_co_count && r.chance(Ext::from_f64(0.7)) {
        g.bonds_issued[e] = r.range(10, 2000);
        let mat = g.cur_year + cast::<i32, _>(r.range(1, 20));
        g.bond_issue_info[e] = if r.chance(Ext::from_f64(0.3)) {
            PbStr::from(format!("{mat}|{:.2}|cv|{:5.2}", g.market_cap[e] * f(r, Ext::from(Ext::from_f64(0.3)), Ext::from(Ext::from_f64(1.5))), f(r, Ext::from(Ext::from_f64(2.0)), Ext::from(Ext::from_f64(14.0)))).as_str())
        } else {
            PbStr::from(format!("{mat}|0||{:5.2}", f(r, Ext::from(Ext::from_f64(2.0)), Ext::from(Ext::from_f64(14.0)))).as_str())
        };
    }
    if r.chance(Ext::from_f64(0.4)) {
        let bank = cast::<i32, _>(g.lending_bank[e]);
        if bank > 0 {
            g.corp_controller[bank] = g.corp_controller[e];
        }
    }
    if r.chance(Ext::from_f64(0.3)) {
        g.lending_bank[e] = 0; // assigned at random
    }
}

#[test]
fn diff_calc_entity_financials() {
    let __content = wsr_core::content::current();
    let mut n = 0;
    for seed in 101..=116u64 {
        let mut g = world(seed);
        pow_safe(&mut g, seed);
        let mut r = Rand(seed);
        for (k, e) in some_entities(&g, &mut r, 5).into_iter().enumerate() {
            if (k + cast::<usize, _>(seed)).is_multiple_of(2) {
                distress(&mut g, e, &mut r);
            }
            let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
            let Some((orig, rng_after, log, _)) = original(0x426610, &[e], &g, rng0) else { return };
            let mut port = g.clone();
            let mut rng = Rng::from_state(rng0);
            let mut ui = Scripted::default();
            calc_entity_financials(&mut port, &mut ui, &mut rng, e);
            assert!(ui.log.is_empty(), "{:?}", ui.log);
            assert!(log.is_empty(), "{__wsr_text_13847}{e}: {log:?}",__wsr_text_13847=__content.string(13847));
            cmp().assert(&port, &orig);
            assert_eq!(rng.state(), rng_after, "RND sequence, entity {e}");
            assert!(!cmp().diffs(&g, &orig).is_empty(), "original changed nothing");
            eprintln!(
                "{__wsr_text_13847}{e} ind {}: TA {:.2} NW {:.2}{__wsr_text_13848}{:.2}{__wsr_text_13849}{:.2}{__wsr_text_13850}{:?}{__wsr_text_13851}{}",
                g.industry_num[e], port.total_assets, port.net_worth, port.tmp_total_debt, port.unused_credit_line, port.credit_rating_str, g.bonds_issued[e]
            ,__wsr_text_13847=__content.string(13847), __wsr_text_13848=__content.string(13848), __wsr_text_13849=__content.string(13849), __wsr_text_13850=__content.string(13850), __wsr_text_13851=__content.string(13851));
            n += 1;
            // continue from the original's result (chained calls, like the game)
            g = orig;
        }
    }
    eprintln!("{n} entities compared");
}

/// CalcEntityFinancials for every active insurer (86..130, industry 2, the "insurer" totals with
/// `InsReserves`/`SubprimeSecs` and no `BusinessAssets`, which is dimensioned 131 TO 1600): healthy and
/// distressed balance sheets, with and without lending bank, bonds held and issued. The original runs
/// without any runtime error and the port matches it. (An insurer slot with an operating industry would
/// index BusinessAssets(86..130) -> error 9 -> ON ERROR exit in the original; the game never gives an
/// insurer another industry: wsr101.dat lists 86..130 under IND=02, and every in-game industry change
/// is restricted to ids > 130 or to funds.)
#[test]
fn diff_calc_entity_financials_insurers() {
    let __content = wsr_core::content::current();
    let mut n = 0;
    let mut neg = 0;
    for seed in 201..=206u64 {
        let mut g = world(seed);
        pow_safe(&mut g, seed);
        let mut r = Rand(seed * 7 + 1);
        for e in 86..=130 {
            if g.entity_status[e] != 1 {
                continue;
            }
            assert_eq!(g.industry_num[e], 2);
            if r.chance(Ext::from_f64(0.4)) {
                distress(&mut g, e, &mut r);
                g.ins_reserves[e] *= Ext::from_f64(3.0);
            }
            if r.chance(Ext::from_f64(0.2)) {
                g.contingency_reserve[e] = cast::<Ext, _>(r.range(0, 500));
            }
            let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
            let Some((orig, rng_after, log, _)) = original(0x426610, &[e], &g, rng0) else { return };
            assert!(log.is_empty(), "{__wsr_text_13852}{e}: {log:?}",__wsr_text_13852=__content.string(13852));
            let mut port = g.clone();
            let mut rng = Rng::from_state(rng0);
            let mut ui = Scripted::default();
            calc_entity_financials(&mut port, &mut ui, &mut rng, e);
            assert!(ui.log.is_empty(), "{:?}", ui.log);
            cmp().assert(&port, &orig);
            assert_eq!(rng.state(), rng_after, "RND sequence, insurer {e}");
            assert!(orig.total_assets != Ext::from_f64(0.0), "{__wsr_text_13852}{e}: no figures",__wsr_text_13852=__content.string(13852));
            assert_eq!(port.business_assets.oob_count(), 0, "{__wsr_text_13852}{e}: BusinessAssets index out of range",__wsr_text_13852=__content.string(13852));
            n += 1;
            if port.net_worth < Ext::from_f64(0.0) {
                neg += 1;
            }
            g = orig;
        }
    }
    eprintln!("{n}{__wsr_text_13853}{neg}{__wsr_text_13854}",__wsr_text_13853=__content.string(13853), __wsr_text_13854=__content.string(13854));
    assert!(n > 100 && neg > 0, "{n}{__wsr_text_13855}{neg}{__wsr_text_9001}",__wsr_text_13855=__content.string(13855), __wsr_text_9001=__content.string(9001));
}

/// Gives corp `e` a convertible bond issue with plausible or extreme terms: conversion market cap from
/// 0 (infinite conversion value) through tiny/huge multiples of the market cap, zero/negative market cap
/// or shares, coupons 0..20 %, maturity from last year to +30, any rating.
fn convertible(g: &mut Globals, e: i32, r: &mut Rand) {
    let f = |r: &mut Rand, lo: Ext, hi: Ext| lo + (hi - lo) * cast::<Ext, _>(r.range(0, 1_000_000_000)) / Ext::from_f64(1e9);
    g.bonds_issued[e] = r.range(1, 3000);
    let mat = g.cur_year + cast::<i32, _>(r.range(-1, 30));
    match r.range(0, 9) {
        0 => g.market_cap[e] = Ext::from_f64(0.0),
        1 => g.shares_outstanding[e] = Ext::from_f64(0.0),
        2 => g.market_cap[e] = f(r, Ext::from_f64(1e5), Ext::from_f64(1e9)),
        _ => {}
    }
    let cap = match r.range(0, 7) {
        0 => Ext::from_f64(0.0),
        1 => g.market_cap[e] * f(r, Ext::from_f64(1e-6), Ext::from_f64(1e-3)),
        2 => g.market_cap[e] * f(r, Ext::from_f64(10.0), Ext::from_f64(1000.0)),
        3 => f(r, Ext::from_f64(0.01), Ext::from_f64(1.0)),
        _ => g.market_cap[e] * f(r, Ext::from_f64(0.3), Ext::from_f64(2.0)),
    };
    let coupon = f(r, Ext::from_f64(0.0), Ext::from_f64(20.0));
    g.bond_issue_info[e] = PbStr::from(format!("{mat}|{cap:.2}|cv|{coupon:5.2}").as_str());
    g.credit_rating_num[e] = cast::<i32, _>(r.range(0, 9));
}

#[test]
fn diff_convertible_bonds() {
    let __content = wsr_core::content::current();
    let mut n = 0;
    for seed in 201..=230u64 {
        let mut g = world(seed);
        let mut r = Rand(seed);
        for k in 0..4 {
            let e = cast::<i32, _>(r.range(11, 1600));
            if g.entity_status[e] != 1 {
                continue;
            }
            convertible(&mut g, e, &mut r);
            random_figures(&mut g, &mut r);
            if r.chance(Ext::from_f64(0.2)) {
                g.credit_rating_str = PbStr::from("D");
            }
            let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
            let (addr, what) = if k % 2 == 0 { (0x428bcf, "CalcCorpBondPrice") } else { (0x426610, "CalcEntityFinancials") };
            let Some((orig, rng_after, log, _)) = original(addr, &[e], &g, rng0) else { return };
            let mut port = g.clone();
            let mut rng = Rng::from_state(rng0);
            if addr == 0x428bcf {
                calc_corp_bond_price(&mut port, &mut rng, e);
            } else {
                let mut ui = Scripted::default();
                calc_entity_financials(&mut port, &mut ui, &mut rng, e);
                assert!(ui.log.is_empty(), "{:?}", ui.log);
            }
            eprintln!(
                "{what} corp {e} {:?} mc {}{__wsr_text_13856}{} crn {}: orig price {} ytm {}{__wsr_text_13857}{} | port price {} ytm {}{__wsr_text_13857}{}",
                g.bond_issue_info[e], g.market_cap[e], g.shares_outstanding[e], g.credit_rating_num[e],
                orig.bond_price[e].0, orig.bond_ytm, orig.pending_forced_conv_corp,
                port.bond_price[e].0, port.bond_ytm, port.pending_forced_conv_corp
            ,__wsr_text_13856=__content.string(13856), __wsr_text_13857=__content.string(13857));
            assert!(log.is_empty(), "{what} {e}: {log:?}");
            cmp().assert(&port, &orig);
            assert_eq!(rng.state(), rng_after, "RND sequence, {what} {e}");
            n += 1;
        }
    }
    eprintln!("{n} convertible issues compared");
}
