//! Tests of `logic::research`: unit tests of the small callees and differential tests of
//! ShowResearchReport (0x448ed3) and its callees against the original under emulation (tools/emu/game.py,
//! see tests/diff/mod.rs): the report lines the original adds to the report listbox vs the lines the port
//! passes to `Ui::report`, the state, the RND sequence and the message boxes.
//!
//! Harness notes:
//! * CalcStockPrice (`logic::valuation`) does not call ShowCashFlowProjection yet (its PORT-TODO 0x6a20e0).
//!   The report runs CalcStockPrice in mode 4 for public companies (and mode 3 for a corp without shares),
//!   so for public companies > 85 the original's extra projection can change the target price / state.
//!   The tests cover public banks (no projection at all), private (100% owned) insurers and companies,
//!   and public funds and companies on sampled states where the outcome agrees; report corps get a valid
//!   share count.
//! * CalcEntityFinancials prices bonds with PB_Pow, whose integer-exponent path Unicorn mis-detects; the
//!   states are restricted like tests/valuation.rs (`pow_safe`).
//! * A lender that is not a bank (`LendingBank` outside 11..85) also makes CalcEntityFinancials itself hit
//!   error 9 in the original (handled there, not in its port), so error exits are tested with other
//!   out-of-range ids.

#![allow(clippy::field_reassign_with_default)]
use wsr_core::pb::real::{Ext, cast};


mod diff;

use diff::{Compare, Opts, Original, Rand, run_original_with, par_seeds};
use wsr_core::logic::research::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::pb::numeric::Cur;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Scripted, Ui};

const SHOW_RESEARCH_REPORT: u32 = 0x448ed3;

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

fn f64r(r: &mut Rand, lo: Ext, hi: Ext) -> Ext {
    lo + (hi - lo) * cast::<Ext, _>(r.range(0, 1_000_000_000)) / Ext::from_f64(1e9)
}

fn f32r(r: &mut Rand, lo: Ext, hi: Ext) -> f32 {
    cast::<f32, _>(f64r(r, lo, hi))
}

const COMMODS: [&str; 8] = ["OIL", "GOLD", "SILVER", "WHEAT", "CORN", "STOCKS", "BITCOIN", "ETHEREUM"];

/// Records every report, message box and news headline.
#[derive(Default)]
struct Rec {
    reports: Vec<(PbStr, Vec<PbStr>)>,
    msgs: Vec<String>,
    news: Vec<String>,
}

impl Ui for Rec {
    fn msgbox(&mut self, text: &PbStr, style: i32, _title: &PbStr) -> i32 {
        self.msgs.push(text.to_display());
        if style & 0xf == 3 || style & 0xf == 4 { 6 } else { 1 }
    }
    fn inputbox(&mut self, _prompt: &PbStr, _title: &PbStr, default: &PbStr) -> PbStr {
        default.clone()
    }
    fn report(&mut self, title: &PbStr, lines: &[PbStr], _yes_no: bool) -> ReportAction {
        self.reports.push((title.clone(), lines.to_vec()));
        ReportAction::Ok
    }
    fn news(&mut self, headline: &PbStr) {
        self.news.push(headline.to_display());
    }
}

/// Report dialogs of the original as (title, listbox 301 items).
fn orig_reports(o: &Original) -> Vec<(PbStr, Vec<PbStr>)> {
    o.dialogs.iter().filter_map(|d| d.control(301).map(|c| (d.title.clone(), c.items.clone()))).collect()
}

/// The port's reports as the original's listbox shows them: ShowReportDialog adds the `@END` line as " ".
fn port_reports(ui: &Rec) -> Vec<(PbStr, Vec<PbStr>)> {
    ui.reports
        .iter()
        .map(|(t, l)| {
            let mut l = l.clone();
            l.push(s(" "));
            (t.clone(), l)
        })
        .collect()
}

#[track_caller]
fn assert_reports(port: &[(PbStr, Vec<PbStr>)], orig: &[(PbStr, Vec<PbStr>)]) {
    let __content = wsr_core::content::current();
    assert_eq!(port.len(), orig.len(), "number of reports (port vs original)");
    for ((pt, pl), (ot, ol)) in port.iter().zip(orig) {
        assert_eq!(pt, ot, "report title");
        for (k, (a, b)) in pl.iter().zip(ol).enumerate() {
            assert_eq!(a, b, "{__wsr_text_13605}{:?}{__wsr_text_13606}{}", pt.to_display(), k + 1,__wsr_text_13605=__content.string(13605), __wsr_text_13606=__content.string(13606));
        }
        assert_eq!(pl.len(), ol.len(), "{__wsr_text_13605}{:?}: number of lines", pt.to_display(),__wsr_text_13605=__content.string(13605));
    }
}

/// MSGBOXes and ShowQuickMessage popups (qm.exe) of the original.
fn orig_msgs(o: &Original) -> usize {
    o.log.iter().filter(|l| l.starts_with("MSGBOX") || l.contains("CreateProcessA(0x0, 'qm.exe")).count()
}

/// UI-only state the ports don't model.
fn cmp() -> Compare {
    // abs_tol: EXT results below the f64 range (denormals) read back as 0 from the 80-bit state
    Compare::default().rel_tol(Ext::from_f64(1e-12)).abs_tol(Ext::from_f64(1e-300)).ignore("TickerPauseUntil").ignore("hReportDlg")
}

fn run(addr: u32, args: &[String], g: &Globals, rng: u32, skips: &[(u32, u32)]) -> Option<Original> {
    let opts = Opts { args: args.to_vec(), rng: Some(rng), skips: skips.to_vec(), ..Opts::default() };
    run_original_with(addr, &opts, g)
}

fn longs(a: &[i32]) -> Vec<String> {
    a.iter().map(|x| x.to_string()).collect()
}

// ------------------------------------------------------------------------------------------- states

