//! Tests of `logic::reports_corp`: unit tests plus differential tests against the original routines
//! run under emulation (tools/emu/game.py, see tests/diff/mod.rs).

#![allow(clippy::field_reassign_with_default)]
use wsr_core::pb::real::{Ext, cast};


mod diff;

use diff::{Compare, Opts, Original, Rand, par_seeds, run_original_with};
use wsr_core::logic::reports_corp::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::pb::numeric::Cur;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

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



const SKIP_ALERTS: (u32, u32) = (0x6830fc, 1);

fn run(addr: u32, args: &[i32], g: &Globals, rng: u32, skips: &[(u32, u32)]) -> Option<Original> {
    let opts = Opts {
        args: args.iter().map(|a| a.to_string()).collect(),
        rng: Some(rng),
        skips: skips.to_vec(),
        ..Opts::default()
    };
    run_original_with(addr, &opts, g)
}

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
            if l.len() < 1050 {
                l.push(s(" "));
            }
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

/// Number of messages the original showed: MSGBOXes and ShowQuickMessage popups (qm.exe).
fn orig_msgs(o: &Original) -> usize {
    o.log.iter().filter(|l| l.starts_with("MSGBOX") || l.contains("CreateProcessA(0x0, 'qm.exe")).count()
}

/// UI-only state the ports don't model.
fn cmp() -> Compare {
    Compare::default().rel_tol(Ext::from_f64(1e-12)).ignore("TickerPauseUntil").ignore("hReportDlg")
}

