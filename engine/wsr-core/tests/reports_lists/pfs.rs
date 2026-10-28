//! Tests of `logic::reports_lists` pfs: ShowPersonalFinancialStatement (0x43987e), CalcPlayerIncomeTax
//! (0x508797), GetEntitySoleCommodity (0x6358eb). Unit tests plus differential tests against the original.
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::{Opts, Rand, run_original_with};
use wsr_core::logic::cashflow::calc_player_income_tax;
use wsr_core::logic::reports_corp::get_entity_sole_commodity;
use wsr_core::logic::reports_lists::*;
use wsr_core::pb::numeric::str_num;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::pb::numeric::Cur;
use wsr_core::state::Globals;

const SHOW_PFS: u32 = 0x43987e;
const CALC_PLAYER_INCOME_TAX: u32 = 0x508797;
const GET_ENTITY_SOLE_COMMODITY: u32 = 0x6358eb;

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

// (copied from tests/finance.rs)
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


fn f64r(r: &mut Rand, lo: Ext, hi: Ext) -> Ext {
    lo + (hi - lo) * cast::<Ext, _>(r.range(0, 1_000_000_000)) / Ext::from_f64(1e9)
}

fn str15(x: Ext) -> String {
    str_num(x, 15).trim().to_string()
}

/// A world for the personal financial statement: names, currency texts, a selected corp the current
/// player has stock / cv bonds / options in, option records, headlines naming the player, swaps.
fn pfs_world(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut g = world(seed);
    pow_safe(&mut g, seed);
    let mut r = Rand(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) | 1);
    for e in 1..=1600 {
        g.ent_symbol[e] = s(&format!("S{e}"));
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.ent_name[e] = s(&format!("{}{__wsr_text_13996}{e}", word(&mut r, 9).to_display(),__wsr_text_13996=__content.string(13996)));
        }
    }
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    let p = g.cur_player;
    g.entity_status[p] = 1;
    for q in 1..=5 {
        g.ent_name[q] = s(&format!("{} {}", word(&mut r, 6).to_display(), word(&mut r, 8).to_display()));
        g.player_name[q] = g.ent_name[q].clone();
        g.player_cap_gains[q] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { Ext::from_f64(0.0) } else { f64r(&mut r, -Ext::from_f64(300.0), Ext::from_f64(600.0)) };
        g.player_ceo_since_year[q] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { 0 } else { g.cur_year - cast::<i32, _>(r.range(0, 10)) };
        g.player_short_margin_bal[q] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(200.0)) } else { Ext::from_f64(0.0) };
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        g.player_ceo_of[p] = 0;
    }
    let (sym, suf, code, name) = [("$", "", "USD", "Dollar"), ("", " Euro", "EUR", "Euro"), ("£", "", "GBP", "Pound")]
        [cast::<usize, _>(r.range(0, 2))];
    g.currency_symbol = s(sym);
    g.currency_suffix = s(suf);
    g.currency_code = s(code);
    g.currency_name = s(name);
    g.million_prefix_lc = s("m");
    g.million_prefix_uc = s("M");
    g.suppress_popups = 1;
    g.t_bill_rate = cast::<f32, _>((cast::<f32, _>(r.range(50, 900))) / Ext::from_f64(100.0));
    // the selected corp, with positions of the player in it
    let sel = cast::<i32, _>(r.range(131, 1580));
    g.entity_status[sel] = 1;
    g.last_sel_entity = if r.chance(cast::<f64, _>(Ext::from_f64(0.85))) { sel } else { cast::<i32, _>(r.range(0, 10)) };
    if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
        let slot = cast::<i32, _>(r.range(1, 15));
        g.hold_stock_id[(slot, p)] = cast::<i16, _>(sel);
        g.hold_stock_pct[(slot, p)] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { -(cast::<i16, _>(r.range(1, 20))) } else { cast::<i16, _>(r.range(1, 40)) };
        g.hold_stock_basis[(slot, p)] = f64r(&mut r, -Ext::from_f64(5.0), Ext::from_f64(200.0));
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
        let slot = cast::<i32, _>(r.range(1, 15));
        let mat = g.cur_year + cast::<i32, _>(r.range(0, 6));
        g.bond_issue_info[sel] = s(&format!("{mat}|{:.2}|cv|{:5.2}", g.market_cap[sel] * f64r(&mut r, Ext::from(Ext::from_f64(0.5)), Ext::from(Ext::from_f64(1.5))), f64r(&mut r, Ext::from(Ext::from_f64(2.0)), Ext::from(Ext::from_f64(12.0)))));
        g.bond_price[sel] = Cur::from_f64(f64r(&mut r, Ext::from_f64(60.0), Ext::from_f64(140.0)));
        g.hold_bond_id[(slot, p)] = sel;
        g.hold_bond_face[(slot, p)] = r.range(1, 300);
        g.bond_hold_cost[(slot, p)] = cast::<Ext, _>(g.hold_bond_face[(slot, p)]) * f64r(&mut r, Ext::from_f64(0.6), Ext::from_f64(1.4));
    }
    // monthly price history (option volatility)
    g.cap_hist_close[(0, sel)] = cast::<Ext, _>(r.range(1, 60));
    for m in 1..=60 {
        let close = g.market_cap[sel] * f64r(&mut r, Ext::from_f64(0.6), Ext::from_f64(1.4));
        g.cap_hist_close[(m, sel)] = close;
        g.price_hist_high[(m, sel)] = close * f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(1.2));
        g.price_hist_low[(m, sel)] = close * f64r(&mut r, Ext::from_f64(0.8), Ext::from_f64(1.0));
    }
    let n_opt = cast::<i32, _>(if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { r.range(1, 5) } else { 0 });
    for k in 1..=n_opt {
        let holder = if r.chance(cast::<f64, _>(Ext::from_f64(0.8))) { p } else { cast::<i32, _>(r.range(131, 1580)) };
        let mut qty = r.range(1, 25);
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            qty = -qty;
        }
        let strike = (g.market_cap[sel] * f64r(&mut r, Ext::from_f64(0.3), Ext::from_f64(2.0)) * Ext::from_f64(100.0)).round() / Ext::from_f64(100.0);
        let month = [1, 4, 7, 10][cast::<usize, _>(r.range(0, 3))];
        let year = g.cur_year + cast::<i32, _>(r.range(0, 2));
        let prem = (g.market_cap[sel] * f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(0.3)) * cast::<Ext, _>(qty.abs()) * Ext::from_f64(100.0)).round() / Ext::from_f64(100.0);
        g.option_recs[k] = s(&format!(
            "{}|{}|{}|{}|{}|{}|{}|{}",
            str15(Ext::from(cast::<Ext, _>(sel))),
            str15(Ext::from(cast::<Ext, _>(r.range(1, 2)))),
            str15(Ext::from(cast::<Ext, _>(holder))),
            str15(Ext::from(cast::<Ext, _>(qty))),
            str15(strike),
            str15(Ext::from(cast::<Ext, _>(month))),
            str15(Ext::from(cast::<Ext, _>(year))),
            str15(if qty < 0 { -prem } else { prem })
        ));
    }
    g.option_recs[0] = s(&n_opt.to_string());
    // headlines, some naming the player
    let up = g.ent_name[p].to_display().to_uppercase();
    for i in 1..=60 {
        g.news_headline[i] = match r.range(0, 9) {
            0 => s(&format!("{} buys a yacht", g.ent_name[p].to_display())),
            1 => s(&format!("Analysts praise {up}.")),
            _ => s(&format!("Old headline {i}.")),
        };
    }
    // swaps of the player
    if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
        let n = cast::<i32, _>(r.range(1, 3));
        for i in 1..=n {
            let other = r.range(11, 85);
            let (a, b) = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { (cast::<i64, _>(p), other) } else { (other, cast::<i64, _>(p)) };
            let fixed = cast::<Ext, _>(r.range(100, 1300)) / Ext::from_f64(100.0);
            let ty = ["P", "L"][cast::<usize, _>(r.range(0, 1))];
            let ey = g.cur_year + cast::<i32, _>(r.range(1, 8));
            g.swap_contracts[i] = s(&format!("{a}|{b}|{fixed}|{ty}|{}.{}|{ey}.{}|{}", g.cur_year - 1, r.range(1, 4), r.range(1, 4), r.range(1, 600)));
        }
        g.swap_contracts[0] = s(&n.to_string());
    }
    g
}

