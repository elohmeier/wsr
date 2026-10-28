//! ShowTopCompaniesList (0x49e21f), ShowIndustryReport (0x47f17e), ShowMyHeadlines (0x49d62b).
//!
//! Harness notes (as tests/valuation.rs): CheckPriceAlerts (0x6830fc) is skipped in the original;
//! ShowCashFlowProjection runs on both sides; `TIME$` is fixed (time_secs = 0); bond pricing states are
//! kept PB_Pow-safe.
use wsr_core::pb::real::{Ext, cast};


use super::common::{Rec, assert_reports, cmp, orig_msgs, orig_reports, port_reports, st};
use super::diff::{Opts, Original, Rand, run_original_with};
use wsr_core::logic::reports_lists::{show_industry_report, show_my_headlines, show_top_companies_list};
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::pb::numeric::Cur;
use wsr_core::state::Globals;

const SHOW_TOP_COMPANIES_LIST: u32 = 0x49e21f;
const SHOW_INDUSTRY_REPORT: u32 = 0x47f17e;
const SHOW_MY_HEADLINES: u32 = 0x49d62b;
const SKIPS: [(u32, u32); 1] = [(0x6830fc, 1)];

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

fn f64r(r: &mut Rand, lo: Ext, hi: Ext) -> Ext {
    lo + (hi - lo) * cast::<Ext, _>(r.range(0, 1_000_000_000)) / Ext::from_f64(1e9)
}

fn f32r(r: &mut Rand, lo: Ext, hi: Ext) -> f32 {
    cast::<f32, _>(f64r(r, lo, hi))
}

const SYMS: [&str; 10] = ["XYZ", "BKH", "GOOG", "AAPL", "AMZN", "ABC", "QRS", "IBM", "GE", "Tsla"];