/// [`world`] plus the data the company reports read: names, currency, calendar, industries, lawsuits,
/// convertible bonds, player advances, fund advisers, physical commodities, swaps and holdings.
fn report_world(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut g = world(seed);
    pow_safe(&mut g, seed);
    let mut r = Rand(seed.wrapping_mul(0x2545_f491) | 1);
    let months = ["", "January", __content.string(6796), "March", "April", "May", "June", "July", "August", __content.string(6797),
        "October", __content.string(6798), __content.string(6799), ""];
    for (k, m) in months.iter().enumerate() {
        g.month_name[cast::<i32, _>(k)] = s(m);
    }
    for i in 0..=71 {
        g.ind_name[i] = s(&format!("{__wsr_text_13937}{i}",__wsr_text_13937=__content.string(13937)));
    }
    for i in 0..=55 {
        g.country_name[i] = s(&format!("{__wsr_text_13977}{i}",__wsr_text_13977=__content.string(13977)));
        g.country_display_name[i] = s(&format!("LAND{i}"));
    }
    match seed % 3 {
        0 => {
            g.currency_suffix = s("");
            g.currency_name = s("Dollar");
            g.currency_symbol = s("$");
            g.currency_code = s("USA");
            g.million_prefix_lc = s("m");
            g.million_prefix_uc = s("M");
        }
        1 => {
            g.currency_suffix = s(" Pound");
            g.currency_name = s("Pounds");
            g.currency_symbol = s(" ");
            g.currency_code = s("GBR");
            g.million_prefix_lc = s("b");
            g.million_prefix_uc = s("B");
        }
        _ => {
            g.currency_suffix = s(" SwFr");
            g.currency_name = s(__content.string(6655));
            g.currency_symbol = s("SwFr ");
            g.currency_code = s("CHE");
            g.million_prefix_lc = s("m");
            g.million_prefix_uc = s("M");
        }
    }
    g.sel_corp_name = s(&format!("Selected Corp {seed}"));
    g.sel_entity = cast::<i32, _>(r.range(0, 1600));
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    g.ceo_bonus_multiple = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(2.0));
    for p in 1..=g.num_players {
        if r.chance(Ext::from_f64(0.5)) {
            g.player_ceo_of[p] = cast::<i32, _>(r.range(131, 1580));
            g.player_ceo_since_year[p] = if r.chance(Ext::from_f64(0.5)) { 0 } else { g.cur_year - cast::<i32, _>(r.range(0, 8)) };
        }
    }
    // antitrust and lawsuits
    if r.chance(Ext::from_f64(0.6)) {
        g.antitrust_defendant = cast::<i32, _>(r.range(131, 1580));
        g.antitrust_damages = r.range(0, 5000);
        g.antitrust_market_share = cast::<i32, _>(r.range(30, 80));
    }
    for k in 1..=10 {
        if r.chance(Ext::from_f64(0.5)) {
            let def = r.range(131, 1580);
            g.lawsuit_rec[k] = s(&format!("{}|{}{__wsr_text_13978}{}|{}", r.range(0, 1), def, r.range(20, 90), r.range(1, 3000),__wsr_text_13978=__content.string(13978)));
        }
    }
    for e in 11..=1600 {
        // convertibles and bond issues of corps after SimCoCount (pow_safe keeps bonds there)
        if g.bonds_issued[e] > 0 && r.chance(Ext::from_f64(0.4)) {
            let mat = g.cur_year + cast::<i32, _>(r.range(-1, 25));
            g.bond_issue_info[e] = s(&format!("{mat}|{:.1}|cv|{:05.2}", f64r(&mut r, Ext::from(Ext::from_f64(50.0)), Ext::from(Ext::from_f64(2000.0))), f64r(&mut r, Ext::from(Ext::from_f64(2.0)), Ext::from(Ext::from_f64(12.0)))));
            g.bond_price[e] = Cur::from_f64(f64r(&mut r, Ext::from_f64(50.0), Ext::from_f64(600.0)));
        } else if g.bonds_issued[e] > 0 {
            let mat = g.cur_year + cast::<i32, _>(r.range(-1, 25));
            g.bond_issue_info[e] = s(&format!("{mat}|0||{:05.2}", f64r(&mut r, Ext::from(Ext::from_f64(2.0)), Ext::from(Ext::from_f64(12.0)))));
        }
        g.goodwill[e] = if e > 130 && r.chance(Ext::from_f64(0.2)) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(100.0)) } else { Ext::from_f64(0.0) };
        g.ctrl_since_year[e] = if r.chance(Ext::from_f64(0.5)) { g.cur_year - cast::<i32, _>(r.range(0, 120)) } else { 0 };
        g.auto_pilot[e] = cast::<i32, _>(r.range(0, 1));
        g.credit_frozen[e] = cast::<i32, _>(r.range(0, 1));
        g.commod_margin_bal[e] = if r.chance(Ext::from_f64(0.2)) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(50.0)) } else { Ext::from_f64(0.0) };
        g.extraordinary_pl[e] = if r.chance(Ext::from_f64(0.5)) { r.range(-500, 500) } else { 0 };
        for p in 1..=g.num_players {
            if r.chance(Ext::from_f64(0.02)) {
                g.player_advances[(p, e)] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(50.0));
            }
        }
        if (11..=130).contains(&e) && r.chance(Ext::from_f64(0.3)) {
            g.hold_bond_id[(1, e)] = cast::<i32, _>(r.range(131, 1600));
            g.hold_bond_id[(0, e)] = 1;
        }
    }
    for f in 1581..=1600 {
        g.fund_adviser[f] = if r.chance(Ext::from_f64(0.7)) { cast::<i32, _>(r.range(86, 1580)) } else { 0 };
        g.fund_fee_pct[f] = cast::<f32, _>([0.1f32, 0.3f32, 0.5f32, 1.2f32][cast::<usize, _>(r.range(0, 3))]);
    }
    // physical commodity records of a few holders (two of the same kind, merged by the report)
    let names = ["OIL", "GOLD", "ETHEREUM", "BITCOIN", "WHEAT"];
    for k in 1..=60 {
        let holder = r.range(120, 1600);
        g.phys_commod_rec[k] = s(&format!("{}|{}|{}|{:.2}", names[cast::<usize, _>(r.range(0, 4))], holder, r.range(1, 900), f64r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(500.0)))));
    }
    g
}