// ------------------------------------------------------------------------------------- unit tests

#[test]
fn player_income_tax_by_hand() {
    let mut g = Globals::default();
    g.corp_tax_rate = 25;
    g.cap_gains_tax_rate = 20;
    g.player_cap_gains[1] = Ext::from_f64(50.0);
    g.extraordinary_pl[1] = 10;
    g.accrued_pl[1] = Ext::from_f64(90.0);
    // (100 * 30% + 50 * 20%) with the SINGLE 0.01
    let t = calc_player_income_tax(&mut g, 1);
    assert!((t - Ext::from_f64(40.0)).abs() < Ext::from_f64(1e-5), "{t}");
    assert_eq!(g.tmp_ext1, Ext::from_f64(150.0));
    // an ordinary loss offsets the gains, taxed at the capital-gains rate
    g.accrued_pl[1] = -Ext::from_f64(40.0); // ordinary -30
    let t = calc_player_income_tax(&mut g, 1);
    assert!((t - Ext::from_f64(4.0)).abs() < Ext::from_f64(1e-6), "{t}");
    assert_eq!(g.tmp_ext1, Ext::from_f64(20.0));
    g.player_cap_gains[1] = -Ext::from_f64(10.0);
    assert_eq!(calc_player_income_tax(&mut g, 1), Ext::from_f64(0.0));
    assert_eq!(g.tmp_ext1, -Ext::from_f64(30.0));
}

