//! Tests of `logic::cashflow`: unit tests plus differential tests against the original routines run under
//! emulation (tools/emu/game.py, see tests/diff/mod.rs). Reports are compared line by line with the
//! listbox (control 301) of the report dialog the original builds.
//!
//! Harness notes:
//! * CalcEntityFinancials prices bonds with PB_Pow, whose integer-exponent path Unicorn mis-detects
//!   (see tests/finance.rs). States with quarter 1/3 keep every exponent integral; for quarters 2/4 the
//!   original's CalcGovBondYields (0x428477) is skipped after the port's has been applied to the input
//!   state (it only depends on rates and dates, so a second run changes nothing) and no corporation has
//!   bonds issued.
//! * SelectTargetCorp (0x4227d5, not ported) is skipped in CashFlowAlert runs.

#![allow(clippy::field_reassign_with_default)]
use wsr_core::pb::real::{Ext, cast};


mod diff;

use diff::{Compare, Opts, Original, Rand, run_original_with};
use wsr_core::logic::cashflow::*;
use wsr_core::logic::finance::calc_gov_bond_yields;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::pb::numeric::Cur;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

const SHOW_CFP: u32 = 0x6a20e0;
const BANK_CFP: u32 = 0x6a99f7;
const PLAYER_CFP: u32 = 0x6afc4d;
const LOAN_DUE: u32 = 0x6a8805;
const ALERT: u32 = 0x6a9490;
const PLAYER_TAX: u32 = 0x508797;
const ETF_STATUS: u32 = 0x64ff35;
const GOV_YIELDS: (u32, u32) = (0x428477, 0);

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// Records every report, message box and news headline; answers message boxes from a queue.
#[derive(Default)]
struct Rec {
    reports: Vec<(PbStr, Vec<PbStr>)>,
    msgs: Vec<(String, i32, String)>,
    news: Vec<String>,
    answers: Vec<i32>,
}