/// Gives company `c` the features the reports describe (bonds, swaps, commodities, advances, a single
/// holding, lawsuits, fund relations), each with some probability.
fn decorate(g: &mut Globals, c: i32, r: &mut Rand) {
    let __content = wsr_core::content::current();
    if c >= g.sim_co_count && r.chance(Ext::from_f64(0.8)) {
        g.bonds_issued[c] = r.range(1, 3000);
        let mat = g.cur_year + [-2, 0, 1, 3, 7, 12, 25][cast::<usize, _>(r.range(0, 6))];
        let coupon = f64r(r, Ext::from_f64(2.0), Ext::from_f64(14.0));
        g.bond_issue_info[c] = if r.chance(Ext::from_f64(0.5)) {
            s(&format!("{mat}|{:.1}|cv|{coupon:05.2}", f64r(r, Ext::from(Ext::from_f64(50.0)), Ext::from(Ext::from_f64(3000.0)))))
        } else {
            s(&format!("{mat}|0||{coupon:05.2}"))
        };
        g.bond_price[c] = Cur::from_f64(f64r(r, Ext::from_f64(30.0), Ext::from_f64(700.0)));
    }
    if r.chance(Ext::from_f64(0.5)) {
        let other = r.range(11, 1600);
        let (a, b) = if r.chance(Ext::from_f64(0.5)) { (cast::<i64, _>(c), other) } else { (other, cast::<i64, _>(c)) };
        g.swap_contracts[cast::<i32, _>(r.range(1, 20))] = s(&format!("{a}|{b}|5.5|P|2020.1|2030.4|{}", r.range(1, 900)));
        if r.chance(Ext::from_f64(0.3)) {
            g.swap_contracts[cast::<i32, _>(r.range(21, 40))] = s(&format!("{c}|{}|4.5|L|2020.1|2031.4|{}", r.range(11, 1600), r.range(1, 900)));
        }
    }
    if r.chance(Ext::from_f64(0.5)) {
        let names = ["OIL", "GOLD", "ETHEREUM", "BITCOIN", "SILVER"];
        let n = names[cast::<usize, _>(r.range(0, 4))];
        for k in 0..r.range(1, 3) {
            let name = if k == 2 && r.chance(Ext::from_f64(0.5)) { "CORN" } else { n };
            g.phys_commod_rec[100 + cast::<i32, _>(k)] = s(&format!("{name}|{c}|{}|{:.2}", r.range(1, 500), f64r(r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(300.0)))));
        }
    }
    if r.chance(Ext::from_f64(0.4)) {
        for p in 1..=g.num_players {
            g.player_advances[(p, c)] = if r.chance(Ext::from_f64(0.6)) { f64r(r, Ext::from_f64(0.0), Ext::from_f64(80.0)) } else { Ext::from_f64(0.0) };
        }
    }
    if r.chance(Ext::from_f64(0.4)) {
        for slot in 1..=15 {
            g.hold_stock_id[(slot, c)] = 0;
            g.hold_stock_pct[(slot, c)] = 0;
        }
        g.hold_stock_id[(1, c)] = cast::<i16, _>(r.range(131, 1580));
        g.hold_stock_pct[(1, c)] = cast::<i16, _>(r.range(1, 60));
        g.hold_stock_id[(0, c)] = 1;
    }
    if r.chance(Ext::from_f64(0.4)) {
        g.lawsuit_rec[cast::<i32, _>(r.range(1, 10))] = s(&format!("1|{c}{__wsr_text_13978}{}|{}", r.range(20, 90), r.range(1, 3000),__wsr_text_13978=__content.string(13978)));
    }
    if g.industry_num[c] == 71 && r.chance(Ext::from_f64(0.8)) {
        // a valid fund: nobody holds 15%+, it controls itself
        for e in 1..=1600 {
            for slot in 1..=15 {
                if cast::<i32, _>(g.hold_stock_id[(slot, e)]) == c {
                    g.hold_stock_pct[(slot, e)] = cast::<i16, _>(r.range(0, 8));
                }
            }
        }
        g.corp_controller[c] = c;
        g.fund_adviser[c] = if r.chance(Ext::from_f64(0.6)) { pick(g, r, 131, 1580) } else { 0 };
        if r.chance(Ext::from_f64(0.5)) {
            let inv = pick(g, r, 131, 1580);
            g.industry_num[inv] = 37;
            g.credit_rating_num[inv] = 8;
            g.fund_adviser[c] = inv;
        }
    }
    if g.industry_num[c] == 37 || g.industry_num[c] == 2 {
        for f in 1581..=1600 {
            if r.chance(Ext::from_f64(0.4)) {
                g.fund_adviser[f] = c;
                g.industry_num[f] = 71;
                g.entity_status[f] = 1;
            }
        }
    }
    if r.chance(Ext::from_f64(0.3)) {
        g.tax_credits[c] = f64r(r, Ext::from_f64(0.0), Ext::from_f64(30.0));
    }
    if r.chance(Ext::from_f64(0.3)) && (11..=84).contains(&c) {
        g.bank_debt[c + 1] = f64r(r, Ext::from_f64(1.0), Ext::from_f64(500.0));
    }
}

fn pick(g: &Globals, r: &mut Rand, lo: i64, hi: i64) -> i32 {
    loop {
        let c = cast::<i32, _>(r.range(lo, hi));
        if g.entity_status[c] == 1 {
            return c;
        }
    }
}