#[test]
fn sole_commodity_by_hand() {
    let mut g = Globals::default();
    assert_eq!(get_entity_sole_commodity(&g, 3, 2), "NO");
    g.phys_commod_rec[4] = s(" GOLD |3|10|5");
    g.phys_commod_rec[9] = s("SILVER|4|10|5");
    assert_eq!(get_entity_sole_commodity(&g, 3, 2), "GOLD");
    g.phys_commod_rec[12] = s("GOLD|3|1|1");
    assert_eq!(get_entity_sole_commodity(&g, 3, 2), "GOLD");
    g.phys_commod_rec[13] = s("OIL|3|1|1");
    assert_eq!(get_entity_sole_commodity(&g, 3, 2), "NO");
    assert_eq!(get_entity_sole_commodity(&g, 4, 2), "SILVER");
    assert_eq!(get_entity_sole_commodity(&g, 4, 1), "NO");
    g.futures_recs[1] = s("CORN|4|x");
    assert_eq!(get_entity_sole_commodity(&g, 4, 1), "CORN");
}

// ----------------------------------------------------------------------------- differential tests

#[test]
fn calc_player_income_tax_matches_original() {
    let __content = wsr_core::content::current();
    for seed in 1..=8u64 {
        let g = pfs_world(seed);
        for p in 1..=g.num_players {
            let Some(o) = run_original_with(CALC_PLAYER_INCOME_TAX, &Opts { args: vec![p.to_string()], ..Opts::default() }, &g)
            else {
                return;
            };
            let mut port = g.clone();
            let t = calc_player_income_tax(&mut port, p);
            let ot = o.return_float.expect("ST0");
            assert!((t - ot).abs() <= Ext::from_f64(1e-12) * ot.abs().max(Ext::from(Ext::from_f64(1.0))), "{__wsr_text_13997}{p}: {t} vs {ot}",__wsr_text_13997=__content.string(13997));
            cmp().assert(&port, &o.globals);
        }
    }
}

