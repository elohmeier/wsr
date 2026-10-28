//! ShowPlayerStandings, ShowBankLoanCustomers, ShowMyAdvances, UpdateControlChain: unit tests and
//! differential tests against the original (report lines, state, RND state, message boxes).
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::{Opts, Rand, run_original_with};
use wsr_core::logic::reports_lists::{
    set_menus_game_ended, show_bank_loan_customers, show_my_advances, show_player_standings, update_control_chain,
};
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::pb::numeric::Cur;
use wsr_core::state::Globals;

const SHOW_PLAYER_STANDINGS: u32 = 0x446484;
const SHOW_BANK_LOAN_CUSTOMERS: u32 = 0x461796;
const SHOW_MY_ADVANCES: u32 = 0x499f8e;
const UPDATE_CONTROL_CHAIN: u32 = 0x5220c5;

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

// World builder copied from tests/finance.rs (`world`, `pow_safe`).
/// A plausible mid-game world: players 1..NumPlayers, banks 11..85, insurers 86..130, operating corps
/// 131..1580, funds 1581..1583, with random balance sheets, holdings, bonds and economy.
pub fn world(seed: u64) -> Globals {
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


/// A world with names, symbols, currency texts and report flags set.
fn report_world(seed: u64) -> (Globals, Rand) {
    let __content = wsr_core::content::current();
    let mut g = world(seed);
    pow_safe(&mut g, seed);
    let mut r = Rand(seed.wrapping_mul(7919) | 1);
    names(&mut g, &mut r);
    g.suppress_popups = 1;
    g.app_title = s(__content.string(13661));
    g.data_path = PbStr::new(); // orig_msgs matches "CreateProcessA(0x0, 'qm.exe"
    let cur = r.range(0, 3);
    g.currency_code = s(["USA", "EUR", "UK", "JPN"][cast::<usize, _>(cur)]);
    g.currency_name = s(["Dollars", "Euros", "Pounds", "Yen"][cast::<usize, _>(cur)]);
    g.currency_symbol = s(["$", "", "£", "¥"][cast::<usize, _>(cur)]);
    g.currency_suffix = s(["", " Euros", "", ""][cast::<usize, _>(cur)]);
    g.million_prefix_lc = s("m");
    g.million_prefix_uc = s("M");
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    g.rand_seed = cast::<i32, _>(r.range(-30000, 30000));
    for p in 1..=g.num_players {
        g.player_name[p] = s(if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { "...OUT..." } else { "Somebody" });
        g.player_cheat_code[p] = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { cast::<i32, _>(r.range(6, 9)) } else { cast::<i32, _>(r.range(0, 5)) };
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.ent_name[p] = s(&format!("A Very Long Player Name {p}"));
        }
        g.player_ceo_since_year[p] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { 0 } else { cast::<i32, _>(r.range(1990, 2030)) };
        g.lending_bank[p] = cast::<i16, _>(r.range(11, 50));
    }
    g.entity_pick_mode = s(["", "GetCorp", __content.string(6093)][cast::<usize, _>(r.range(0, 2))]);
    (g, r)
}