fn check_balance_sheet(g: &Globals, corp: i32, rng0: u32) {
    let __content = wsr_core::content::current();
    let Some(o) = run(0x430898, &[corp], g, rng0, &[SKIP_ALERTS]) else {
        return;
    };
    let mut port = g.clone();
    let mut ui = Rec::default();
    let mut rng = Rng::from_state(rng0);
    show_balance_sheet(&mut port, &mut ui, &mut rng, corp);
    let orig = orig_reports(&o);
    eprintln!("corp {corp}{__wsr_text_13979}{}): {}{__wsr_text_13751}{:.2}{__wsr_text_13748}", g.industry_num[corp], orig.len(), o.elapsed,__wsr_text_13979=__content.string(13979), __wsr_text_13751=__content.string(13751), __wsr_text_13748=__content.string(13748));
    if std::env::var("SHOW_REPORT").is_ok() {
        for (_, l) in &orig {
            for x in l {
                eprintln!("  |{x}");
            }
        }
        for l in &o.log {
            eprintln!("  LOG {l}");
        }
    }
    assert_reports(&port_reports(&ui), &orig);
    assert_eq!(rng.state(), o.rng, "RND state");
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "messages: port {:?}, original log {:?}", ui.msgs, o.log);
    cmp().assert(&port, &o.globals);
}

#[test]
fn diff_show_balance_sheet() {
    par_seeds(1u64..=10, |seed| {
        let g = report_world(seed);
        let mut r = Rand(seed * 977);
        let mut corps = vec![pick(&g, &mut r, 11, 85), pick(&g, &mut r, 86, 130), pick(&g, &mut r, 131, 1580)];
        corps.push(pick(&g, &mut r, 1581, 1583));
        corps.push(g.antitrust_defendant.max(131));
        for corp in corps {
            if g.entity_status[corp] != 1 {
                continue;
            }
            let mut g = g.clone();
            decorate(&mut g, corp, &mut r);
            check_balance_sheet(&g, corp, cast::<u32, _>(r.range(1, 0x7fff_ffff)));
        }
    });
}

/// A plausible `EntMiscData` earnings record (41 `|` fields) for company `c`.
fn misc_record(g: &Globals, c: i32, r: &mut Rand) -> PbStr {
    let __content = wsr_core::content::current();
    let mut f: Vec<String> = Vec::new();
    let money = |r: &mut Rand, lo: Ext, hi: Ext| format!("{:.2}", f64r(r, lo, hi));
    let qtr = money(r, -Ext::from_f64(30.0), Ext::from_f64(80.0));
    for k in 1..=41 {
        let v = match k {
            1 | 2 | 4 | 5 => if r.chance(Ext::from_f64(0.15)) { "0".to_string() } else { money(r, -Ext::from_f64(30.0), Ext::from_f64(100.0)) },
            3 => qtr.clone(),
            6 => r.range(1, 4).to_string(),
            8 => if r.chance(Ext::from_f64(0.6)) { "0".to_string() } else { money(r, -Ext::from_f64(40.0), Ext::from_f64(40.0)) },
            9 => if r.chance(Ext::from_f64(0.7)) { "0".to_string() } else { format!("{}^{}^0", r.range(11, 1600), r.range(11, 1600)) },
            10 | 14 => r.range(-20, 40).to_string(),
            11 => [-1, 0, 0, 1, 2][cast::<usize, _>(r.range(0, 4))].to_string(),
            12 => if r.chance(Ext::from_f64(0.5)) { "0".to_string() } else { money(r, Ext::from_f64(0.0), Ext::from_f64(30.0)) },
            13 => if r.chance(Ext::from_f64(0.5)) { "0".to_string() } else { money(r, -Ext::from_f64(100.0), Ext::from_f64(100.0)) },
            15 => ["0", "0", "-999", "-9999", "-12", "35"][cast::<usize, _>(r.range(0, 5))].to_string(),
            16 => if r.chance(Ext::from_f64(0.8)) { g.industry_num[c].to_string() } else { r.range(0, 75).to_string() },
            17 | 18 => if r.chance(Ext::from_f64(0.3)) { "0".to_string() } else { money(r, -Ext::from_f64(50.0), Ext::from_f64(50.0)) },
            19 | 20 => if r.chance(Ext::from_f64(0.3)) { "0".to_string() } else { money(r, Ext::from_f64(0.0), Ext::from_f64(900.0)) },
            21 | 22 => if r.chance(Ext::from_f64(0.4)) { "0".to_string() } else { money(r, Ext::from_f64(0.0), Ext::from_f64(400.0)) },
            23 => ["", "x", __content.string(13980), __content.string(13981)][cast::<usize, _>(r.range(0, 3))].to_string(),
            24 | 25 => if r.chance(Ext::from_f64(0.6)) { "0".to_string() } else { money(r, Ext::from_f64(0.0), Ext::from_f64(50.0)) },
            26 => if r.chance(Ext::from_f64(0.5)) { "0".to_string() } else { money(r, -Ext::from_f64(40.0), Ext::from_f64(40.0)) },
            27 => money(r, -Ext::from_f64(10.0), Ext::from_f64(30.0)),
            28 => if r.chance(Ext::from_f64(0.6)) { "0".to_string() } else { r.range(11, 1600).to_string() },
            31 => r.range(0, 3).to_string(),
            32 => if r.chance(Ext::from_f64(0.6)) { "0".to_string() } else { money(r, Ext::from_f64(0.0), Ext::from_f64(9.0)) },
            33..=37 => if r.chance(Ext::from_f64(0.7)) { "0".to_string() } else { money(r, Ext::from_f64(0.0), Ext::from_f64(20.0)) },
            38 => if r.chance(Ext::from_f64(0.5)) { "0".to_string() } else { money(r, -Ext::from_f64(60.0), Ext::from_f64(60.0)) },
            39 => if r.chance(Ext::from_f64(0.5)) { "0".to_string() } else { money(r, Ext::from_f64(0.0), Ext::from_f64(20.0)) },
            40 => if r.chance(Ext::from_f64(0.5)) { "0".to_string() } else { money(r, -Ext::from_f64(20.0), Ext::from_f64(20.0)) },
            41 => if r.chance(Ext::from_f64(0.2)) { "0".to_string() } else { format!("{:.2}", qtr.parse::<Ext>().unwrap() * f64r(r, Ext::from(Ext::from_f64(0.7)), Ext::from(Ext::from_f64(1.3))) + f64r(r, Ext::from(-Ext::from_f64(3.0)), Ext::from(Ext::from_f64(3.0)))) },
            _ => r.range(0, 9).to_string(),
        };
        f.push(v);
    }
    s(&f.join("|"))
}