impl Ui for Rec {
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 {
        self.msgs.push((text.to_display(), style, title.to_display()));
        // ShowQuickMessage pop-ups (style 0, no title) run qm.exe in the original and take no answer
        if !self.answers.is_empty() && !(style == 0 && title.is_empty()) {
            return self.answers.remove(0);
        }
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

/// The port's reports as the original's listbox shows them (ShowReportDialog adds `@END` as " ").
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

/// Message boxes and ShowQuickMessage pop-ups (qm.exe) of the original.
fn orig_msgs(o: &Original) -> usize {
    o.log.iter().filter(|l| l.starts_with("MSGBOX") || l.contains("CreateProcessA(0x0, 'qm.exe")).count()
}

/// With CF_DUMP=file, appends every report line (for checking which report items the tests reach).
fn dump_lines(ui: &Rec) {
    if let Ok(path) = std::env::var("CF_DUMP") {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap();
        for (_, l) in &ui.reports {
            for x in l {
                writeln!(f, "{}", x.to_display()).unwrap();
            }
        }
    }
}

/// UI-only state the ports don't model.
fn cmp() -> Compare {
    Compare::default().rel_tol(Ext::from_f64(1e-12)).ignore("TickerPauseUntil").ignore("hReportDlg")
}

fn f64r(r: &mut Rand, lo: Ext, hi: Ext) -> Ext {
    lo + (hi - lo) * cast::<Ext, _>(r.range(0, 1_000_000_000)) / Ext::from_f64(1e9)
}

fn f32r(r: &mut Rand, lo: Ext, hi: Ext) -> f32 {
    cast::<f32, _>(f64r(r, lo, hi))
}

/// Quarter / SimCoCount / bond issues such that the emulated PB_Pow stays exact (see the module docs).
/// `odd_quarter` = quarter 2 or 4 (then the original's CalcGovBondYields is skipped, see [`run`]).
fn pow_safe(g: &mut Globals, seed: u64, odd_quarter: bool) {
    g.cur_quarter = match (odd_quarter, seed.is_multiple_of(2)) {
        (false, true) => 1,
        (false, false) => 3,
        (true, true) => 2,
        (true, false) => 4,
    };
    g.sim_co_count = if seed.is_multiple_of(3) { 300 } else { 450 };
    for e in 11..=1600 {
        if odd_quarter || e < g.sim_co_count {
            g.bonds_issued[e] = 0;
        }
    }
    if odd_quarter {
        calc_gov_bond_yields(g);
    }
}

/// A plausible mid-game world: players 1..NumPlayers, banks 11..85, insurers 86..130, operating corps,
/// funds 1581..1583, with holdings, bonds, loans and earnings data.
fn world(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut r = Rand(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut g = Globals::default();
    g.world_generated = 1;
    g.game_end_flag = s("N");
    g.num_players = cast::<i32, _>(r.range(1, 4));
    g.cur_player = 1;
    g.difficulty_level = cast::<i32, _>(r.range(1, 4));
    g.cur_year = cast::<i32, _>(r.range(2000, 2030));
    g.start_year = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { g.cur_year } else { g.cur_year - cast::<i32, _>(r.range(1, 10)) };
    g.cur_quarter = cast::<i32, _>(r.range(1, 4));
    g.sim_co_count = 450;
    g.base_interest_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(12.0)));
    g.prime_rate = cast::<f32, _>(g.base_interest_rate + f32r(&mut r, Ext::from_f64(0.0), Ext::from_f64(3.0)));
    g.sofr_rate = cast::<f32, _>(g.base_interest_rate + f32r(&mut r, -Ext::from_f64(0.5), Ext::from_f64(0.5)));
    g.t_bill_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.5), Ext::from_f64(8.0)));
    g.bank_cd_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(9.0)));
    g.gdp_growth = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(5.0), Ext::from_f64(8.0)));
    g.monetary_policy = cast::<i16, _>(r.range(0, 2));
    g.monetary_stance = cast::<i32, _>(r.range(-1, 2));
    g.subprime_crisis_state = cast::<i16, _>(r.range(0, 1));
    g.exchange_rate = cast::<f64, _>(if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { Ext::from_f64(1.0) } else { f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(2.0)) });
    g.loan_limit_multiple = cast::<f64, _>((cast::<Ext, _>(r.range(20, 35))) * Ext::from_f64(0.1));
    g.corp_tax_rate = cast::<i32, _>(r.range(15, 45));
    g.cap_gains_tax_rate = cast::<i32, _>(r.range(0, 30));
    g.wealth_tax_rate = cast::<i32, _>(r.range(0, 3));
    g.long_bond_desc = s(&format!("{} LONG T-BOND {:5.2}", g.cur_year + cast::<i32, _>(r.range(8, 30)), f32r(&mut r, Ext::from(Ext::from_f64(2.0)), Ext::from(Ext::from_f64(12.0)))));
    g.short_bond_desc = s(&format!("{} SHORT T-NOTE {:5.2}", g.cur_year + cast::<i32, _>(r.range(0, 3)), f32r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(10.0)))));
    g.long_bond_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(70.0), Ext::from_f64(130.0)));
    g.short_bond_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(85.0), Ext::from_f64(110.0)));
    g.currency_name = s(["Dollars", "Pound", "Euros", "Yen", "Franc"][cast::<usize, _>(r.range(0, 4))]);
    g.currency_suffix = s(["", " Dollar", " SwFr", " Euro"][cast::<usize, _>(r.range(0, 3))]);
    g.currency_symbol = s(["$", " £ ", "EUR"][cast::<usize, _>(r.range(0, 2))]);
    g.million_prefix_uc = s(["M", "B"][cast::<usize, _>(r.range(0, 1))]);
    g.million_prefix_lc = s(["m", "b"][cast::<usize, _>(r.range(0, 1))]);
    g.rand_seed = cast::<i32, _>(r.range(0, 32600));
    g.tmp_ext1 = cast::<Ext, _>(r.range(0, 30));
    g.tmp_player_advances = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(100.0)) } else { Ext::from_f64(0.0) };
    for p in 1..=5 {
        g.player_name[p] = s(&format!("{__wsr_text_13556}{p}",__wsr_text_13556=__content.string(13556)));
        g.is_computer_player[p] = if p > 1 && r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { 1 } else { 0 };
        g.player_growth_throttle[p] = cast::<i16, _>(r.range(0, 30));
        g.player_prepaid_tax[p] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(50.0));
        g.player_cap_gains[p] = f64r(&mut r, -Ext::from_f64(30.0), Ext::from_f64(80.0));
        g.player_ceo_of[p] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { cast::<i32, _>(r.range(11, 1600)) } else { 0 };
    }
    for e in 1..=1600 {
        let active = if e <= 10 { e <= g.num_players } else { r.chance(cast::<f64, _>(Ext::from_f64(0.93))) };
        g.entity_status[e] = if active { 1 } else if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { 0 } else { 2 };
        g.ent_name[e] = s(&format!("{}{__wsr_text_13607}{e}", ["Acme", "Bk", "Global Widgets", "X"][cast::<usize, _>(r.range(0, 3))],__wsr_text_13607=__content.string(13607)));
        g.ent_symbol[e] = s(["XYZ", "BKH", "GOOG", "AAPL", "AMZN", "ABC", "QRS"][cast::<usize, _>(r.range(0, 6))]);
        g.cash[e] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(3000.0));
        g.t_bills[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(1000.0)) } else { Ext::from_f64(0.0) };
        g.bank_debt[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(2000.0)) } else { Ext::from_f64(0.0) };
        g.lending_bank[e] = cast::<i16, _>(r.range(11, 50));
        g.credit_rating_num[e] = cast::<i32, _>(r.range(0, 10));
        g.corp_controller[e] = match r.range(0, 5) {
            0 => 0,
            1 | 2 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
            _ => cast::<i32, _>(r.range(11, 1600)),
        };
        g.credit_frozen[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) { 1 } else { 0 };
        g.accrued_pl[e] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(200.0));
        g.div_recvd1[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(20.0));
        g.div_recvd2[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(20.0));
        g.extraordinary_pl[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { r.range(-50, 80) } else { 0 };
        let mut n = 0;
        for slot in 1..=15 {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
                g.hold_stock_id[(slot, e)] = cast::<i16, _>(r.range(11, 1600));
                g.hold_stock_pct[(slot, e)] = cast::<i16, _>(if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { r.range(80, 100) } else { r.range(1, 79) });
                n += 1;
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) {
                g.hold_bond_id[(slot, e)] = cast::<i32, _>(r.range(11, 1600));
                g.hold_bond_face[(slot, e)] = r.range(1, 200);
                g.bond_hold_cost[(slot, e)] = cast::<Ext, _>(g.hold_bond_face[(slot, e)]) * f64r(&mut r, Ext::from_f64(0.7), Ext::from_f64(1.3));
            }
        }
        g.hold_stock_id[(0, e)] = n;
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.long_gov_bond_face[e] = r.range(1, 300);
            g.lt_gov_bond_cost[e] = cast::<Ext, _>(g.long_gov_bond_face[e]) * f64r(&mut r, Ext::from_f64(0.8), Ext::from_f64(1.2));
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.short_gov_bond_face[e] = r.range(1, 300);
            g.st_gov_bond_cost[e] = cast::<Ext, _>(g.short_gov_bond_face[e]) * f64r(&mut r, Ext::from_f64(0.9), Ext::from_f64(1.1));
        }
        if e > 10 {
            g.industry_num[e] = match e {
                11..=85 => 1,
                86..=130 => 2,
                1581..=1583 => 71,
                _ => {
                    if r.chance(cast::<f64, _>(Ext::from_f64(0.05))) {
                        [37, 69, 70][cast::<usize, _>(r.range(0, 2))]
                    } else {
                        cast::<i16, _>(r.range(3, 40))
                    }
                }
            };
            g.market_cap[e] = f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(3000.0));
            g.shares_outstanding[e] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(50.0));
            g.dividend_per_share[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(0.3)) } else { Ext::from_f64(0.0) };
            g.tax_loss_carry[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { f64r(&mut r, -Ext::from_f64(100.0), Ext::from_f64(30.0)) } else { Ext::from_f64(0.0) };
            g.consol_tax_inc_ytd[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { f64r(&mut r, -Ext::from_f64(30.0), Ext::from_f64(100.0)) } else { Ext::from_f64(0.0) };
            g.tax_credits[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(20.0)) } else { Ext::from_f64(0.0) };
            g.proj_qtr_earnings[e] = f64r(&mut r, -Ext::from_f64(10.0), Ext::from_f64(30.0));
            g.profit_margin[e] = cast::<i16, _>(r.range(-25, 40));
            g.growth_rate[e] = cast::<i16, _>(r.range(-10, 40));
            g.disc_spend_pct[e] = cast::<i16, _>(r.range(0, 25));
            g.reserve_adj_pending[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { f64r(&mut r, -Ext::from_f64(10.0), Ext::from_f64(10.0)) } else { Ext::from_f64(0.0) };
            g.ctrl_since_year[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.8))) { g.cur_year - cast::<i32, _>(r.range(0, 5)) } else { 0 };
            g.ent_misc_data[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.8))) {
                let fields: Vec<String> = (1..=30)
                    .map(|k| match k {
                        12 => format!("{:.3}", f64r(&mut r, Ext::from(Ext::from_f64(0.0)), Ext::from(Ext::from_f64(5.0)))),
                        13 => {
                            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                                "0".to_string()
                            } else {
                                format!("{:.2}", f64r(&mut r, Ext::from(-Ext::from_f64(50.0)), Ext::from(Ext::from_f64(50.0))))
                            }
                        }
                        29 => r.range(0, 25).to_string(),
                        30 => format!("{:.2}", f64r(&mut r, Ext::from(-Ext::from_f64(10.0)), Ext::from(Ext::from_f64(30.0)))),
                        _ => r.range(0, 99).to_string(),
                    })
                    .collect();
                s(&fields.join("|"))
            } else {
                s(&format!("{:.2}", f64r(&mut r, Ext::from(-Ext::from_f64(5.0)), Ext::from(Ext::from_f64(20.0)))))
            };
            if e >= 450 && r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                g.bonds_issued[e] = r.range(1, 1500);
                let mat = g.cur_year + cast::<i32, _>(r.range(0, 12));
                let coupon = f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(15.0));
                let cv = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { "cv" } else { "" };
                g.bond_issue_info[e] = s(&format!("{mat}|{}|{cv}|{coupon:5.2}", r.range(0, 3000)));
                g.bond_price[e] = Cur::from_f64(f64r(&mut r, Ext::from_f64(60.0), Ext::from_f64(130.0)));
            }
        }
        if (11..=130).contains(&e) && r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
            g.subprime_secs[e] = r.range(0, 3000);
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
            g.bank_alloc_cash_pct[e] = cast::<i16, _>(r.range(0, 40));
            g.bank_alloc_off[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { 1 } else { 0 };
        }
        if (86..=130).contains(&e) {
            g.ins_reserves[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(10000.0));
            g.insur_uw_profit_ytd[e] = f64r(&mut r, -Ext::from_f64(200.0), Ext::from_f64(200.0));
        }
        if (1581..=1600).contains(&e) {
            g.fund_adviser[e] = cast::<i32, _>(r.range(86, 1600));
            g.fund_fee_pct[e] = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.2), Ext::from_f64(1.0)));
        }
    }
    g
}