/// A plausible mid-game world (like tests/valuation.rs): players, banks 11..85, insurers 86..130,
/// operating corps 131..1580 (industries 3..`max_ind`), funds 1581..1583; `active` = share of active corps;
/// `convertibles`: som [runtime resource wording] ("cv"; CalcEntityFinancials' convertible pricing is not
/// compared here: it overflows in the port for some of these states, see the report).
fn world(seed: u64, active: Ext, max_ind: i64, convertibles: bool) -> Globals {
    let __content = wsr_core::content::current();
    let mut r = Rand(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut g = Globals::default();
    g.world_generated = 1;
    g.game_end_flag = s("N");
    g.num_players = cast::<i32, _>(r.range(1, 4));
    g.difficulty_level = cast::<i32, _>(r.range(1, 4));
    g.cur_year = cast::<i32, _>(r.range(2000, 2030));
    g.start_year = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { g.cur_year } else { g.cur_year - cast::<i32, _>(r.range(1, 10)) };
    g.cur_quarter = if seed.is_multiple_of(2) { 1 } else { 3 };
    g.sim_co_count = if seed.is_multiple_of(3) { 300 } else { 450 };
    g.base_interest_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(16.0)));
    g.prime_rate = cast::<f32, _>(g.base_interest_rate + f32r(&mut r, Ext::from_f64(0.0), Ext::from_f64(3.0)));
    g.long_bond_yield = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(12.0)));
    g.short_bond_yield = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(10.0)));
    g.sofr_rate = cast::<f32, _>(g.base_interest_rate + f32r(&mut r, -Ext::from_f64(0.5), Ext::from_f64(0.5)));
    g.gdp_growth = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(5.0), Ext::from_f64(8.0)));
    g.gdp_growth_target = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(5.0), Ext::from_f64(8.0)));
    g.monetary_policy = cast::<i16, _>(r.range(0, 2));
    g.monetary_stance = cast::<i32, _>(r.range(0, 2));
    g.exchange_rate = cast::<f64, _>(Ext::from_f64(1.0));
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
    g.stock_index = f64r(&mut r, Ext::from_f64(500.0), Ext::from_f64(20000.0));
    g.stock_index_divisor = f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(5.0));
    g.market_valuation = cast::<f32, _>([Ext::from_f64(0.3), Ext::from_f64(0.8), Ext::from_f64(1.0), Ext::from_f64(1.2)][cast::<usize, _>(r.range(0, 3))]);
    g.hot_stock_corp = cast::<i16, _>(r.range(131, 1580));
    g.million_prefix_uc = s(["M", "B", "M"][cast::<usize, _>(r.range(0, 2))]);
    g.rand_seed = cast::<i32, _>(r.range(0, 32600));
    g.game_in_progress = 0;
    g.active_corp_count = cast::<i32, _>(r.range(700, 1300));
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    g.cur_player_name = s(&format!("{__wsr_text_13556}{}", g.cur_player,__wsr_text_13556=__content.string(13556)));
    for p in 1..=5 {
        g.player_name[p] = s(&format!("{__wsr_text_13556}{p}",__wsr_text_13556=__content.string(13556)));
    }
    for ind in 0..=71 {
        g.ind_name[ind] = s(&format!("{__wsr_text_13991}{ind}",__wsr_text_13991=__content.string(13991)));
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
    for e in 1..=1600 {
        let on = if e <= 10 { e <= g.num_players } else { (1581..=1583).contains(&e) || r.chance(cast::<f64, _>(active)) };
        g.entity_status[e] = if on { 1 } else if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { 0 } else { 2 };
        g.ent_name[e] = s(&format!("{__wsr_text_13758}{e}",__wsr_text_13758=__content.string(13758)));
        g.ent_symbol[e] = s(SYMS[cast::<usize, _>(r.range(0, 9))]);
        g.cash[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.05))) { Ext::from_f64(0.0) } else { f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(3000.0)) };
        g.t_bills[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(1000.0)) } else { Ext::from_f64(0.0) };
        g.bank_debt[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(2000.0)) } else { Ext::from_f64(0.0) };
        g.lending_bank[e] = cast::<i16, _>(r.range(11, 50));
        g.credit_rating_num[e] = cast::<i32, _>(r.range(0, 10));
        g.corp_controller[e] = match r.range(0, 5) {
            0 => 0,
            1 | 2 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
            3 => e,
            _ => cast::<i32, _>(r.range(11, 1600)),
        };
        for slot in 1..=15 {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
                g.hold_stock_id[(slot, e)] = cast::<i16, _>(r.range(11, 1600));
                g.hold_stock_pct[(slot, e)] = cast::<i16, _>(r.range(1, 100));
            }
        }
        if e > 10 {
            g.industry_num[e] = match e {
                11..=85 => 1,
                86..=130 => 2,
                1581..=1583 => 71,
                _ => cast::<i16, _>(r.range(3, max_ind)),
            };
            g.market_cap[e] = f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(300.0));
            if r.chance(cast::<f64, _>(Ext::from_f64(0.02))) && e < g.sim_co_count {
                g.market_cap[e] = Ext::from_f64(1e7) + f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(1e7)); // wide cap column (no bonds: see world())
            }
            g.shares_outstanding[e] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(50.0));
            g.dividend_per_share[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(3.0)) } else { Ext::from_f64(0.0) };
            g.tax_loss_carry[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { f64r(&mut r, -Ext::from_f64(100.0), Ext::from_f64(0.0)) } else { Ext::from_f64(0.0) };
            g.accrued_pl[e] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(200.0));
            g.consol_tax_inc_ytd[e] = f64r(&mut r, -Ext::from_f64(30.0), Ext::from_f64(100.0));
            g.earnings_y1[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.08))) { Ext::from_f64(0.0) } else { f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(100.0)) };
            g.earnings_yr2_ago[e] = f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(100.0));
            g.earnings_yr3_ago[e] = f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(100.0));
            g.earnings_yr4_ago[e] = f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(100.0));
            g.earnings_ytd[e] = f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(60.0));
            if g.earnings_ytd[e] == Ext::from_f64(0.0) {
                g.earnings_ytd[e] = Ext::from_f64(1.0);
            }
            g.proj_qtr_earnings[e] = f64r(&mut r, -Ext::from_f64(10.0), Ext::from_f64(30.0));
            g.corp_prior_net_worth[e] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(3000.0));
            g.profit_margin[e] = cast::<i16, _>(r.range(-25, 40));
            g.growth_rate[e] = cast::<i16, _>(r.range(-10, 60));
            g.disc_spend_pct[e] = cast::<i16, _>(r.range(0, 15));
            g.investor_sentiment[e] = cast::<i32, _>(r.range(-60, 70));
            g.ent_misc_data[e] = s(&format!("{:.2}", f64r(&mut r, Ext::from(-Ext::from_f64(5.0)), Ext::from(Ext::from_f64(20.0)))));
            if e >= g.sim_co_count && r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                g.bonds_issued[e] = r.range(1, 1500);
                let mat = g.cur_year + cast::<i32, _>(r.range(1, 25));
                let coupon = f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(15.0));
                let cv = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) && convertibles { "cv" } else { "" };
                g.bond_issue_info[e] = s(&format!("{mat}|0|{cv}|{coupon:5.2}"));
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
        }
        if (86..=130).contains(&e) {
            g.ins_reserves[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(10000.0));
        }
    }
    for e in 11..g.sim_co_count {
        g.bonds_issued[e] = 0; // PB_Pow-safe bond pricing (see tests/valuation.rs)
    }
    for series in 0..=1606 {
        if r.chance(cast::<f64, _>(Ext::from_f64(0.05))) {
            continue;
        }
        let cur = cast::<i32, _>(r.range(1, 60));
        g.cap_hist_close[(0, series)] = cast::<Ext, _>(cur);
        let months = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { cast::<i32, _>(r.range(1, 30)) } else { 60 };
        let mut v = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(500.0));
        for k in 0..months {
            let mut slot = cur - k;
            if slot < 1 {
                slot += 60;
            }
            g.cap_hist_close[(slot, series)] = v;
            g.price_hist_high[(slot, series)] = v * Ext::from_f64(1.1);
            g.price_hist_low[(slot, series)] = v * Ext::from_f64(0.9);
            v *= Ext::from_f64(1.0) + f64r(&mut r, -Ext::from_f64(0.06), Ext::from_f64(0.06));
        }
    }
    g
}