fn check_earnings_report(g: &Globals, corp: i32, rng0: u32) {
    let __content = wsr_core::content::current();
    let Some(o) = run(0x462f1e, &[corp], g, rng0, &[SKIP_ALERTS]) else {
        return;
    };
    let mut port = g.clone();
    let mut ui = Rec::default();
    let mut rng = Rng::from_state(rng0);
    show_earnings_report(&mut port, &mut ui, &mut rng, corp);
    let orig = orig_reports(&o);
    eprintln!("corp {corp}{__wsr_text_13979}{}): {}{__wsr_text_13751}{:.2}{__wsr_text_13748}", g.industry_num[corp], orig.len(), o.elapsed,__wsr_text_13979=__content.string(13979), __wsr_text_13751=__content.string(13751), __wsr_text_13748=__content.string(13748));
    if std::env::var("SHOW_REPORT").is_ok() {
        for (_, l) in &orig {
            for x in l {
                eprintln!("  |{x}");
            }
        }
        for l in &o.log {
            eprintln!("  LOG {l}");
        }
    }
    assert_reports(&port_reports(&ui), &orig);
    assert_eq!(rng.state(), o.rng, "RND state (draws: port {:?}{__wsr_text_13982}{:?})", draws(rng0, rng.state()), draws(rng0, o.rng),__wsr_text_13982=__content.string(13982));
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "messages: port {:?}, original log {:?}", ui.msgs, o.log);
    cmp().assert(&port, &o.globals);
}

/// Number of RND draws from `from` to `to` (for failure messages).
fn draws(from: u32, to: u32) -> Option<usize> {
    let mut r = Rng::from_state(from);
    (0..100_000).find(|_| {
        if r.state() == to {
            return true;
        }
        r.rnd();
        false
    })
}

#[test]
fn diff_show_earnings_report() {
    let __content = wsr_core::content::current();
    par_seeds(1u64..=18, |seed| {
        let mut g = report_world(seed);
        let mut r = Rand(seed * 7717);
        g.rand_seed = cast::<i32, _>(r.range(0, 30000));
        g.asbestos_corp = if r.chance(Ext::from_f64(0.5)) { -(cast::<i32, _>(r.range(131, 1580))) } else { cast::<i32, _>(r.range(131, 1580)) };
        g.superfund_corp = cast::<i16, _>(r.range(131, 1580));
        g.cur_player_name = s(if seed % 4 == 0 { "...OUT..." } else { "Player One" });
        let mut corps = vec![pick(&g, &mut r, 11, 85), pick(&g, &mut r, 86, 130), pick(&g, &mut r, 131, 1580)];
        corps.push(pick(&g, &mut r, 131, 1580));
        corps.push(pick(&g, &mut r, 1581, 1583));
        if seed % 6 == 1 {
            corps.push(cast::<i32, _>(r.range(1, 10))); // not a corporation: popup
        }
        for corp in corps {
            let mut g = g.clone();
            if corp > 10 {
                decorate(&mut g, corp, &mut r);
                if !r.chance(Ext::from_f64(0.1)) {
                    g.ent_misc_data[corp] = misc_record(&g, corp, &mut r);
                }
                if r.chance(Ext::from_f64(0.3)) {
                    g.asbestos_corp = corp;
                }
                if (131..=1580).contains(&corp) && r.chance(Ext::from_f64(0.5)) {
                    g.industry_num[corp] = [37, 7, 15, 50, 52, 3][cast::<usize, _>(r.range(0, 5))];
                }
                // headlines mentioning the company
                for k in 1..=60 {
                    if r.chance(Ext::from_f64(0.15)) {
                        let sym = g.ent_symbol[corp].to_display();
                        g.news_headline[k] = s(&[format!("News about {sym}{__wsr_text_13983}",__wsr_text_13983=__content.string(13983)), format!("Big deal ({sym})"), format!("Rumors on {sym}."), format!("{sym} reports earnings of 5")][cast::<usize, _>(r.range(0, 3))]);
                    }
                }
            }
            check_earnings_report(&g, corp, cast::<u32, _>(r.range(1, 0x7fff_ffff)));
        }
    });
}