/// Runs the original and the port of a report routine and compares reports, messages, RND and state.
fn check(
    addr: u32,
    args: &[i32],
    g: &Globals,
    rng0: u32,
    port: impl FnOnce(&mut Globals, &mut Rec, &mut Rng),
) -> Option<usize> {
    let __content = wsr_core::content::current();
    let specs: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let opts = Opts { args: specs, rng: Some(rng0), ..Opts::default() };
    let o = run_original_with(addr, &opts, g)?;
    let mut pg = g.clone();
    let mut ui = Rec::default();
    let mut rng = Rng::from_state(rng0);
    port(&mut pg, &mut ui, &mut rng);
    let orig = orig_reports(&o);
    for (t, l) in &orig {
        let joined: Vec<String> = l.iter().map(|x| x.to_display()).collect();
        eprintln!("  report {:?}: {} lines; ** {} @C {} FROZ {} * {}", t.to_display(), l.len(),
            joined.iter().filter(|x| x.contains("****")).count(), joined.iter().filter(|x| x.contains("@C")).count(),
            joined.iter().filter(|x| x.contains("FROZ")).count(), joined.iter().filter(|x| x.ends_with("  *") || x.contains(__content.string(14009))).count());
    }
    eprintln!("  msgs {:?}", ui_msgs_preview(&o));
    assert_reports(&port_reports(&ui, &[]), &orig);
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "messages: port {:?}, original log {:?}", ui.msgs, o.log);
    assert!(!o.log.iter().any(|l| l.starts_with("PBERR")), "runtime error: {:?}", o.log);
    assert_eq!(rng.state(), o.rng, "RND state");
    cmp().assert(&pg, &o.globals);
    Some(orig.len())
}

#[test]
fn standings_unit() {
    let mut g = Globals::default();
    g.auto_add_stream_list = 5;
    set_menus_game_ended(&mut g);
    assert_eq!(g.auto_add_stream_list, 0);
    // UpdateControlChain is a no-op for players
    let mut g = world(3);
    let before = g.clone();
    let mut ui = Rec::default();
    let mut rng = Rng::from_state(1);
    update_control_chain(&mut g, &mut ui, &mut rng, 1);
    assert!(cmp().diffs(&g, &before).is_empty());
}

#[test]
fn show_player_standings_matches_original() {
    let __content = wsr_core::content::current();
    let mut reports = 0;
    for seed in 1u64..=10 {
        let (mut g, mut r) = report_world(seed * 31 + 7);
        g.long_game_marker = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { 6 } else { 0 };
        g.start_year = 2000;
        g.game_end_year = 2000 + cast::<i32, _>(r.range(10, 50));
        g.antitrust_cheat_used = cast::<i32, _>(r.range(0, 1));
        g.game_end_flag = s(if seed % 3 == 0 { "Y" } else { "N" });
        g.hi_score_done = if seed % 2 == 0 { 0 } else { 1 };
        g.auto_add_stream_list = 1;
        if seed == 4 {
            for p in 1..=5 {
                g.player_cheat_code[p] = 7; // no leader
            }
            g.game_end_flag = s("Y");
        }
        if seed == 5 {
            g.difficulty_level = 3;
        }
        if seed >= 9 {
            // eligible human winner at game end: HiScore (DLL stub) path; huge numbers overflow the table
            g.game_end_flag = s("Y");
            g.hi_score_done = 0;
            g.long_game_marker = 0;
            g.antitrust_cheat_used = 0;
            for p in 1..=5 {
                g.player_cheat_code[p] = 0;
                g.is_computer_player[p] = 0;
                g.player_name[p] = s("Somebody");
            }
            g.cash[1] = if seed == 9 { Ext::from_f64(5.0e12) } else { Ext::from_f64(9.0e5) };
            g.player_name[2] = s("...OUT...");
        }
        // CEO posts at inactive corps, controlled companies, holdings
        for p in 1..=g.num_players {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                let c = cast::<i32, _>(r.range(131, 1580));
                g.player_ceo_of[p] = c;
            }
        }
        let rng0 = cast::<u32, _>(r.next());
        let Some(n) = check(SHOW_PLAYER_STANDINGS, &[], &g, rng0, |g, ui, rng| show_player_standings(g, ui, rng))
        else {
            return;
        };
        reports += n;
        eprintln!("standings seed {seed}: {n}{__wsr_text_14010}",__wsr_text_14010=__content.string(14010));
    }
    assert!(reports >= 8);
}