fn run(addr: u32, arg: i32, g: &Globals, rng: u32) -> Option<Original> {
    let opts = Opts { args: vec![arg.to_string()], rng: Some(rng), skips: SKIPS.to_vec(), ..Opts::default() };
    run_original_with(addr, &opts, g)
}

/// Report lines, popups, state and RND state of port vs original.
#[track_caller]
fn check(port: &Globals, ui: &Rec, rng: &Rng, o: &Original) {
    let orig = orig_reports(o);
    assert_reports(&port_reports(ui, &[]), &orig);
    assert_eq!(ui.msgs.len(), orig_msgs(o), "popups: port {:?}", ui.msgs);
    assert_eq!(rng.state(), o.rng, "RND state");
    cmp().assert(port, &o.globals);
}

// ------------------------------------------------------------------------------------------ unit tests

#[test]
fn my_headlines_sections_and_matching() {
    let __content = wsr_core::content::current();
    let mut g = Globals::default();
    g.cur_player = 1;
    g.num_players = 1;
    g.cur_player_name = s("Joe");
    g.entity_status[1] = 1;
    g.ent_symbol[1] = s("JOE");
    for c in 11..=20 {
        g.entity_status[c] = 1;
        g.ent_symbol[c] = s(&format!("S{c}"));
    }
    g.hold_stock_id[(1, 1)] = 12;
    g.corp_controller[15] = 1;
    g.news_headline[1] = s("JOE buys stock.");
    g.news_headline[2] = s("Big news for (S12) today");
    g.news_headline[3] = s("Nothing about S99");
    g.news_headline[4] = s("Merger: S15 and S12 talk");
    let mut ui = Rec::default();
    show_my_headlines(&mut g, &mut ui, &mut Rng::from_state(1));
    let (title, lines) = &ui.reports[0];
    assert_eq!(*title, __content.string(8633));
    let l: Vec<String> = lines.iter().map(|x| x.to_display()).collect();
    assert_eq!(l[1], __content.string(14444));
    assert_eq!(l[3], " - JOE buys stock.");
    assert_eq!(l[5], __content.string(8628));
    assert_eq!(l[7], " - Big news for (S12) today");
    assert_eq!(l[8], " - Merger: S15 and S12 talk"); // first matching section only
    assert!(l.iter().all(|x| !x.contains("S99")));
    assert!(l.iter().all(|x| !x.contains(__content.string(13992)))); // empty section: heading blanked
    // nothing found -> popup
    let mut g2 = Globals::default();
    g2.cur_player = 1;
    let mut ui2 = Rec::default();
    show_my_headlines(&mut g2, &mut ui2, &mut Rng::from_state(1));
    assert!(ui2.reports.is_empty());
    assert_eq!(ui2.msgs, vec![__content.string(8632).to_string()]);
}