// ------------------------------------------------------------------------------------------ helpers

#[test]
fn calc_bond_call_price_pct_values() {
    let __content = wsr_core::content::current();
    let want = [(-3, 105), (0, 100), (1, 100), (2, 101), (3, 102), (5, 104), (6, 105), (9, 105), (12, 105)];
    for (y, p) in want {
        assert_eq!(calc_bond_call_price_pct(y), p, "{__wsr_text_13984}{y}",__wsr_text_13984=__content.string(13984));
    }
}

#[test]
fn diff_calc_bond_call_price_pct() {
    let g = Globals::default();
    for y in [-3, -1, 0, 1, 2, 3, 4, 5, 6, 9, 10, 40] {
        let Some(o) = run(0x4f8ed2, &[y], &g, 1, &[]) else { return };
        assert_eq!(calc_bond_call_price_pct(y), cast::<i32, _>(o.return_value), "years {y}");
    }
}

#[test]
fn get_entity_sole_commodity_cases() {
    let mut g = Globals::default();
    g.phys_commod_rec[3] = s(" GOLD |200|5|10");
    g.phys_commod_rec[9] = s("GOLD|200|7|12");
    g.futures_recs[4] = s("OIL|200|L|3");
    g.futures_recs[5] = s("CORN|200|S|3");
    assert_eq!(get_entity_sole_commodity(&g, 200, 2), "GOLD");
    assert_eq!(get_entity_sole_commodity(&g, 200, 1), "NO");
    assert_eq!(get_entity_sole_commodity(&g, 201, 2), "NO");
}

#[test]
fn diff_get_entity_sole_commodity_and_consolidate() {
    let __content = wsr_core::content::current();
    for seed in 1u64..=6 {
        let mut r = Rand(seed * 31337);
        let mut g = Globals::default();
        let names = ["OIL", "GOLD", " GOLD", "SILVER", "ETHEREUM"];
        let holders = [140, 141, 142, 120];
        for k in 1..=40 {
            if r.chance(Ext::from_f64(0.7)) {
                let h = holders[cast::<usize, _>(r.range(0, 3))];
                let rec = format!("{}|{}|{}|{:.3}", names[cast::<usize, _>(r.range(0, 4))], h, r.range(1, 2000), f64r(&mut r, Ext::from(Ext::from_f64(0.1)), Ext::from(Ext::from_f64(900.0))));
                g.phys_commod_rec[cast::<i32, _>(r.range(1, 1000))] = s(&rec);
                g.futures_recs[k] = s(&format!("{}|{}{__wsr_text_13985}{}", names[cast::<usize, _>(r.range(0, 1))], h, r.range(1, 9),__wsr_text_13985=__content.string(13985)));
            }
        }
        for h in holders {
            for kind in [1, 2] {
                let opts = Opts { args: vec![h.to_string(), kind.to_string()], ..Opts::default() };
                let Some(o) = run_original_with(0x6358eb, &opts, &g) else { return };
                assert_eq!(Some(get_entity_sole_commodity(&g, h, kind)), o.return_str, "{__wsr_text_13986}{h}{__wsr_text_13623}{kind}",__wsr_text_13986=__content.string(13986), __wsr_text_13623=__content.string(13623));
            }
            let Some(o) = run(0x665a58, &[h], &g, 1, &[]) else { return };
            let mut port = g.clone();
            consolidate_phys_commod_recs(&mut port, h);
            cmp().assert(&port, &o.globals);
        }
    }
}