fn run(addr: u32, args: &[String], g: &Globals, rng: u32, answers: &[&str], skips: &[(u32, u32)]) -> Option<Original> {
    let hooks = if std::env::var("CF_HOOKS").is_ok() { vec![0x42c72a, 0x41727f, 0x418678] } else { vec![] };
    let opts = Opts { args: args.to_vec(), rng: Some(rng), answers, skips: skips.to_vec(), hooks, ..Opts::default() };
    run_original_with(addr, &opts, g)
}

fn longs(a: &[i32]) -> Vec<String> {
    a.iter().map(|x| x.to_string()).collect()
}

#[track_caller]
fn assert_close(port: Ext, orig: Option<Ext>, what: &str) {
    let o = orig.unwrap_or_else(|| panic!("{what}: no float result"));
    assert!((port - o).abs() <= Ext::from_f64(1e-12) * port.abs().max(o.abs()), "{what}: {port} vs {o}");
}

fn active(g: &Globals, r: &mut Rand, n: usize, lo: i64, hi: i64) -> Vec<i32> {
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

/// Skips for the original: CalcGovBondYields when the quarter is 2 or 4.
fn skips(g: &Globals) -> Vec<(u32, u32)> {
    if g.cur_quarter % 2 == 0 { vec![GOV_YIELDS] } else { vec![] }
}

// ------------------------------------------------------------------------------------- unit tests

#[test]
fn player_income_tax_unit() {
    let mut g = Globals::default();
    g.corp_tax_rate = 30;
    g.cap_gains_tax_rate = 20;
    g.accrued_pl[2] = Ext::from_f64(100.0);
    g.player_cap_gains[2] = Ext::from_f64(50.0);
    let t = calc_player_income_tax(&mut g, 2);
    assert!((t - (Ext::from_f64(50.0) * (cast::<Ext, _>(0.01f32) * Ext::from_f64(20.0)) + Ext::from_f64(100.0) * (Ext::from_f64(35.0) * cast::<Ext, _>(0.01f32)))).abs() < Ext::from_f64(1e-9));
    assert_eq!(g.tmp_ext1, Ext::from_f64(150.0));
    // ordinary loss offsets the gains
    g.accrued_pl[2] = -Ext::from_f64(30.0);
    let t = calc_player_income_tax(&mut g, 2);
    assert!((t - Ext::from_f64(20.0) * (cast::<Ext, _>(0.01f32) * Ext::from_f64(20.0))).abs() < Ext::from_f64(1e-9));
    assert_eq!(g.tmp_ext1, Ext::from_f64(20.0));
}

#[test]
fn loan_principal_due_unit() {
    let mut g = Globals::default();
    let mut ui = Rec::default();
    let mut rng = Rng::default();
    g.exchange_rate = cast::<f64, _>(Ext::from_f64(1.0));
    g.entity_status[200] = 1;
    g.industry_num[200] = 10;
    g.bank_debt[200] = Ext::from_f64(100.0);
    g.cash[200] = Ext::from_f64(10.0);
    g.lending_bank[200] = 20;
    g.credit_rating_num[200] = 5;
    // (11 - 5) * 0.5 % = 3 %
    let d = calc_bank_loan_principal_due(&mut g, &mut ui, &mut rng, 200, -1);
    assert!((d - Ext::from_f64(100.0) * cast::<Ext, _>(cast::<f32, _>(Ext::from_f64(6.0) * cast::<Ext, _>(0.005f32)))).abs() < Ext::from_f64(1e-12), "{d}");
    // banks owe nothing here
    assert_eq!(calc_bank_loan_principal_due(&mut g, &mut ui, &mut rng, 50, -1), Ext::from_f64(0.0));
}

// ----------------------------------------------------------------------------- differential tests

#[test]
fn diff_calc_player_income_tax() {
    for seed in 1..=3u64 {
        let g = world(seed);
        for p in 1..=5 {
            let Some(o) = run(PLAYER_TAX, &longs(&[p]), &g, 1, &[], &[]) else { return };
            let mut port = g.clone();
            let t = calc_player_income_tax(&mut port, p);
            cmp().assert(&port, &o.globals);
            assert_close(t, o.return_float, &format!("CalcPlayerIncomeTax({p})"));
        }
    }
}

#[test]
fn diff_get_etf_adviser_human_status() {
    let mut n_assign = 0;
    for seed in 11..=14u64 {
        let mut g = world(seed);
        let mut r = Rand(seed);
        for (k, etf) in [1581, 1582, 1583, 1584, 1580].into_iter().enumerate() {
            let mut g2 = g.clone();
            g2.entity_status[etf.min(1600)] = 1;
            g2.industry_num[etf] = 71;
            match (k + cast::<usize, _>(seed)) % 4 {
                0 => g2.fund_adviser[etf.max(1581)] = 40, // not a corp adviser: reassigned
                1 => {
                    let a = cast::<i32, _>(r.range(86, 130));
                    g2.fund_adviser[etf.max(1581)] = a;
                    g2.entity_status[a] = 1;
                    g2.industry_num[a] = 2;
                    g2.corp_controller[a] = cast::<i32, _>(r.range(1, 3));
                }
                2 => g2.ctrl_since_year[etf] = 0,
                _ => {}
            }
            let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
            let Some(o) = run(ETF_STATUS, &longs(&[etf]), &g2, rng0, &[], &[]) else { return };
            let mut port = g2.clone();
            let mut ui = Rec::default();
            let mut rng = Rng::from_state(rng0);
            let st = get_etf_adviser_human_status(&mut port, &mut ui, &mut rng, etf);
            cmp().assert(&port, &o.globals);
            assert_eq!(st, cast::<i32, _>(o.return_value), "GetEtfAdviserHumanStatus({etf})");
            assert_eq!(rng.state(), o.rng, "RND");
            if port.fund_adviser[etf.max(1581)] != g2.fund_adviser[etf.max(1581)] {
                n_assign += 1;
            }
        }
        g.cur_player = 2;
    }
    eprintln!("{n_assign} advisers reassigned");
}

#[test]
fn diff_calc_bank_loan_principal_due() {
    let mut nonzero = 0;
    let mut news = 0;
    for seed in 21..=26u64 {
        let mut g = world(seed);
        pow_safe(&mut g, seed, false);
        let mut r = Rand(seed * 3);
        let mut corps = active(&g, &mut r, 8, 86, 1580);
        corps.extend(active(&g, &mut r, 2, 1581, 1583));
        corps.push(cast::<i32, _>(r.range(11, 85)));
        for (k, e) in corps.into_iter().enumerate() {
            let mut g2 = g.clone();
            // big weak borrowers for the covenant branch
            if k % 3 == 1 {
                g2.bank_debt[e] = f64r(&mut r, Ext::from_f64(400.0), Ext::from_f64(3000.0));
                g2.credit_rating_num[e] = cast::<i32, _>(r.range(2, 4));
                g2.rand_seed = 3 * cast::<i32, _>(r.range(0, 1000)) + 1;
                if k % 2 == 0 {
                    g2.news_msg = s("Something happened");
                }
            }
            if k % 4 == 2 {
                g2.credit_rating_num[e] = cast::<i32, _>(r.range(0, 1));
            }
            let rating = [-1, -1, 3, 1, 7][k % 5];
            let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
            let Some(o) = run(LOAN_DUE, &longs(&[e, rating]), &g2, rng0, &[], &[]) else { return };
            let mut port = g2.clone();
            let mut ui = Rec::default();
            let mut rng = Rng::from_state(rng0);
            let d = calc_bank_loan_principal_due(&mut port, &mut ui, &mut rng, e, rating);
            cmp().assert(&port, &o.globals);
            assert_close(d, o.return_float, &format!("CalcBankLoanPrincipalDue({e}, {rating})"));
            assert_eq!(rng.state(), o.rng, "RND");
            nonzero += cast::<usize, _>(d != Ext::from_f64(0.0));
            news += cast::<usize, _>(port.news_msg != g2.news_msg);
        }
    }
    eprintln!("{nonzero} non-zero, {news} news changes");
    assert!(nonzero > 10);
}

/// One run of ShowCashFlowProjection / BankCashFlowProjection on both sides; returns the result.
fn compare_projection(addr: u32, g: &Globals, e: i32, mode: i32, rng0: u32) -> Option<Ext> {
    compare_projection_with(addr, g, e, mode, rng0, cmp())
}

fn compare_projection_with(addr: u32, g: &Globals, e: i32, mode: i32, rng0: u32, comparison: diff::Compare) -> Option<Ext> {
    let __content = wsr_core::content::current();
    let o = run(addr, &longs(&[e, mode]), g, rng0, &[], &skips(g))?;
    let mut port = g.clone();
    let mut ui = Rec::default();
    let mut rng = Rng::from_state(rng0);
    let ret = if addr == SHOW_CFP {
        let x = show_cash_flow_projection(&mut port, &mut ui, &mut rng, e, mode);
        assert_close(x, o.return_float, &format!("ShowCashFlowProjection({e}, {mode})"));
        x
    } else {
        let x = bank_cash_flow_projection(&mut port, &mut ui, &mut rng, e, mode);
        assert_eq!(x, cast::<i32, _>(o.return_value), "BankCashFlowProjection({e}, {mode})");
        cast::<Ext, _>(x)
    };
    let unexpected: Vec<&String> =
        o.log.iter().filter(|l| l.starts_with("PBERR") || l.starts_with("SKIP ") && !l.contains("CalcGovBondYields")).collect();
    assert!(unexpected.is_empty(), "{unexpected:?}");
    assert_reports(&port_reports(&ui), &orig_reports(&o));
    dump_lines(&ui);
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "message boxes: {:?}", ui.msgs);
    comparison.assert(&port, &o.globals);
    assert_eq!(rng.state(), o.rng, "RND, 0x{addr:x}({e}, {mode})");
    if mode < 1 {
        for (_, l) in ui.reports.iter().take(1) {
            eprintln!("--- {e}{__wsr_text_13548}{mode}: {}{__wsr_text_13608}", l.len(),__wsr_text_13548=__content.string(13548), __wsr_text_13608=__content.string(13608));
        }
    }
    Some(ret)
}