#[test]
fn industry_report_needs_industry() {
    let mut g = Globals::default();
    let mut ui = Rec::default();
    let mut rng = Rng::from_state(1);
    show_industry_report(&mut g, &mut ui, &mut rng, 0x10000); // low word 0
    assert!(ui.reports.is_empty());
    assert_eq!(ui.msgs.len(), 1);
}

// ---------------------------------------------------------------------------------- differential tests

#[test]
fn diff_show_my_headlines() {
    let __content = wsr_core::content::current();
    for seed in 1u64..=6 {
        let mut r = Rand(seed * 7717);
        let mut g = world(seed, Ext::from_f64(0.9), 40, true);
        g.cur_player_name = s(["Joe", "Ann Lee", "ACME"][cast::<usize, _>(r.range(0, 2))]);
        g.ent_symbol[g.cur_player] = s(["JOE", "ANN", "ACM"][cast::<usize, _>(r.range(0, 2))]);
        // headlines mentioning random symbols in the matched forms
        let n = if seed == 6 { 0 } else { r.range(10, 60) };
        for i in 1..=cast::<i32, _>(n) {
            let e = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { g.cur_player } else { cast::<i32, _>(r.range(11, 1600)) };
            let sym = g.ent_symbol[e].to_display();
            let form = match r.range(0, 5) {
                0 => format!("News on {sym}{__wsr_text_13983}",__wsr_text_13983=__content.string(13983)),
                1 => format!("Report ({sym}) out"),
                2 => format!("{__wsr_text_13993}{sym}.",__wsr_text_13993=__content.string(13993)),
                3 => format!("{sym}{__wsr_text_13994}",__wsr_text_13994=__content.string(13994)),
                4 => format!("x {} y", sym.to_uppercase()),
                _ => "Market calm".to_string(),
            };
            g.news_headline[i] = s(&form);
        }
        // option records "corp|type|holder|qty|strike|month|year|premium"
        for i in 1..=cast::<i32, _>(r.range(0, 8)) {
            let holder = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { g.cur_player } else { cast::<i32, _>(r.range(0, 1600)) };
            g.option_recs[i] = s(&format!("{}|1|{}|5|10|6|{}|1.5", r.range(11, 1600), holder, g.cur_year + 1));
        }
        let Some(o) = run(SHOW_MY_HEADLINES, 0, &g, 1) else { return };
        let mut port = g.clone();
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(1);
        show_my_headlines(&mut port, &mut ui, &mut rng);
        eprintln!("seed {seed}: {}{__wsr_text_13751}{} popups", ui.reports.len(), ui.msgs.len(),__wsr_text_13751=__content.string(13751));
        check(&port, &ui, &rng, &o);
    }
}