#[test]
fn diff_update_antitrust_reserves() {
    let __content = wsr_core::content::current();
    for seed in 1u64..=4 {
        let mut g = report_world(seed);
        let mut r = Rand(seed * 4241);
        g.private_antitrust_off = if seed == 4 { 1 } else { 0 };
        let mut corps = vec![g.antitrust_defendant, pick(&g, &mut r, 131, 1580), 0];
        for k in 1..=10 {
            if mid_is_one(&g.lawsuit_rec[k]) {
                corps.push(cast::<i32, _>(wsr_core::pb::numeric::val(wsr_core::pb::string::parse_any(g.lawsuit_rec[k].as_bytes(), b"|", 2).as_bytes())));
            }
        }
        corps.truncate(6);
        for corp in corps {
            let mut g = g.clone();
            if corp > 0 && r.chance(Ext::from_f64(0.5)) {
                g.contingency_reserve[corp] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(500.0));
                g.reserve_adj_pending[corp] = Ext::from_f64(0.0);
            }
            let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
            let Some(o) = run(0x5469ca, &[corp], &g, rng0, &[SKIP_ALERTS]) else { return };
            let mut port = g.clone();
            let mut ui = Rec::default();
            let mut rng = Rng::from_state(rng0);
            let res = update_antitrust_reserves(&mut port, &mut ui, &mut rng, corp);
            eprintln!("corp {corp}: result {res}, {:.2}{__wsr_text_13748}", o.elapsed,__wsr_text_13748=__content.string(13748));
            assert_eq!(Some(cast::<Ext, _>(res)), o.return_float, "result");
            assert_eq!(rng.state(), o.rng, "RND state");
            cmp().assert(&port, &o.globals);
        }
    }
}

fn mid_is_one(x: &PbStr) -> bool {
    x.as_bytes().first() == Some(&b'1')
}

#[test]
fn diff_assign_etf_adviser() {
    let __content = wsr_core::content::current();
    for seed in 1u64..=6 {
        let mut g = report_world(seed);
        let mut r = Rand(seed * 6421);
        // a few investment companies and insurers, some already advising funds
        for _ in 0..30 {
            let c = pick(&g, &mut r, 131, 1580);
            g.industry_num[c] = 37;
        }
        if seed % 3 == 0 {
            g.start_year = g.cur_year;
            g.cur_quarter = 1;
        }
        for (k, etf) in [1581, 1586, 1590].into_iter().enumerate() {
            let old = [0, pick(&g, &mut r, 86, 130), pick(&g, &mut r, 131, 1580)][(k + cast::<usize, _>(seed)) % 3];
            let voluntary = cast::<i32, _>((k + cast::<usize, _>(seed)) / 2 % 2);
            if old > 0 && r.chance(Ext::from_f64(0.5)) {
                g.corp_controller[old] = cast::<i32, _>(r.range(1, 10));
            }
            g.fund_adviser[etf] = old;
            let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
            let Some(o) = run(0x64cdb6, &[etf, old, voluntary], &g, rng0, &[]) else { return };
            let mut port = g.clone();
            let mut ui = Rec::default();
            let mut rng = Rng::from_state(rng0);
            assign_etf_adviser(&mut port, &mut ui, &mut rng, etf, old, voluntary);
            eprintln!("etf {etf}{__wsr_text_13575}{old} vol {voluntary}{__wsr_text_13987}{}{__wsr_text_13988}{}", port.fund_adviser[etf], port.fund_fee_pct[etf],__wsr_text_13575=__content.string(13575), __wsr_text_13987=__content.string(13987), __wsr_text_13988=__content.string(13988));
            assert_eq!(rng.state(), o.rng, "RND state");
            assert_eq!(ui.msgs.len(), orig_msgs(&o), "messages {:?} / {:?}", ui.msgs, o.log);
            cmp().assert(&port, &o.globals);
            g = o.globals;
        }
    }
}

/// Clears all stock holdings and gives the companies in `ids` the same controller.
fn clean_holdings(g: &mut Globals, ids: &[i32], ctrl: i32) {
    for e in 1..=1600 {
        for slot in 0..=15 {
            g.hold_stock_id[(slot, e)] = 0;
            g.hold_stock_pct[(slot, e)] = 0;
        }
    }
    for &i in ids {
        g.corp_controller[i] = ctrl;
        g.entity_status[i] = 1;
    }
}

fn hold(g: &mut Globals, holder: i32, corp: i32, pct: i16) {
    let n = g.hold_stock_id[(0, holder)] + 1;
    g.hold_stock_id[(cast::<i32, _>(n), holder)] = cast::<i16, _>(corp);
    g.hold_stock_pct[(cast::<i32, _>(n), holder)] = pct;
    g.hold_stock_basis[(cast::<i32, _>(n), holder)] = cast::<Ext, _>(pct) * Ext::from_f64(3.0);
    g.hold_stock_id[(0, holder)] = n;
}