#[test]
fn diff_show_cash_flow_projection_corps() {
    let mut statuses = [0usize; 9];
    for seed in 31..=38u64 {
        let mut g = world(seed);
        pow_safe(&mut g, seed, seed % 4 == 3);
        let mut r = Rand(seed * 5);
        let mut corps = active(&g, &mut r, 2, 86, 130);
        corps.extend(active(&g, &mut r, 4, 131, 1580));
        corps.extend(active(&g, &mut r, 1, 1581, 1583));
        for (k, e) in corps.into_iter().enumerate() {
            let mode = [0, 1, 2, 1, 0, 2, 1][(k + cast::<usize, _>(seed)) % 7];
            let mut g2 = g.clone();
            if k % 3 == 2 {
                // a cash squeeze
                g2.cash[e] = f64r(&mut r, -Ext::from_f64(100.0), Ext::from_f64(20.0));
                g2.t_bills[e] = Ext::from_f64(0.0);
                g2.dividend_per_share[e] = f64r(&mut r, Ext::from_f64(0.1), Ext::from_f64(2.0));
            }
            if k % 4 == 1 {
                g2.player_ceo_of[1] = e;
            }
            let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
            let Some(x) = compare_projection(SHOW_CFP, &g2, e, mode, rng0) else { return };
            statuses[cast::<usize, _>(x)] += 1;
        }
    }
    eprintln!("cash statuses (mode >= 1): {statuses:?}");
}