/// A plausible mid-game world (players 1..NumPlayers, banks 11..85, insurers 86..130, operating corps
/// 131..1580, funds 1581..1600) with balance sheets, earnings history, industry data, holdings, the
/// economy, price histories, news, lawsuits, futures / commodities / options, and fund data.
fn world(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut r = Rand(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut g = Globals::default();
    g.world_generated = 1;
    g.game_end_flag = s("N");
    g.num_players = cast::<i32, _>(r.range(1, 4));
    g.difficulty_level = cast::<i32, _>(r.range(1, 4));
    g.cur_year = cast::<i32, _>(r.range(2000, 2030));
    g.start_year = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { g.cur_year } else { g.cur_year - cast::<i32, _>(r.range(1, 10)) };
    g.cur_quarter = cast::<i32, _>(r.range(1, 4));
    g.sim_co_count = [300, 450][cast::<usize, _>(r.range(0, 1))];
    g.base_interest_rate = cast::<f32, _>([f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(16.0)), 8.0f32, 7.0f32, 9.0f32][cast::<usize, _>(r.range(0, 3))]);
    g.prime_rate = cast::<f32, _>(g.base_interest_rate + f32r(&mut r, Ext::from_f64(0.0), Ext::from_f64(3.0)));
    g.long_bond_yield = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(12.0)));
    g.short_bond_yield = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(10.0)));
    g.sofr_rate = cast::<f32, _>(g.base_interest_rate + f32r(&mut r, -Ext::from_f64(0.5), Ext::from_f64(0.5)));
    g.gdp_growth = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(25.0), Ext::from_f64(11.0)));
    g.gdp_growth_target = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(15.0), Ext::from_f64(8.0)));
    g.monetary_policy = cast::<i16, _>(r.range(0, 2));
    g.monetary_stance = cast::<i32, _>(r.range(0, 2));
    g.exchange_rate = cast::<f64, _>(if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { Ext::from_f64(1.0) } else { f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(2.0)) });
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
    g.fad_industry = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { cast::<i32, _>(r.range(3, 69)) } else { 0 };
    g.fad_valuation_pct = if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) { cast::<i32, _>(r.range(50, 200)) } else { 0 };
    g.hot_stock_corp = cast::<i16, _>(r.range(131, 1580));
    g.last_fraud_corp = cast::<i32, _>(r.range(131, 1580));
    g.million_prefix_uc = s(["M", "B", "M"][cast::<usize, _>(r.range(0, 2))]);
    g.million_prefix_lc = s(["m", "b", "m"][cast::<usize, _>(r.range(0, 2))]);
    g.currency_symbol = s(["$", "$", " EUR ", "Y"][cast::<usize, _>(r.range(0, 3))]);
    g.currency_suffix = s(["", "", " (US)"][cast::<usize, _>(r.range(0, 2))]);
    g.currency_code = s(["USA", "UK", "EUR", "JPN"][cast::<usize, _>(r.range(0, 3))]);
    g.central_bank_name = s(["FEDERAL RESERVE", "bank of england", "ECB"][cast::<usize, _>(r.range(0, 2))]);
    g.rand_seed = cast::<i32, _>(r.range(0, 32600));
    g.game_in_progress = if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) { 1 } else { 0 };
    g.active_corp_count = cast::<i32, _>(r.range(700, 1300));
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    g.subprime_crisis_state = cast::<i16, _>(r.range(0, 2));
    g.carbon_tax_rate = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { cast::<i32, _>(r.range(1, 30)) } else { 0 };
    g.carbon_tax_qtr = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { Ext::from_f64(0.0) } else { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(50.0)) };
    g.health_care_tax_rate = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { cast::<i32, _>(r.range(1, 10)) } else { 0 };
    g.health_care_tax_qtr = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { Ext::from_f64(0.0) } else { Ext::from_f64(3.0) };
    g.holding_co_tax_rate = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { cast::<i32, _>(r.range(1, 10)) } else { 0 };
    g.oil_windfall_tax_rate = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { cast::<i32, _>(r.range(1, 60)) } else { 0 };
    g.corp_shares_tax_country = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { cast::<i32, _>(r.range(1, 20)) } else { 0 };
    g.new_mgmt_list = s(&(0..r.range(0, 6)).map(|_| format!("{:04}", r.range(11, 1600))).collect::<String>());
    for p in 1..=5 {
        g.player_name[p] = if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) { s("...OUT...") } else { s(&format!("{__wsr_text_13556}{p}",__wsr_text_13556=__content.string(13556))) };
        g.player_ceo_of[p] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { cast::<i32, _>(r.range(11, 1600)) } else { 0 };
    }
    g.cur_player_name = g.player_name[g.cur_player].clone();
    for c in 0..=55 {
        g.country_name[c] = s(&format!("{__wsr_text_14012}{c}",__wsr_text_14012=__content.string(14012)));
    }
    for ind in 0..=71 {
        g.ind_name[ind] = s(["OIL & GAS", "Banking", "AUTOMOBILES AND TRUCKS", "Retail", "x"][cast::<usize, _>(ind % 5)]);
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
        let active = if e <= 10 { e <= g.num_players } else { r.chance(cast::<f64, _>(Ext::from_f64(0.93))) };
        g.entity_status[e] = if active { 1 } else if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { 0 } else { 2 };
        g.ent_name[e] = s(&format!("{__wsr_text_13758}{e}{}", if r.chance(Ext::from_f64(0.3)) { __content.string(14013) } else { "" },__wsr_text_13758=__content.string(13758)));
        g.ent_symbol[e] = s(&format!("S{e}"));
        g.cash[e] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(3000.0));
        g.t_bills[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(1000.0)) } else { Ext::from_f64(0.0) };
        g.bank_debt[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(2000.0)) } else { Ext::from_f64(0.0) };
        g.lending_bank[e] = cast::<i16, _>(r.range(11, 50));
        g.credit_rating_num[e] = cast::<i32, _>(r.range(0, 10));
        g.credit_frozen[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { 1 } else { 0 };
        g.auto_pilot[e] = cast::<i32, _>(r.range(0, 1));
        g.corp_controller[e] = match r.range(0, 5) {
            0 => 0,
            1 | 2 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
            _ => cast::<i32, _>(r.range(11, 1600)),
        };
        let mut n = 0;
        for slot in 1..=15 {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
                n = slot;
                g.hold_stock_id[(slot, e)] = cast::<i16, _>(r.range(11, 1600));
                g.hold_stock_pct[(slot, e)] = cast::<i16, _>(r.range(1, 100));
            }
        }
        g.hold_stock_id[(0, e)] = cast::<i16, _>(n);
        if e > 10 {
            g.industry_num[e] = match e {
                11..=85 => 1,
                86..=130 => 2,
                1581..=1600 => 71,
                _ => {
                    if r.chance(cast::<f64, _>(Ext::from_f64(0.06))) {
                        [69, 70, 37][cast::<usize, _>(r.range(0, 2))]
                    } else {
                        cast::<i16, _>(r.range(3, 40))
                    }
                }
            };
            g.market_cap[e] = f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(300.0));
            g.shares_outstanding[e] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(50.0));
            if r.chance(cast::<f64, _>(Ext::from_f64(0.03))) && e < 1581 {
                g.shares_outstanding[e] = Ext::from_f64(0.0);
            }
            g.dividend_per_share[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(3.0)) } else { Ext::from_f64(0.0) };
            g.tax_loss_carry[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { f64r(&mut r, -Ext::from_f64(1000.0), Ext::from_f64(0.0)) } else { Ext::from_f64(0.0) };
            g.accrued_pl[e] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(200.0));
            g.consol_tax_inc_ytd[e] = f64r(&mut r, -Ext::from_f64(30.0), Ext::from_f64(100.0));
            g.tax_credits[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(900.0)) } else { Ext::from_f64(0.0) };
            g.contingency_reserve[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(50.0)) } else { Ext::from_f64(0.0) };
            let zero = |r: &mut Rand| r.chance(cast::<f64, _>(Ext::from_f64(0.08)));
            g.earnings_y1[e] = if zero(&mut r) { Ext::from_f64(0.0) } else { f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(100.0)) };
            g.earnings_yr2_ago[e] = if zero(&mut r) { Ext::from_f64(0.0) } else { f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(100.0)) };
            g.earnings_yr3_ago[e] = if zero(&mut r) { Ext::from_f64(0.0) } else { f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(100.0)) };
            g.earnings_yr4_ago[e] = if zero(&mut r) { Ext::from_f64(0.0) } else { f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(100.0)) };
            g.earnings_ytd[e] = f64r(&mut r, -Ext::from_f64(2000.0), Ext::from_f64(6000.0));
            g.proj_qtr_earnings[e] = if zero(&mut r) { Ext::from_f64(0.0) } else { f64r(&mut r, -Ext::from_f64(10.0), Ext::from_f64(30.0)) };
            g.extraordinary_pl[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { r.range(-200, 200) } else { 0 };
            g.extra_items_ytd[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { f64r(&mut r, -Ext::from_f64(200.0), Ext::from_f64(200.0)) } else { Ext::from_f64(0.0) };
            g.corp_prior_net_worth[e] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(3000.0));
            g.profit_margin[e] = cast::<i16, _>(r.range(-25, 40));
            g.growth_rate[e] = cast::<i16, _>(r.range(-10, 60));
            g.disc_spend_pct[e] = cast::<i16, _>(r.range(0, 15));
            g.investor_sentiment[e] = cast::<i32, _>(r.range(-60, 70));
            g.country_idx[e] = cast::<i32, _>(r.range(0, 25));
            g.ent_misc_data[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) {
                let n = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { 40 } else { 12 };
                let fields: Vec<String> = (1..=n)
                    .map(|k| match k {
                        1 | 3 => format!("{:.2}", f64r(&mut r, Ext::from(-Ext::from_f64(10.0)), Ext::from(Ext::from_f64(40.0)))),
                        12 => format!("{:.3}", f64r(&mut r, Ext::from(Ext::from_f64(0.0)), Ext::from(Ext::from_f64(5.0)))),
                        18 => format!("{}", r.range(-5, 5)),
                        22 => format!("{}", r.range(0, 200)),
                        29 => r.range(0, 25).to_string(),
                        38 => format!("{}", r.range(-300, 300)),
                        _ => r.range(0, 99).to_string(),
                    })
                    .collect();
                s(&fields.join("|"))
            } else {
                s(&format!("{:.2}", f64r(&mut r, Ext::from(-Ext::from_f64(5.0)), Ext::from(Ext::from_f64(20.0)))))
            };
            if e >= g.sim_co_count && r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                g.bonds_issued[e] = r.range(1, 1500);
                let mat = g.cur_year + cast::<i32, _>(r.range(0, 9));
                let coupon = f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(15.0));
                // BondIssueInfo: "<maturity>@<coupon>" or "<maturity>cv|<conversion price>|@<coupon>"
                g.bond_issue_info[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                    s(&format!("{mat}{__wsr_text_13555}{:.2}|@{coupon:5.2}", f64r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(90.0))),__wsr_text_13555=__content.string(13555)))
                } else {
                    s(&format!("{mat}@{coupon:5.2}"))
                };
                g.bond_price[e] = Cur::from_f64(f64r(&mut r, Ext::from_f64(20.0), Ext::from_f64(180.0)));
            }
        }
        if (131..=1600).contains(&e) {
            g.business_assets[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5000.0));
            g.working_capital[e] = f64r(&mut r, -Ext::from_f64(100.0), Ext::from_f64(1000.0));
            g.goodwill[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(3000.0)) } else { Ext::from_f64(0.0) };
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
            g.subprime_secs[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) { r.range(0, 20000) } else { 0 };
        }
        if (86..=130).contains(&e) {
            g.ins_reserves[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(10000.0));
            g.insur_uw_profit_ytd[e] = f64r(&mut r, -Ext::from_f64(500.0), Ext::from_f64(300.0));
        }
        if (1581..=1600).contains(&e) {
            g.fund_fee_pct[e] = cast::<f32, _>([Ext::from_f64(0.1), Ext::from_f64(0.2), Ext::from_f64(0.5), Ext::from_f64(1.0), Ext::from_f64(1.5), Ext::from_f64(0.3)][cast::<usize, _>(r.range(0, 5))]);
            g.fund_adviser[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) { cast::<i32, _>(r.range(86, 1580)) } else { 0 };
            g.fund_bonus_year[e] = cast::<i32, _>(r.range(0, 1)) * g.cur_year;
            g.ctrl_since_year[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { 0 } else { g.cur_year - 2 };
        }
    }
    g.covered_call_fund_id = cast::<i32, _>(r.range(1581, 1600));
    // price histories
    for series in 0..=1606 {
        if r.chance(cast::<f64, _>(Ext::from_f64(0.05))) {
            continue;
        }
        let cur = cast::<i32, _>(r.range(1, 60));
        g.cap_hist_close[(0, series)] = cast::<Ext, _>(cur);
        let months = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { cast::<i32, _>(r.range(1, 30)) } else { 60 };
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
            if !r.chance(cast::<f64, _>(Ext::from_f64(0.05))) {
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

/// Situations the report comments on, centred on `corp`: news, lawsuits, antitrust, merger, special
/// liabilities, asset sale offers, futures / commodity / option / swap records.
fn decorate(g: &mut Globals, r: &mut Rand, corp: i32, own: Own) {
    let __content = wsr_core::content::current();
    let sym = g.ent_symbol[corp].to_display();
    let headlines = [
        format!("{sym} reports earnings of $5"),
        format!("Rumors about {sym}{__wsr_text_14014}",__wsr_text_14014=__content.string(14014)),
        format!("Analysts upgrade Entity {corp} ({sym})"),
        format!("Big deal for {sym}."),
        __content.string(14015).to_string(),
    ];
    for i in 1..=60 {
        g.news_headline[i] = if r.chance(cast::<f64, _>(Ext::from_f64(0.15))) { s(&headlines[cast::<usize, _>(r.range(0, 4))]) } else { PbStr::new() };
    }
    let other = cast::<i32, _>(r.range(131, 1580));
    for i in 1..=10 {
        g.lawsuit_rec[i] = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
            let (a, b) = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { (corp, other) } else { (other, corp) };
            s(&format!("{}|{a}|{b}{__wsr_text_13978}{}", r.range(1, 2), r.range(0, 500),__wsr_text_13978=__content.string(13978)))
        } else {
            PbStr::new()
        };
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        g.antitrust_plaintiff = corp;
        g.antitrust_defendant = other;
    } else if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        g.antitrust_plaintiff = other;
        g.antitrust_defendant = corp;
    }
    g.antitrust_damages = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { r.range(1, 900) } else { 0 };
    if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
        g.merger_acquirer = corp;
        g.merger_target = other;
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
        g.asbestos_corp = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { corp } else { -corp };
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
        g.strike_corp_code = cast::<i16, _>(corp + 10000 * cast::<i32, _>(r.range(0, 2)));
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
        g.superfund_corp = cast::<i16, _>(corp);
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
        g.malicious_suit_defendant = corp;
        g.malicious_suit_plaintiff = other;
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        g.scenario_corp = cast::<i16, _>(corp);
        g.scenario_num = [101, -102, 50, 7][cast::<usize, _>(r.range(0, 3))];
        g.scenario_stage = cast::<i16, _>(r.range(0, 7));
    }
    let mut n = 0;
    for i in 1..=r.range(0, 4) {
        n = i;
        let target = r.range(131, 1580);
        let kind = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { "CA".to_string() } else { target.to_string() };
        let seller = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { corp } else { other };
        g.asset_sale_offers[cast::<i32, _>(i)] = s(&format!("{seller}|{kind}|{}", r.range(5, 90)));
    }
    g.asset_sale_offers[0] = s(&n.to_string());
    // [runtime resource wording] ies
    let n = cast::<i32, _>(r.range(0, 12));
    for i in 1..=n {
        let c = COMMODS[if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { 0 } else { cast::<usize, _>(r.range(0, 7)) }];
        let owner = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { corp } else { other };
        let qty = r.range(1, 60) * if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) { -1 } else { 1 };
        let year = g.cur_year + cast::<i32, _>(r.range(-1, 3));
        let cost = f64r(r, Ext::from_f64(1.0), Ext::from_f64(2000.0));
        g.futures_recs[i] = s(&format!("{c}|{owner}|{qty}|{}|{year}|{cost:.4}|{:.2}", r.range(1, 12), f64r(r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(999.0)))));
    }
    g.futures_recs[0] = s(&n.to_string());
    let n = cast::<i32, _>(r.range(0, 8));
    for i in 1..=n {
        let c = ["GOLD", "SILVER", "OIL", "BITCOIN", "ETHEREUM", "GOLD"][cast::<usize, _>(r.range(0, 5))];
        let owner = if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) { corp } else { other };
        g.phys_commod_rec[i] = s(&format!("{c}|{owner}|{}|{:.2}", r.range(1, 2000), f64r(r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(900.0)))));
    }
    g.phys_commod_rec[0] = s(&n.to_string());
    let n = cast::<i32, _>(r.range(0, 6));
    for i in 1..=n {
        let holder = [1, g.cur_player, other, corp][cast::<usize, _>(r.range(0, 3))];
        let under = if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) { corp } else { other };
        let qty = r.range(1, 25) * if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { -1 } else { 1 };
        let strike = (g.market_cap[under] * f64r(r, Ext::from_f64(0.3), Ext::from_f64(2.6)) * Ext::from_f64(100.0)).round() / Ext::from_f64(100.0);
        g.option_recs[i] = s(&format!(
            "{under}| {}| {holder}| {qty}| {strike}| {}| {}| {:.2}",
            r.range(1, 2),
            [1, 4, 7, 10][cast::<usize, _>(r.range(0, 3))],
            g.cur_year + cast::<i32, _>(r.range(0, 2)),
            f64r(r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(50.0)))
        ));
    }
    g.option_recs[0] = s(&n.to_string());
    let n = cast::<i32, _>(r.range(0, 5));
    for i in 1..=n {
        let (a, b) = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { (corp, other) } else { (other, corp) };
        let ty = ["P", "L", "S", "B"][cast::<usize, _>(r.range(0, 3))];
        g.swap_contracts[i] = s(&format!(
            "{a}|{b}|{:.2}|{ty}|{}.{}|{}.{}|{}",
            f64r(r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(9.0))),
            g.cur_year - 1,
            r.range(1, 4),
            g.cur_year + cast::<i32, _>(r.range(1, 8)),
            r.range(1, 4),
            r.range(1, 5000)
        ));
    }
    g.swap_contracts[0] = s(&n.to_string());
    // the corp's own situation
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        g.cash[corp] = -f64r(r, Ext::from_f64(10.0), Ext::from_f64(500.0));
    }
    // earnings history patterns (EPS of a few currency units)
    let mut plain = false;
    if corp > 10 {
        let base = f64r(r, Ext::from_f64(50.0), Ext::from_f64(3000.0));
        let (y1, y2, y3, y4) = match r.range(0, 5) {
            0 => (base * Ext::from_f64(2.0), base * Ext::from_f64(1.5), base, base * f64r(r, Ext::from_f64(0.5), Ext::from_f64(1.2))),
            1 => (base * Ext::from_f64(0.5), base * Ext::from_f64(0.8), base, base * f64r(r, Ext::from_f64(0.9), Ext::from_f64(1.5))),
            2 => (-base, -base * Ext::from_f64(0.5), if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { -base } else { base }, -base),
            3 => (Ext::from_f64(0.0), Ext::from_f64(0.0), Ext::from_f64(0.0), Ext::from_f64(0.0)),
            _ => (g.earnings_y1[corp], g.earnings_yr2_ago[corp], g.earnings_yr3_ago[corp], g.earnings_yr4_ago[corp]),
        };
        g.earnings_y1[corp] = y1;
        g.earnings_yr2_ago[corp] = y2;
        g.earnings_yr3_ago[corp] = y3;
        g.earnings_yr4_ago[corp] = y4;
        g.earnings_ytd[corp] = match r.range(0, 3) {
            0 => Ext::from_f64(0.0),
            1 => y1 * f64r(r, Ext::from_f64(0.0), Ext::from_f64(1.2)),
            _ => f64r(r, -Ext::from_f64(3000.0), Ext::from_f64(8000.0)),
        };
        g.proj_qtr_earnings[corp] = f64r(r, -Ext::from_f64(300.0), Ext::from_f64(900.0));
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.shares_outstanding[corp] = f64r(r, Ext::from_f64(0.5), Ext::from_f64(5.0));
        }
        // a large subsidiary
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            let sub = cast::<i32, _>(r.range(131, 1580));
            g.hold_stock_id[(1, corp)] = cast::<i16, _>(sub);
            g.hold_stock_pct[(1, corp)] = cast::<i16, _>(r.range(20, 100));
            g.market_cap[sub] = f64r(r, Ext::from_f64(200.0), Ext::from_f64(5000.0));
            g.earnings_y1[sub] = f64r(r, -Ext::from_f64(900.0), Ext::from_f64(900.0));
        }
        // weak or strong balance sheet
        let sheet = r.range(0, 3);
        plain = sheet == 2 && corp > 130 && corp < 1581;
        match sheet {
            0 => {
                g.bank_debt[corp] = f64r(r, Ext::from_f64(3000.0), Ext::from_f64(30000.0));
                g.cash[corp] = f64r(r, -Ext::from_f64(100.0), Ext::from_f64(20.0));
                g.t_bills[corp] = Ext::from_f64(0.0);
            }
            1 => {
                g.bank_debt[corp] = Ext::from_f64(0.0);
                g.cash[corp] = f64r(r, Ext::from_f64(1000.0), Ext::from_f64(9000.0));
            }
            2 if corp > 130 && corp < 1581 => {
                // a plain balance sheet with a debt ratio across the rating scale
                g.business_assets[corp] = Ext::from_f64(1000.0);
                g.working_capital[corp] = Ext::from_f64(0.0);
                g.goodwill[corp] = Ext::from_f64(0.0);
                g.cash[corp] = f64r(r, Ext::from_f64(0.0), Ext::from_f64(60.0));
                g.t_bills[corp] = Ext::from_f64(0.0);
                g.bonds_issued[corp] = 0;
                for slot in 1..=15 {
                    g.hold_stock_id[(slot, corp)] = 0;
                }
                g.hold_stock_id[(0, corp)] = 0;
                g.bank_debt[corp] = Ext::from_f64(1050.0) * f64r(r, Ext::from_f64(0.15), Ext::from_f64(0.97));
            }
            _ => {}
        }
        if (131..=1600).contains(&corp) && !plain && r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.goodwill[corp] = f64r(r, Ext::from_f64(1000.0), Ext::from_f64(20000.0));
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.dividend_per_share[corp] = f64r(r, Ext::from_f64(1.0), Ext::from_f64(20.0));
        }
        g.credit_rating_num[corp] = cast::<i32, _>(r.range(0, 10));
        g.growth_rate[corp] = cast::<i16, _>([r.range(-10, 60), 0, -8, 35][cast::<usize, _>(r.range(0, 3))]);
        g.profit_margin[corp] = cast::<i16, _>(r.range(-25, 40));
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
        g.player_ceo_of[g.cur_player] = corp;
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
        g.new_mgmt_list = g.new_mgmt_list.clone() + format!("{corp:04}").as_str();
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        // a player short the stock
        let p = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
        g.hold_stock_id[(15, p)] = cast::<i16, _>(corp);
        g.hold_stock_pct[(15, p)] = -(cast::<i16, _>(r.range(1, 30)));
        g.hold_stock_id[(0, p)] = 15;
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        g.swap_pn_l_qtr[corp] = r.range(-500, 500);
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) && corp >= g.sim_co_count {
        let mat = g.cur_year + cast::<i32, _>(r.range(0, 3));
        g.bonds_issued[corp] = r.range(1, 3000);
        g.bond_issue_info[corp] = if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
            s(&format!("{mat}{__wsr_text_13555}{:.2}|@{:5.2}", f64r(r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(90.0))), f64r(r, Ext::from(Ext::from_f64(2.0)), Ext::from(Ext::from_f64(12.0))),__wsr_text_13555=__content.string(13555)))
        } else {
            s(&format!("{mat}@{:5.2}", f64r(r, Ext::from(Ext::from_f64(2.0)), Ext::from(Ext::from_f64(12.0)))))
        };
        g.bond_price[corp] = Cur::from_f64(f64r(r, Ext::from_f64(20.0), Ext::from_f64(200.0)));
    }
    // advised funds (insurers and investment companies) / fund advisers controlled by a player
    if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
        for fund in 1581..=1600 {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                g.fund_adviser[fund] = corp;
            }
        }
    }
    if (1581..=1600).contains(&corp) {
        g.entity_status[corp] = 1;
        g.industry_num[corp] = 71;
        let adv = cast::<i32, _>([r.range(86, 130), r.range(131, 1580)][cast::<usize, _>(r.range(0, 1))]);
        g.industry_num[adv] = if adv <= 130 { 2 } else { 37 };
        g.entity_status[adv] = 1;
        g.corp_controller[adv] = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { g.cur_player } else { cast::<i32, _>(r.range(1, 1600)) };
        g.fund_adviser[corp] = adv;
        g.fund_fee_pct[corp] = cast::<f32, _>([Ext::from_f64(0.2), Ext::from_f64(0.2), Ext::from_f64(0.5), Ext::from_f64(1.0)][cast::<usize, _>(r.range(0, 3))]);
        g.fund_bonus_year[corp] = if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) { g.cur_year } else { 0 };
        g.extraordinary_pl[corp] = r.range(-900, 900);
        g.covered_call_fund_id = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { corp } else { g.covered_call_fund_id };
    }
    if g.industry_num[corp.clamp(11, 1600)] == 1 && corp > 10 {
        // borrowers of the bank, some in trouble
        for _ in 0..r.range(0, 12) {
            let b = cast::<i32, _>(r.range(131, 1580));
            g.lending_bank[b] = cast::<i16, _>(corp);
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                g.bank_debt[b] = f64r(r, Ext::from_f64(2000.0), Ext::from_f64(40000.0));
                g.cash[b] = -f64r(r, Ext::from_f64(0.0), Ext::from_f64(500.0));
                g.business_assets[b] = f64r(r, Ext::from_f64(0.0), Ext::from_f64(500.0));
            }
        }
        g.bad_debt_reserve[corp] = [f64r(r, -Ext::from_f64(5.0), Ext::from_f64(300.0)), f64r(r, Ext::from_f64(1000.0), Ext::from_f64(9000.0))][cast::<usize, _>(r.range(0, 1))];
    }
    if (11..=130).contains(&corp) && r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
        // government bonds carried at cost (unrealized gain / loss)
        let face = r.range(1000, 50000);
        g.long_gov_bond_face[corp] = face;
        g.lt_gov_bond_cost[corp] = cast::<Ext, _>(face) * cast::<Ext, _>(g.long_bond_price) * Ext::from_f64(0.01) * f64r(r, Ext::from_f64(0.6), Ext::from_f64(1.4));
    }
    if (11..=85).contains(&corp) && r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        // a small deposit base (loan-to-deposit regulation)
        g.bank_demand_deposits[corp] = f64r(r, Ext::from_f64(0.0), Ext::from_f64(500.0));
        g.bank_c_ds[corp] = f64r(r, Ext::from_f64(0.0), Ext::from_f64(500.0));
        g.bank_cust_deposits[corp] = f64r(r, Ext::from_f64(0.0), Ext::from_f64(500.0));
        g.credit_frozen[corp] = 1;
    }
    if corp > 130 && corp < 1581 && r.chance(cast::<f64, _>(Ext::from_f64(0.25))) {
        g.industry_num[corp] = [37, 4, 21, 70][cast::<usize, _>(r.range(0, 3))];
    }
    if corp > 130 && corp < 1581 && !plain && r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
        g.business_assets[corp] = f64r(r, Ext::from_f64(50000.0), Ext::from_f64(200000.0)); // the dominant company
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        g.market_cap[corp.clamp(11, 1600)] = f64r(r, Ext::from_f64(1000.0), Ext::from_f64(100000.0));
    }
    if plain {
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            g.goodwill[corp] = f64r(r, Ext::from_f64(100.0), Ext::from_f64(900.0));
        }
        for i in 1..=1000 {
            g.phys_commod_rec[i] = PbStr::new();
        }
        g.phys_commod_rec[0] = s("0");
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        // written options of the corp (naked liabilities)
        for i in 1..=3 {
            let under = cast::<i32, _>(r.range(131, 1580));
            g.option_recs[i] = s(&format!(
                "{under}| {}| {corp}| {}| {:.2}| 4| {}| -50",
                r.range(1, 2),
                -r.range(5, 200),
                g.market_cap[under] * f64r(r, Ext::from(Ext::from_f64(0.2)), Ext::from(Ext::from_f64(1.0))),
                g.cur_year + 1
            ));
        }
        g.option_recs[0] = s("3");
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
        // a large swap book
        let (a, b) = (corp, cast::<i32, _>(r.range(131, 1580)));
        g.swap_contracts[1] = s(&format!("{a}|{b}|{:.2}|P|{}.1|{}.2|{}", f64r(r, Ext::from(Ext::from_f64(0.5)), Ext::from(Ext::from_f64(15.0))), g.cur_year - 1, g.cur_year + 5, r.range(5000, 90000)));
        g.swap_contracts[0] = s("1");
    }
    // price history of the corp: a noisy rise (appreciation without a streak), flat with the cap near a
    // 52-week extreme, or a steady trend
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        let cur = cast::<i32, _>(g.cap_hist_close[(0, corp)]);
        if cur > 0 {
            let flat = r.chance(cast::<f64, _>(Ext::from_f64(0.5)));
            let mut v = g.market_cap[corp];
            for k in 0..60 {
                let mut slot = cur - k;
                if slot < 1 {
                    slot += 60;
                }
                g.cap_hist_close[(slot, corp)] = v;
                g.price_hist_high[(slot, corp)] = v * Ext::from_f64(1.01);
                g.price_hist_low[(slot, corp)] = v * Ext::from_f64(0.99);
                let step = if flat { [Ext::from_f64(1.03), Ext::from_f64(0.97)][cast::<usize, _>(k) % 2] } else { [Ext::from_f64(1.06), Ext::from_f64(0.99)][cast::<usize, _>(k) % 2] };
                v /= step;
            }
            if flat {
                g.market_cap[corp] *= [Ext::from_f64(1.3), Ext::from_f64(0.7), Ext::from_f64(1.0)][cast::<usize, _>(r.range(0, 2))];
            }
        }
    } else if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
        let cur = cast::<i32, _>(g.cap_hist_close[(0, corp)]);
        if cur > 0 {
            let step = [Ext::from_f64(0.93), Ext::from_f64(0.97), Ext::from_f64(1.0), Ext::from_f64(1.04), Ext::from_f64(1.08)][cast::<usize, _>(r.range(0, 4))];
            let mut v = g.market_cap[corp];
            for k in 0..60 {
                let mut slot = cur - k;
                if slot < 1 {
                    slot += 60;
                }
                g.cap_hist_close[(slot, corp)] = v;
                g.price_hist_high[(slot, corp)] = v * Ext::from_f64(1.02);
                g.price_hist_low[(slot, corp)] = v * Ext::from_f64(0.98);
                v /= step;
            }
        }
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        g.public_pct = 0;
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        g.wholly_owned_by = cast::<i16, _>(other);
    }
    g.largest_holder = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { cast::<i16, _>(other) } else { 0 };
    if corp > 85 && corp < 1581 {
        // a valid share count (else CalcStockPrice values the corp in mode 3, see the module docs)
        if g.shares_outstanding[corp] == Ext::from_f64(0.0) {
            g.shares_outstanding[corp] = Ext::from_f64(3.0);
        }
    }
    match own {
        Own::Private => {
            // one holder owns all of it (see the module docs)
            let owner = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { other } else { cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))) };
            let slot = cast::<i32, _>(r.range(1, 15));
            g.hold_stock_id[(slot, owner)] = cast::<i16, _>(corp);
            g.hold_stock_pct[(slot, owner)] = 100;
            if g.hold_stock_id[(0, owner)] < cast::<i16, _>(slot) {
                g.hold_stock_id[(0, owner)] = cast::<i16, _>(slot);
            }
        }
        Own::Fund => {
            // a fund stays one (industry 71) only while no holder has more than 15%
            for e in 1..=1600 {
                for slot in 1..=15 {
                    if cast::<i32, _>(g.hold_stock_id[(slot, e)]) == corp && g.hold_stock_pct[(slot, e)] > 12 {
                        g.hold_stock_pct[(slot, e)] = 12;
                    }
                }
            }
            g.corp_controller[corp] = corp;
        }
        Own::Any => {}
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Own {
    Any,
    Private,
    Fund,
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
fn etf_description_table() {
    let __content = wsr_core::content::current();
    assert_eq!(get_etf_description(1581), __content.string(8856));
    assert_eq!(get_etf_description(1600), __content.string(8875));
    assert_eq!(get_etf_description(1580), "");
    assert_eq!(get_etf_description(1601), "");
}

#[test]
fn etf_largest_holding_slot_picks_largest_value() {
    let mut g = Globals::default();
    g.entity_status[1590] = 1;
    g.hold_stock_id[(2, 1590)] = 200;
    g.hold_stock_pct[(2, 1590)] = 10;
    g.market_cap[200] = Ext::from_f64(50.0); // 500
    g.hold_stock_id[(5, 1590)] = 300;
    g.hold_stock_pct[(5, 1590)] = 30;
    g.market_cap[300] = Ext::from_f64(20.0); // 600
    g.hold_stock_id[(7, 1590)] = 9; // a player id: ignored
    g.hold_stock_pct[(7, 1590)] = 99;
    assert_eq!(get_etf_largest_holding_slot(&g, 1590), 5);
    assert_eq!(get_etf_largest_holding_slot(&g, 1585), 0);
}

#[test]
fn players_and_defunct_entities_get_a_message() {
    let __content = wsr_core::content::current();
    let mut g = Globals::default();
    g.cur_player_name = s("Jane");
    g.entity_status[3] = 1;
    g.entity_status[500] = 2;
    g.ent_name[500] = s("Gone Corp");
    g.game_end_flag = s("Y");
    let mut ui = Rec::default();
    let mut rng = Rng::from_state(1);
    show_research_report(&mut g, &mut ui, &mut rng, 3);
    show_research_report(&mut g, &mut ui, &mut rng, 500);
    assert!(ui.reports.is_empty());
    assert_eq!(
        ui.msgs,
        [
            __content.string(14446),
            "Gone Corp has been liquidated, and no longer exists."
        ]
    );
}

#[test]
fn bank_regulator_restricts_overlent_bank() {
    let mut g = Globals::default();
    let mut ui = Scripted::default();
    g.entity_status[20] = 1;
    g.bank_demand_deposits[20] = Ext::from_f64(1000.0);
    g.bank_loans_total = Ext::from_f64(990.0);
    g.credit_rating_num[20] = 4;
    g.net_worth = Ext::from_f64(10.0);
    // 99% loans to deposits - 2 >= 95; capital 1% -> no deduction; 99 >= 94 and rating < 7
    assert_eq!(bank_regulator_lending_check(&mut g, &mut ui, 20, 0), 1);
    g.credit_rating_num[20] = 7;
    assert_eq!(bank_regulator_lending_check(&mut g, &mut ui, 20, 0), 0);
    assert_eq!(bank_regulator_lending_check(&mut g, &mut ui, 90, 0), 0);
}

// ------------------------------------------------------------------------------- differential tests

/// The small callees on many states.
#[test]
fn diff_small_callees() {
    par_seeds(1..=3u64, |seed| {
        let mut g = world(seed);
        pow_safe(&mut g, seed);
        let mut r = Rand(seed * 31);
        for fund in [1578, 1580, 1581, 1585, 1586, 1590, 1593, 1600, 1601] {
            let o = run(0x64a5fe, &longs(&[fund]), &g, 1, &[]).unwrap_or_else(|| panic!("harness"));
            assert_eq!(get_etf_largest_holding_slot(&g, fund), cast::<i32, _>(o.return_value), "GetEtfLargestHoldingSlot({fund})");
            let o = run(0x64a767, &longs(&[fund]), &g, 1, &[]).unwrap();
            // an empty string is a null BSTR (no return string)
            assert_eq!(get_etf_description(fund), o.return_str.unwrap_or_default(), "GetEtfDescription({fund})");
        }
        for bank in active_corps(&g, &mut r, 8, 11, 85) {
            let mut g = g.clone();
            g.bank_loans_total = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(30000.0));
            g.tmp_corp_bonds_value = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(3000.0));
            g.net_worth = f64r(&mut r, -Ext::from_f64(100.0), Ext::from_f64(2000.0));
            for post in [0, 1] {
                let o = run(0x6a117d, &longs(&[bank, post]), &g, 1, &[]).unwrap();
                let mut port = g.clone();
                let mut ui = Rec::default();
                let v = bank_regulator_lending_check(&mut port, &mut ui, bank, post);
                cmp().assert(&port, &o.globals);
                assert_eq!(v, cast::<i32, _>(o.return_value), "BankRegulatorLendingCheck({bank}, {post})");
            }
        }
    });
}