#[test]
fn get_entity_sole_commodity_matches_original() {
    let __content = wsr_core::content::current();
    for seed in 1..=4u64 {
        let mut g = pfs_world(seed);
        let mut r = Rand(seed);
        for i in 1..=40 {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                let code = ["GOLD", "OIL", __content.string(13998), "BITCOIN"][cast::<usize, _>(r.range(0, 3))];
                g.futures_recs[i] = s(&format!("{code}|{}|5|1|2030", r.range(1, 4)));
            }
        }
        for (e, kind) in [(1, 1), (2, 1), (3, 2), (1, 2), (2, 2), (200, 2)] {
            let Some(o) = run_original_with(
                GET_ENTITY_SOLE_COMMODITY,
                &Opts { args: vec![e.to_string(), kind.to_string()], ..Opts::default() },
                &g,
            ) else {
                return;
            };
            let port = get_entity_sole_commodity(&g, e, kind);
            assert_eq!(Some(port), o.return_str, "{__wsr_text_13847}{e}{__wsr_text_13623}{kind}",__wsr_text_13847=__content.string(13847), __wsr_text_13623=__content.string(13623));
        }
    }
}

fn run_pfs(g: &Globals, view_mode: i32, rng0: u32) {
    let __content = wsr_core::content::current();
    let opts = Opts {
        args: vec![view_mode.to_string()],
        rng: Some(rng0),
        ..Opts::default()
    };
    let Some(o) = run_original_with(SHOW_PFS, &opts, g) else { return };
    let mut port = g.clone();
    let mut ui = Rec::default();
    let mut rng = Rng::from_state(rng0);
    show_personal_financial_statement(&mut port, &mut ui, &mut rng, view_mode);
    let orig = orig_reports(&o);
    eprintln!(
        "{__wsr_text_13997}{}{__wsr_text_13999}{view_mode}: {}{__wsr_text_13751}{}{__wsr_text_14000}{:.2}{__wsr_text_13748}",
        g.cur_player,
        orig.len(),
        orig.first().map_or(0, |r| r.1.len()),
        o.elapsed
    ,__wsr_text_13997=__content.string(13997), __wsr_text_13999=__content.string(13999), __wsr_text_13751=__content.string(13751), __wsr_text_14000=__content.string(14000), __wsr_text_13748=__content.string(13748));
    if std::env::var("PFS_DUMP").is_ok() { for l in &orig[0].1 { eprintln!("  {}", l.to_display()); } }
    assert_reports(&port_reports(&ui, &[]), &orig);
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "messages: port {:?}, original log {:?}", ui.msgs, o.log);
    assert_eq!(rng.state(), o.rng, "RND state");
    cmp().rel_tol(Ext::from_f64(1e-12)).assert(&port, &o.globals);
}

#[test]
fn show_personal_financial_statement_matches_original() {
    for seed in 1..=16u64 {
        let mut g = pfs_world(seed);
        let mut r = Rand(seed * 31);
        let p = g.cur_player;
        if seed % 4 == 0 {
            // overdrawn within the credit line
            g.cash[p] = -f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(30.0));
        }
        if seed % 5 == 0 {
            // big balances: the "* #################," format
            g.cash[p] = Ext::from_f64(150_000.0) + f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(1e6));
        }
        if seed % 6 == 0 {
            // minimal assets: no cash flow statement
            for slot in 0..=15 {
                g.hold_stock_id[(slot, p)] = 0;
                g.hold_bond_id[(slot, p)] = 0;
            }
            g.cash[p] = Ext::from_f64(1.0);
            g.t_bills[p] = Ext::from_f64(0.0);
            g.bank_debt[p] = Ext::from_f64(0.0);
            g.long_gov_bond_face[p] = 0;
            g.short_gov_bond_face[p] = 0;
        }
        if seed % 7 == 0 || seed == 13 {
            // the player's only physical commodity is a cryptocurrency
            let code = if seed == 13 { "BITCOIN" } else { "ETHEREUM" };
            let n = 31;
            for i in 1..=n {
                g.phys_commod_rec[i] = PbStr::new();
            }
            g.phys_commod_rec[1] = s(&format!("{code}|{p}|{}|10", r.range(10, 900)));
            g.phys_commod_rec[2] = s(&format!("GOLD|{}|5|10", p % 5 + 1));
            g.phys_commod_rec[3] = s(&format!(" {code} |{p}|3|10"));
            g.phys_commod_rec[0] = s("3");
        }
        let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
        run_pfs(&g, cast::<i32, _>(seed % 3 == 0), rng0);
    }
}