#[test]
fn diff_cash_flow_skips_unordered_dividends() {
    let mut g = world(71);
    pow_safe(&mut g, 71, false);
    for e in [11, 200] {
        g.entity_status[e] = 1;
        g.industry_num[e] = if e == 11 { 1 } else { 10 };
        g.hold_stock_id[(0, e)] = 1;
        for slot in 1..=15 { g.hold_stock_id[(slot, e)] = 0; }
        g.hold_stock_id[(1, e)] = 201;
        g.hold_stock_pct[(1, e)] = 50;
    }
    g.entity_status[201] = 1;
    g.industry_num[201] = 10;
    g.dividend_per_share[201] = Ext::NAN;
    // Both the report totals and the mode-1 status/sentinel affect the valuation bonus.
    for e in [11, 200] {
        for mode in [0, 1] {
            compare_projection_with(SHOW_CFP, &g, e, mode, 123456, cmp().allow_nonfinite_from(&g));
        }
    }
}

#[test]
fn diff_show_cash_flow_projection_dispatch() {
    // players (always the current player) and banks through the dispatcher, inactive / invalid ids
    for seed in 41..=43u64 {
        let mut g = world(seed);
        pow_safe(&mut g, seed, false);
        let mut r = Rand(seed);
        let bank = active(&g, &mut r, 1, 11, 85)[0];
        let inactive = (11..=1600).find(|&e| g.entity_status[e] != 1).unwrap();
        for (k, (e, mode)) in [(1, 0), (bank, 1), (inactive, 0), (g.num_players + 1, 0), (bank, 0)].into_iter().enumerate() {
            let _ = k;
            let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
            if compare_projection(SHOW_CFP, &g, e, mode, rng0).is_none() {
                return;
            }
        }
    }
}