/// One research report: original vs port.
fn compare_report(g: &Globals, corp: i32, rng0: u32) -> Option<(usize, bool)> {
    let __content = wsr_core::content::current();
    let o = run(SHOW_RESEARCH_REPORT, &longs(&[corp]), g, rng0, &[])?;
    for l in o.log.iter().filter(|l| l.starts_with("PBERR")) {
        eprintln!("  {l}");
    }
    let mut port = g.clone();
    let mut ui = Rec::default();
    let mut rng = Rng::from_state(rng0);
    show_research_report(&mut port, &mut ui, &mut rng, corp);
    // error in ShowResearchReport itself (its ON ERROR handler shows ShowRuntimeError and exits)
    let aborted = o.log.iter().any(|l| l.starts_with("PBERR") && l.contains("in 0x448ed3"));
    let orig = orig_reports(&o);
    eprintln!(
        "corp {corp} (ind {}): {} report lines, aborted {aborted}, {:.1}{__wsr_text_13748}",
        g.industry_num[corp.clamp(11, 1600)],
        orig.first().map(|r| r.1.len()).unwrap_or(0),
        o.elapsed
    ,__wsr_text_13748=__content.string(13748));
    if let Some((_, l)) = orig.first() {
        for x in l {
            eprintln!("    | {}", x.to_display());
        }
    }
    assert_reports(&port_reports(&ui), &orig);
    assert_eq!(rng.state(), o.rng, "RND state");
    cmp().assert(&port, &o.globals);
    assert_eq!(ui.msgs.len(), orig_msgs(&o) - usize::from(aborted && orig_msgs(&o) > 0), "message boxes: {:?} / {:?}", ui.msgs, o.log);
    Some((orig.first().map(|r| r.1.len()).unwrap_or(0), aborted))
}