#[test]
fn diff_show_top_companies_list() {
    let __content = wsr_core::content::current();
    let mut n_lines = 0;
    for seed in 1u64..=12 {
        let list_type = [1, 2, 3, 4, 1, 2, 3, 4, 5, 3, 3, 4][cast::<usize, _>(seed - 1)];
        // few active corps keep the emulated CalcEntityFinancials calls affordable
        let mut g = world(seed, if seed > 10 { Ext::from_f64(0.015) } else { Ext::from_f64(0.08) }, 12, false);
        g.sel_entity = 0;
        if seed == 10 {
            for e in 11..=1600 {
                g.cash[e] = -Ext::from_f64(100.0); // list 3: no corp qualifies -> popup
                g.t_bills[e] = Ext::from_f64(0.0);
            }
        }
        if seed > 10 {
            // short lists: zero balances / non-positive caps mix with the sort's trailing empty lines
            for e in (11..=1600).step_by(2) {
                g.cash[e] = Ext::from_f64(0.0);
                g.t_bills[e] = Ext::from_f64(0.0);
                g.market_cap[e] = [Ext::from_f64(0.0), -Ext::from_f64(1.0), Ext::from_f64(0.0)][cast::<usize, _>(e % 3)];
            }
        }
        if seed == 7 {
            // zero balances mix with the sort's trailing empty lines
            for e in (11..=1600).step_by(3) {
                g.cash[e] = Ext::from_f64(0.0);
                g.t_bills[e] = Ext::from_f64(0.0);
            }
        }
        let rng0 = 1000 + cast::<u32, _>(seed);
        let Some(o) = run(SHOW_TOP_COMPANIES_LIST, list_type, &g, rng0) else { return };
        let mut port = g.clone();
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(rng0);
        show_top_companies_list(&mut port, &mut ui, &mut rng, list_type);
        eprintln!(
            "seed {seed}{__wsr_text_13995}{list_type}: {} report lines, {} popups, {:.1}{__wsr_text_13748}",
            ui.reports.first().map_or(0, |x| x.1.len()),
            ui.msgs.len(),
            o.elapsed
        ,__wsr_text_13995=__content.string(13995), __wsr_text_13748=__content.string(13748));
        n_lines += ui.reports.first().map_or(0, |x| x.1.len());
        check(&port, &ui, &rng, &o);
    }
    assert!(n_lines == 0 || n_lines > 100);
}

#[test]
fn diff_show_industry_report() {
    let __content = wsr_core::content::current();
    for seed in 1u64..=8 {
        let mut r = Rand(seed * 31);
        let mut g = world(seed, Ext::from_f64(0.5), 40, true);
        g.sel_entity = 0;
        // Corps > 85 call ShowCashFlowProjection from CalcStockPrice (skipped: in this nested position the
        // skip leaves a stale x87 value as its result). Operating industries are therefore only run after
        // the game has ended (CalcStockPrice mode 2 returns at once; CalcAnalystRating's mode 4 does not
        // reach the call for these corps' states, see the assertions).
        let industry = match seed {
            1 | 5 => 71,
            2 | 6 => 1,
            8 => 0,
            _ => cast::<i32, _>(r.range(3, 40)),
        };
        if (3..=69).contains(&industry) {
            g.game_end_flag = st("Y");
        }
        if seed % 2 == 0 {
            g.entity_pick_mode = st(__content.string(6093));
        }
        let rng0 = 77 + cast::<u32, _>(seed);
        let Some(o) = run(SHOW_INDUSTRY_REPORT, industry, &g, rng0) else { return };
        let mut port = g.clone();
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(rng0);
        show_industry_report(&mut port, &mut ui, &mut rng, industry);
        eprintln!(
            "seed {seed}{__wsr_text_1975}{industry}: {} report lines, {:.1}{__wsr_text_13748}",
            ui.reports.first().map_or(0, |x| x.1.len()),
            o.elapsed
        ,__wsr_text_1975=__content.string(1975), __wsr_text_13748=__content.string(13748));
        check(&port, &ui, &rng, &o);
    }
}