#[test]
fn diff_bank_cash_flow_projection_player() {
    // a player id: MarketCap(1) raises error 9 at once (ShowRuntimeError, result 0)
    let g = world(57);
    for mode in [0, 1] {
        let Some(o) = run(BANK_CFP, &longs(&[1, mode]), &g, 1, &[], &[]) else { return };
        assert!(o.log.iter().any(|l| l.starts_with("PBERR 9 in 0x6a99f7")), "{:?}", o.log);
        let mut port = g.clone();
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(1);
        assert_eq!(bank_cash_flow_projection(&mut port, &mut ui, &mut rng, 1, mode), cast::<i32, _>(o.return_value));
        assert!(ui.msgs.is_empty() && ui.reports.is_empty());
        cmp().ignore("ErrorLogText").assert(&port, &o.globals);
    }
}

#[test]
fn diff_bank_cash_flow_projection() {
    let mut statuses = [0usize; 9];
    for seed in 51..=56u64 {
        let mut g = world(seed);
        pow_safe(&mut g, seed, seed % 3 == 2);
        let mut r = Rand(seed * 11);
        let banks = active(&g, &mut r, 3, 11, 85);
        for (k, b) in banks.into_iter().enumerate() {
            let mode = [0, 1, 2][(k + cast::<usize, _>(seed)) % 3];
            let mut g2 = g.clone();
            if k == 1 {
                g2.cash[b] = f64r(&mut r, -Ext::from_f64(3000.0), Ext::from_f64(0.0));
                g2.t_bills[b] = Ext::from_f64(0.0);
                g2.bank_debt[b] = [Ext::from_f64(2000.0), f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(1999.0))][cast::<usize, _>(seed % 2)];
            }
            if k == 2 && seed % 2 == 0 {
                g2.ent_misc_data[b] = s("1.5"); // no quarter reported yet
            }
            let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
            let Some(x) = compare_projection(BANK_CFP, &g2, b, mode, rng0) else { return };
            statuses[cast::<usize, _>(x)] += 1;
        }
    }
    eprintln!("bank cash statuses (mode >= 1): {statuses:?}");
}

#[test]
fn diff_player_cash_flow_projection() {
    let __content = wsr_core::content::current();
    for seed in 61..=66u64 {
        let mut g = world(seed);
        pow_safe(&mut g, seed, false);
        let mut r = Rand(seed);
        // portfolio totals of the last CalcEntityFinancials
        g.tmp_gov_bonds_value = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(500.0)) } else { Ext::from_f64(0.0) };
        g.tmp_corp_bonds_value = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(500.0)) } else { Ext::from_f64(0.0) };
        g.tmp_subsid_stock_value = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(50.0));
        g.phys_commod_value = Ext::from_f64(0.0);
        g.net_worth = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(5000.0));
        g.total_assets = [Ext::from_f64(500.0), Ext::from_f64(200_000.0)][cast::<usize, _>(seed % 2)];
        g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
        let p = g.cur_player;
        g.entity_status[p] = 1;
        if seed == 64 {
            // minimal assets: no statement
            g.tmp_gov_bonds_value = Ext::from_f64(0.0);
            g.tmp_corp_bonds_value = Ext::from_f64(0.0);
            g.tmp_subsid_stock_value = Ext::from_f64(0.0);
            g.bank_debt[p] = Ext::from_f64(0.0);
            g.cash[p] = Ext::from_f64(5.0);
            g.t_bills[p] = Ext::from_f64(0.0);
        }
        if seed == 65 {
            g.entity_status[p] = 2;
        }
        for slot in 1..=15 {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                let h = cast::<i32, _>(r.range(450, 1600));
                g.hold_bond_id[(slot, p)] = h;
                g.credit_rating_num[h] = cast::<i32, _>(r.range(2, 10));
                g.hold_bond_face[(slot, p)] = r.range(1, 200);
                g.bond_hold_cost[(slot, p)] = cast::<Ext, _>(g.hold_bond_face[(slot, p)]) * f64r(&mut r, Ext::from_f64(0.7), Ext::from_f64(1.3));
                let mat = g.cur_year + cast::<i32, _>(r.range(-1, 4));
                let cv = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { "cv" } else { "" };
                g.bond_issue_info[h] = s(&format!("{mat}|{}|{cv}|{:5.2}", r.range(0, 3000), f32r(&mut r, Ext::from(Ext::from_f64(2.0)), Ext::from(Ext::from_f64(12.0)))));
                g.bond_price[h] = Cur::from_f64(f64r(&mut r, Ext::from_f64(80.0), Ext::from_f64(130.0)));
            }
        }
        let Some(o) = run(PLAYER_CFP, &longs(&[p, 0]), &g, 1, &[], &[]) else { return };
        let mut port = g.clone();
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(1);
        let ret = player_cash_flow_projection(&mut port, &mut ui, &mut rng, p, 0);
        assert_eq!(ret, cast::<i32, _>(o.return_value));
        assert_eq!(rng.state(), o.rng, "RND state");
        assert_reports(&port_reports(&ui), &orig_reports(&o));
        dump_lines(&ui);
        assert_eq!(ui.msgs.len(), orig_msgs(&o), "message boxes: {:?} / {:?}", ui.msgs, o.log);
        cmp().assert(&port, &o.globals);
        if let Some((_, l)) = ui.reports.first() {
            eprintln!("seed {seed}: {}{__wsr_text_13608}", l.len(),__wsr_text_13608=__content.string(13608));
        }
    }
}