fn report_seeds(seeds: std::ops::RangeInclusive<u64>, own: Own, pick: impl Fn(&Globals, &mut Rand) -> Vec<i32>) {
    let mut n = 0;
    for seed in seeds {
        let mut g = world(seed);
        pow_safe(&mut g, seed);
        let mut r = Rand(seed * 7 + 3);
        for corp in pick(&g, &mut r) {
            let mut g = g.clone();
            decorate(&mut g, &mut r, corp, own);
            let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
            let Some(_) = compare_report(&g, corp, rng0) else { return };
            n += 1;
        }
    }
    assert!(n > 0);
}

#[test]
fn diff_research_report_corps() {
    report_seeds(1..=8, Own::Private, |g, r| active_corps(g, r, 3, 131, 1580));
}

#[test]
fn diff_research_report_banks() {
    report_seeds(31..=40, Own::Any, |g, r| active_corps(g, r, 3, 11, 85));
}

#[test]
fn diff_research_report_insurers() {
    report_seeds(41..=48, Own::Private, |g, r| active_corps(g, r, 2, 86, 130));
}

#[test]
fn diff_research_report_funds() {
    report_seeds(51..=60, Own::Fund, |g, r| {
        let mut v = active_corps(g, r, 2, 1586, 1600);
        v.push(1581 + cast::<i32, _>(r.range(0, 4)));
        v.push(1581 + cast::<i32, _>(r.range(0, 4)));
        v
    });
}