#[test]
fn diff_check_cross_ownership() {
    // Divested stocks are banks (11..85), whose valuation does not involve ShowCashFlowProjection.
    for seed in 1u64..=8 {
        let mut g = report_world(seed);
        let mut r = Rand(seed * 99991);
        let ids: Vec<i32> = (0..6).map(|_| pick(&g, &mut r, 11, 85)).collect();
        let ctrl = if seed % 2 == 0 { 2 } else { 700 };
        clean_holdings(&mut g, &ids, ctrl);
        let (a, b, c2, d) = (ids[0], ids[1], ids[2], ids[3]);
        match seed % 4 {
            0 => {
                // mutual holding, both over 10%
                hold(&mut g, a, b, 25);
                hold(&mut g, b, a, cast::<i16, _>(r.range(11, 40)));
            }
            1 => {
                // circle a -> b -> c -> a (holdings over 10% in the same group)
                hold(&mut g, a, b, 30);
                hold(&mut g, b, c2, 40);
                hold(&mut g, c2, a, 15);
                hold(&mut g, d, a, 12);
            }
            2 => {
                // a deeper circle and an unrelated holding
                hold(&mut g, a, b, 30);
                hold(&mut g, b, c2, 40);
                hold(&mut g, c2, d, 45);
                hold(&mut g, d, a, 20);
                hold(&mut g, a, ids[4], 5);
            }
            _ => {
                // nothing illegal
                hold(&mut g, a, b, 30);
                hold(&mut g, b, c2, 8);
            }
        }
        let force = cast::<i32, _>(seed % 3 != 0);
        let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
        let Some(o) = run(0x481079, &[a, force], &g, rng0, &[SKIP_ALERTS]) else { return };
        let mut port = g.clone();
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(rng0);
        let res = check_cross_ownership(&mut port, &mut ui, &mut rng, a, force);
        eprintln!("seed {seed}: result {res}, news {:?}, msgs {:?}", ui.news, ui.msgs);
        assert_eq!(res, cast::<i32, _>(o.return_value), "result");
        assert_eq!(rng.state(), o.rng, "RND state");
        assert_eq!(ui.msgs.len(), orig_msgs(&o), "messages {:?} / {:?}", ui.msgs, o.log);
        cmp().assert(&port, &o.globals);
    }
}

#[test]
fn diff_divest_stock() {
    let __content = wsr_core::content::current();
    for seed in 1u64..=6 {
        let mut g = report_world(seed);
        let mut r = Rand(seed * 1237);
        let stock = if seed % 2 == 0 { pick(&g, &mut r, 11, 85) } else { pick(&g, &mut r, 131, 1580) };
        let holder = if seed % 3 == 0 { cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))) } else { pick(&g, &mut r, 131, 1580) };
        let slot = cast::<i32, _>(r.range(1, 15));
        g.hold_stock_id[(slot, holder)] = cast::<i16, _>(stock);
        g.hold_stock_pct[(slot, holder)] = cast::<i16, _>(r.range(1, 30));
        g.hold_stock_basis[(slot, holder)] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(300.0));
        if g.hold_stock_id[(0, holder)] < cast::<i16, _>(slot) {
            g.hold_stock_id[(0, holder)] = cast::<i16, _>(slot);
        }
        // potential buyers: rich [runtime resource wording] companies
        for _ in 0..40 {
            let b = pick(&g, &mut r, 86, 1580);
            g.corp_controller[b] = pick(&g, &mut r, 131, 1580);
            g.market_cap[b] = f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(9000.0));
            g.cash[b] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(9000.0));
        }
        let reason = cast::<i32, _>(seed % 4);
        let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
        let Some(o) = run(0x511e37, &[stock, holder, reason], &g, rng0, &[SKIP_ALERTS]) else { return };
        let mut port = g.clone();
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(rng0);
        divest_stock(&mut port, &mut ui, &mut rng, stock, holder, reason);
        eprintln!("seed {seed}: stock {stock}{__wsr_text_13774}{holder}{__wsr_text_13989}{reason}: news {:?}", ui.news,__wsr_text_13774=__content.string(13774), __wsr_text_13989=__content.string(13989));
        assert_eq!(rng.state(), o.rng, "RND state");
        assert_eq!(ui.msgs.len(), orig_msgs(&o), "messages {:?} / {:?}", ui.msgs, o.log);
        cmp().assert(&port, &o.globals);
    }
}