#[test]
fn diff_cash_flow_alert() {
    let __content = wsr_core::content::current();
    let mut alerts = 0;
    let mut selected = 0;
    for seed in 71..=76u64 {
        let mut g = world(seed);
        pow_safe(&mut g, seed, false);
        let mut r = Rand(seed);
        let mut corp = g.sim_co_count + 1000;
        if corp > 1600 {
            corp -= 1600;
        }
        g.entity_status[corp] = 1;
        // deep trouble: negative cash and no credit
        g.cash[corp] = f64r(&mut r, -Ext::from_f64(5000.0), -Ext::from_f64(2000.0));
        g.t_bills[corp] = Ext::from_f64(0.0);
        g.bank_debt[corp] = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(3000.0));
        g.business_assets[corp] = Ext::from_f64(5.0);
        g.working_capital[corp] = Ext::from_f64(0.0);
        g.market_cap[corp] = Ext::from_f64(1.0);
        for slot in 0..=15 {
            g.hold_stock_id[(slot, corp)] = 0;
        }
        g.credit_rating_num[corp] = 0;
        // SelEntity > 0 makes ShowReportDialog run UpdateMarketDisplay (market tick)
        g.sel_entity = if seed % 2 == 1 { corp } else { 0 };
        g.last_sel_entity = if seed % 2 == 0 { corp } else { 0 };
        g.cur_suppress_cash_warn = 0;
        if seed == 75 {
            g.cash_alert_corp2 = corp;
        }
        let ctrl = seed % 3 == 0;
        if ctrl {
            g.corp_controller[corp] = g.cur_player;
        }
        if seed == 72 {
            // a 60% stake keeps the player in control when the report recomputes it
            let p = g.cur_player;
            for h in 1..=1600 {
                for slot in 1..=15 {
                    if cast::<i32, _>(g.hold_stock_id[(slot, h)]) == corp {
                        g.hold_stock_id[(slot, h)] = 0;
                        g.hold_stock_pct[(slot, h)] = 0;
                    }
                }
            }
            g.hold_stock_id[(15, p)] = cast::<i16, _>(corp);
            g.hold_stock_pct[(15, p)] = 60;
        }
        let answers: &[&str] = match seed % 3 {
            // seed 72: make it the active entity (SelectTargetCorp)
            0 if seed == 72 => &["IDYES", "IDYES"],
            0 => &["IDYES", "IDNO"],
            1 => &["IDYES"],
            _ => &["IDNO"],
        };
        let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
        let Some(o) = run(ALERT, &[], &g, rng0, answers, &[]) else { return };
        let mut port = g.clone();
        let mut ui = Rec::default();
        ui.answers = answers.iter().map(|a| if *a == "IDYES" { 6 } else { 7 }).collect();
        let mut rng = Rng::from_state(rng0);
        cash_flow_alert(&mut port, &mut ui, &mut rng, 0);
        if std::env::var("CF_DEBUG").is_ok() {
            eprintln!("orig log: {:#?}\nport msgs: {:#?}", o.log.iter().filter(|l| !l.starts_with("API") || l.contains("qm.exe")).collect::<Vec<_>>(), ui.msgs);
            for (t, l) in orig_reports(&o) { eprintln!("{t:?}: {l:#?}"); }
        }
        assert_reports(&port_reports(&ui), &orig_reports(&o));
        assert_eq!(ui.msgs.len(), orig_msgs(&o), "message boxes: {:?} / {:?}", ui.msgs, o.log);
        cmp().assert(&port, &o.globals);
        assert_eq!(rng.state(), o.rng, "RND");
        alerts += cast::<usize, _>(port.cash_alert_corp1 == corp);
        eprintln!("seed {seed}: corp {corp} msgs {:?}{__wsr_text_13609}{} sel {} ctl {} cur {}", ui.msgs.iter().map(|m| &m.2).collect::<Vec<_>>(), ui.reports.len(), port.sel_entity, port.corp_controller[corp], port.cur_player,__wsr_text_13609=__content.string(13609));
        if answers.len() == 2 && answers[1] == "IDYES" && ui.msgs.iter().any(|m| m.2.starts_with(__content.string(13610))) {
            selected += 1;
        }
    }
    assert!(alerts > 0);
    assert!(selected > 0, "SelectTargetCorp path not hit");
}