#[test]
fn diff_research_report_players_and_defunct() {
    report_seeds(14..=14, Own::Any, |g, r| {
        let mut v = vec![1, cast::<i32, _>(r.range(2, 10))];
        if let Some(d) = (11..=1600).find(|&e| g.entity_status[e] == 2) {
            v.push(d);
        }
        v
    });
}

#[test]
fn diff_research_report_public_corps() {
    report_seeds(21..=30, Own::Any, |g, r| {
        let mut v = active_corps(g, r, 2, 131, 1580);
        v.extend(active_corps(g, r, 1, 86, 130));
        v
    });
}



/// Mixed corps, ownership and game-end states over more seeds.
#[test]
fn diff_research_report_mixed() {
    par_seeds(61..=80u64, |seed| {
        let mut g = world(seed);
        pow_safe(&mut g, seed);
        let mut r = Rand(seed * 13 + 1);
        if r.chance(cast::<f64, _>(Ext::from_f64(0.25))) {
            g.game_end_flag = s("Y");
        }
        let corps = [active_corps(&g, &mut r, 1, 11, 85), active_corps(&g, &mut r, 1, 86, 1580), active_corps(&g, &mut r, 1, 1581, 1600)];
        for (k, corp) in corps.concat().into_iter().enumerate() {
            let own = match k {
                2 => Own::Fund,
                1 if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) => Own::Private,
                _ => Own::Any,
            };
            let mut g = g.clone();
            decorate(&mut g, &mut r, corp, own);
            let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
            if compare_report(&g, corp, rng0).is_none() {
                return;
            }
        }
    });
}

/// Out-of-range subscripts end the original in its ON ERROR handler (no report): an asset sale offer of
/// stock id 0 (`EntSymbol(0)`), a malicious-prosecution suit without plaintiff, a merger without target.
#[test]
fn diff_research_report_error_exits() {
    let mut aborted = 0;
    for seed in 91..=96u64 {
        let mut g = world(seed);
        pow_safe(&mut g, seed);
        let mut r = Rand(seed);
        let corp = active_corps(&g, &mut r, 1, 11, 1580)[0];
        decorate(&mut g, &mut r, corp, if corp > 85 { Own::Private } else { Own::Any });
        match seed % 3 {
            0 => {
                g.asset_sale_offers[1] = s(&format!("{corp}|0|25"));
                g.asset_sale_offers[0] = s("1");
            }
            1 => {
                g.malicious_suit_defendant = corp;
                g.malicious_suit_plaintiff = 0;
            }
            _ => {
                g.merger_acquirer = corp;
                g.merger_target = 0;
            }
        }
        let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
        let Some((_, a)) = compare_report(&g, corp, rng0) else { return };
        aborted += cast::<usize, _>(a);
    }
    assert_eq!(aborted, 6, "error exits");
}