#[test]
fn update_control_chain_matches_original() {
    for seed in 1u64..=4 {
        let (mut g, mut r) = report_world(seed * 101 + 3);
        g.ctrl_change_msg_mode = if seed % 2 == 0 { 2 } else { 0 };
        let e = cast::<i32, _>(r.range(131, 1580));
        g.entity_status[e] = 1;
        let rng0 = cast::<u32, _>(r.next());
        let o = run_original_with(UPDATE_CONTROL_CHAIN, &Opts { args: vec![e.to_string()], rng: Some(rng0), ..Opts::default() }, &g);
        let Some(o) = o else { return };
        let mut pg = g.clone();
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(rng0);
        update_control_chain(&mut pg, &mut ui, &mut rng, e);
        assert_eq!(rng.state(), o.rng, "RND state");
        assert_eq!(ui.msgs.len(), orig_msgs(&o));
        cmp().assert(&pg, &o.globals);
    }
}

#[test]
fn show_bank_loan_customers_matches_original() {
    let __content = wsr_core::content::current();
    let mut reports = 0;
    for seed in 1u64..=8 {
        let (mut g, mut r) = report_world(seed * 53 + 11);
        let list_mode = [0, 0, 1, 2, 0, 1, 0, 2][cast::<usize, _>(seed) - 1];
        let bank = cast::<i32, _>(r.range(11, 50));
        g.entity_status[bank] = 1;
        g.sel_entity = if seed == 5 { 200 } else { bank };
        g.active_entity = bank;
        // more borrowers at this bank
        for e in 1..=1600 {
            if (e <= g.num_players || 85 < e) && r.chance(if seed % 2 == 0 { cast::<f64, _>(Ext::from_f64(0.02)) } else { cast::<f64, _>(Ext::from_f64(0.005)) }) {
                g.lending_bank[e] = cast::<i16, _>(bank);
            }
        }
        if seed == 7 {
            g.bank_debt[bank + 1] = Ext::from_f64(123.4);
            g.entity_status[bank + 1] = 1;
        }
        let rng0 = cast::<u32, _>(r.next());
        eprintln!("bank loans seed {seed}{__wsr_text_13548}{list_mode}{__wsr_text_14011}{bank}",__wsr_text_13548=__content.string(13548), __wsr_text_14011=__content.string(14011));
        let Some(n) = check(SHOW_BANK_LOAN_CUSTOMERS, &[list_mode], &g, rng0, |g, ui, rng| {
            show_bank_loan_customers(g, ui, rng, list_mode)
        }) else {
            return;
        };
        reports += n;
        eprintln!("bank loans seed {seed}{__wsr_text_13548}{list_mode}: {n}{__wsr_text_14010}",__wsr_text_13548=__content.string(13548), __wsr_text_14010=__content.string(14010));
    }
    assert!(reports >= 6);
}

#[test]
fn show_my_advances_matches_original() {
    let __content = wsr_core::content::current();
    let mut reports = 0;
    for seed in 1u64..=6 {
        let (mut g, mut r) = report_world(seed * 77 + 5);
        let player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
        if seed == 3 {
            for c in 0..=1600 {
                g.player_advances[(player, c)] = Ext::from_f64(0.0);
            }
        } else {
            for _ in 0..r.range(1, 25) {
                let c = cast::<i32, _>(r.range(11, 1600));
                g.player_advances[(player, c)] = cast::<Ext, _>(r.range(1, 100000)) / Ext::from_f64(100.0);
            }
        }
        let rng0 = cast::<u32, _>(r.next());
        let Some(n) = check(SHOW_MY_ADVANCES, &[player], &g, rng0, |g, ui, rng| show_my_advances(g, ui, rng, player))
        else {
            return;
        };
        reports += n;
        eprintln!("advances seed {seed}: {n}{__wsr_text_14010}",__wsr_text_14010=__content.string(14010));
    }
    assert!(reports >= 4);
}


fn ui_msgs_preview(o: &super::diff::Original) -> Vec<String> {
    o.log.iter().filter(|l| l.starts_with("MSGBOX") || l.contains("HiScore")).cloned().collect()
}