/// Targeted states for report branches the random worlds rarely reach: goodwill amortization, capital
/// taxes, prepaid-tax refunds, loss-making 80 % subsidiaries, maturing bonds, a bond issue due, fund
/// distributions and adviser fees (insurer), banks with an excess bad-debt reserve, net corporate
/// borrowing and loans to be sold.
#[test]
fn diff_cash_flow_projection_features() {
    for seed in 81..=86u64 {
        let odd = seed % 2 == 1;
        let mut g = world(seed);
        pow_safe(&mut g, seed, odd);
        let mut r = Rand(seed * 13);
        let cy = g.cur_year;
        // operating corp 1200 with a loss-making sub 900 and bonds
        let (e, sub, issuer) = (1200, 900, 1300);
        for x in [e, sub, issuer] {
            g.entity_status[x] = 1;
            g.industry_num[x] = 3;
            g.country_idx[x] = 1;
        }
        g.carbon_tax_rate = 20;
        g.goodwill[e] = Ext::from_f64(50.0);
        g.business_assets[e] = Ext::from_f64(1000.0);
        g.tax_loss_carry[e] = Ext::from_f64(40.0);
        g.profit_margin[e] = -25;
        g.ent_misc_data[e] = s(&format!("{}|-40.5", ["1"; 29].join("|")));
        for slot in 0..=15 {
            g.hold_stock_id[(slot, e)] = 0;
            g.hold_bond_id[(slot, e)] = 0;
        }
        g.hold_stock_id[(0, e)] = 2;
        g.hold_stock_id[(1, e)] = cast::<i16, _>(sub);
        g.hold_stock_pct[(1, e)] = 90;
        g.hold_stock_id[(2, e)] = 1581;
        g.hold_stock_pct[(2, e)] = 5;
        g.proj_qtr_earnings[sub] = -Ext::from_f64(20.0);
        g.accrued_pl[sub] = -Ext::from_f64(5.0);
        g.consol_tax_inc_ytd[sub] = Ext::from_f64(0.0);
        g.extraordinary_pl[sub] = 0;
        g.consol_tax_inc_ytd[e] = Ext::from_f64(0.0);
        g.credit_rating_num[issuer] = 6;
        g.bond_issue_info[issuer] = s(&format!("{}|0||{:5.2}", cy, Ext::from_f64(6.5)));
        g.hold_bond_id[(3, e)] = issuer;
        g.hold_bond_face[(3, e)] = 120;
        g.bond_hold_cost[(3, e)] = Ext::from_f64(118.5);
        g.short_bond_desc = s(&format!("{} SHORT T-NOTE {:5.2}", cy + 1, Ext::from_f64(4.25)));
        g.short_gov_bond_face[e] = 200;
        g.st_gov_bond_cost[e] = Ext::from_f64(199.0);
        if !odd {
            g.bonds_issued[e] = 300;
            g.bond_issue_info[e] = s(&format!("{}|0||{:5.2}", cy, Ext::from_f64(7.0)));
            g.bond_price[e] = Cur::from_f64(Ext::from_f64(98.0));
        }
        // advances from player 1, an interest rate swap (fixed 5 %, P = pay fixed?) with corp 1300
        g.player_advances[(1, e)] = Ext::from_f64(50.0);
        g.swap_contracts[1] = s(&format!("{e}|{issuer}|5|P|{}.1|{}.4|200", cy - 1, cy + 3));
        g.swap_contracts[0] = s("1");
        // a distressed issuer: bond interest accrues
        let d = 1250;
        g.entity_status[d] = 1;
        g.industry_num[d] = 5;
        g.cash[d] = -Ext::from_f64(3000.0);
        g.t_bills[d] = Ext::from_f64(0.0);
        g.bank_debt[d] = Ext::from_f64(4000.0);
        g.business_assets[d] = Ext::from_f64(10.0);
        g.market_cap[d] = Ext::from_f64(0.6);
        if !odd {
            g.bonds_issued[d] = 500;
            g.bond_issue_info[d] = s(&format!("{}|0||{:5.2}", cy + 5, Ext::from_f64(11.0)));
            g.bond_price[d] = Cur::from_f64(Ext::from_f64(40.0));
        }
        // funds: 1581 rich in T-bills with capital gains, 1582 advised by insurer 100
        for f in [1581, 1582] {
            g.entity_status[f] = 1;
            g.industry_num[f] = 71;
            g.t_bills[f] = Ext::from_f64(5000.0);
            g.extraordinary_pl[f] = 25;
            g.fund_fee_pct[f] = cast::<f32, _>(Ext::from_f64(0.5));
            g.market_cap[f] = Ext::from_f64(400.0);
        }
        g.fund_adviser[1582] = 100;
        g.entity_status[100] = 1;
        g.corp_controller[100] = 0;
        // bank 30: big reserve, borrowers that keep borrowing, little cash
        let b = 30;
        g.entity_status[b] = 1;
        g.bad_debt_reserve[b] = Ext::from_f64(20000.0);
        g.bank_alloc_off[b] = 0;
        g.bank_alloc_cash_pct[b] = 30;
        g.credit_frozen[b] = 0;
        g.cash[b] = Ext::from_f64(10.0);
        g.t_bills[b] = Ext::from_f64(0.0);
        g.bank_consumer_loans[b] = 20000;
        g.bank_mortgage_loans[b] = 20000;
        for x in 86..=1600 {
            if cast::<i32, _>(g.lending_bank[x]) == b {
                let f: Vec<String> = (1..=30).map(|k| if k == 13 { format!("-{}", r.range(10, 60)) } else { "1".into() }).collect();
                g.ent_misc_data[x] = s(&f.join("|"));
                g.credit_frozen[x] = 0;
            }
        }
        if odd {
            calc_gov_bond_yields(&mut g); // the original's is skipped
        }
        for (k, (addr, x)) in [(SHOW_CFP, e), (SHOW_CFP, 1581), (SHOW_CFP, 1582), (SHOW_CFP, 100), (BANK_CFP, b), (SHOW_CFP, d)]
            .into_iter()
            .enumerate()
        {
            let mode = if k == 5 { 0 } else { [0, 0, 1][(k + cast::<usize, _>(seed)) % 3] };
            let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
            if compare_projection(addr, &g, x, mode, rng0).is_none() {
                return;
            }
        }
        // mode 2 keeps the preset ratings (no CalcEntityFinancials): bond interest and player-advance
        // interest accrue unpaid for ratings 0/1
        g.credit_rating_num[d] = cast::<i32, _>(seed % 2);
        g.tmp_player_advances = Ext::from_f64(30.0);
        let rng0 = cast::<u32, _>(r.range(1, 0x7fff_ffff));
        if compare_projection(SHOW_CFP, &g, d, 2, rng0).is_none() {
            return;
        }
    }
}
