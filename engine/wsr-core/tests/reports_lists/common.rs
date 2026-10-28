//! Shared helpers of the reports_lists tests (recording Ui, report comparison, random state builders).
#![allow(dead_code)]
use wsr_core::pb::real::{Ext, cast};


use super::diff::{Compare, Opts, Original, Rand, run_original_with};
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::numeric::Cur;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

/// Records every report, message box and news headline.
#[derive(Default)]
pub struct Rec {
    pub reports: Vec<(PbStr, Vec<PbStr>)>,
    pub msgs: Vec<String>,
    /// Titles of the message boxes in `msgs`.
    pub titles: Vec<String>,
    pub news: Vec<String>,
    /// Popped (from the end) for successive reports; default `Ok`.
    pub actions: Vec<ReportAction>,
    /// Popped (from the front) for successive message boxes; default YES/OK.
    pub answers: std::collections::VecDeque<i32>,
}

impl Ui for Rec {
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 {
        self.msgs.push(text.to_display());
        self.titles.push(title.to_display());
        if let Some(a) = self.answers.pop_front() {
            return a;
        }
        if style & 0xf == 3 || style & 0xf == 4 { 6 } else { 1 }
    }
    fn inputbox(&mut self, _prompt: &PbStr, _title: &PbStr, default: &PbStr) -> PbStr {
        default.clone()
    }
    fn report(&mut self, title: &PbStr, lines: &[PbStr], _yes_no: bool) -> ReportAction {
        self.reports.push((title.clone(), lines.to_vec()));
        self.actions.pop().unwrap_or(ReportAction::Ok)
    }
    fn news(&mut self, headline: &PbStr) {
        self.news.push(headline.to_display());
    }
}

pub fn st(x: &str) -> PbStr {
    PbStr::from(x)
}

pub fn word(r: &mut Rand, n: i64) -> PbStr {
    PbStr((0..r.range(1, n)).map(|_| b'A' + cast::<u8, _>(r.range(0, 25))).collect())
}

/// Report dialogs of the original as (title, listbox 301 items).
pub fn orig_reports(o: &Original) -> Vec<(PbStr, Vec<PbStr>)> {
    o.dialogs
        .iter()
        .filter_map(|d| d.control(301).map(|c| (d.title.clone(), c.items.clone())))
        .collect()
}

/// The port's reports as the original's listbox shows them: ShowReportDialog adds the `@END` line as " "
/// (`end_found[k]` = report k had an `@END` line; missing entries count as true).
pub fn port_reports(ui: &Rec, end_found: &[bool]) -> Vec<(PbStr, Vec<PbStr>)> {
    ui.reports
        .iter()
        .zip(end_found.iter().chain(std::iter::repeat(&true)))
        .map(|((t, l), &e)| {
            let mut l = l.clone();
            if e {
                l.push(st(" "));
            }
            (t.clone(), l)
        })
        .collect()
}

#[track_caller]
pub fn assert_reports(port: &[(PbStr, Vec<PbStr>)], orig: &[(PbStr, Vec<PbStr>)]) {
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

/// Number of messages the original showed: MSGBOXes and ShowQuickMessage popups (which run `qm.exe` on
/// TMP_QM.DAT; the port shows those through `Ui::msgbox` too).
pub fn orig_msgs(o: &Original) -> usize {
    o.log.iter().filter(|l| l.starts_with("MSGBOX") || l.contains("CreateProcessA(0x0, 'qm.exe")).count()
}

/// UI-only state the ports don't model.
pub fn cmp() -> Compare {
    Compare::default().ignore("TickerPauseUntil").ignore("hReportDlg")
}

/// Random names, symbols and countries for all entities.
pub fn names(g: &mut Globals, r: &mut Rand) {
    let __content = wsr_core::content::current();
    for i in 0..=55 {
        g.country_name[i] = word(r, 12);
    }
    for i in 1..=1600 {
        g.ent_name[i] = PbStr::from(format!("{} {}{__wsr_text_13676}", word(r, 8).to_display(), i,__wsr_text_13676=__content.string(13676)).as_str());
        g.ent_symbol[i] = word(r, 5);
        g.country_idx[i] = cast::<i32, _>(r.range(0, 55));
    }
}

/// Run the original at `addr` with harness arg specs `args`, RND state `rng` and MSGBOX `answers`.
pub fn run(addr: u32, args: &[&str], g: &Globals, rng: u32, answers: &[&str]) -> Option<Original> {
    let opts = Opts { args: args.iter().map(|a| a.to_string()).collect(), rng: Some(rng), answers, ..Opts::default() };
    run_original_with(addr, &opts, g)
}

// ------------------------------------------------------------------ plausible world (from tests/valuation.rs)

pub fn f64r(r: &mut Rand, lo: Ext, hi: Ext) -> Ext {
    lo + (hi - lo) * cast::<Ext, _>(r.range(0, 1_000_000_000)) / Ext::from_f64(1e9)
}

pub fn f32r(r: &mut Rand, lo: Ext, hi: Ext) -> f32 {
    cast::<f32, _>(f64r(r, lo, hi))
}

/// A plausible mid-game world (players 1..NumPlayers, banks 11..85, insurers 86..130, operating corps
/// 131..1580, funds 1581..1583) with balance sheets, earnings history, industry data, the economy and
/// 60-month price histories of every series.
pub fn world(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut r = Rand(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut g = Globals::default();
    g.world_generated = 1;
    g.game_end_flag = st("N");
    g.num_players = cast::<i32, _>(r.range(1, 4));
    g.difficulty_level = cast::<i32, _>(r.range(1, 4));
    g.cur_year = cast::<i32, _>(r.range(2000, 2030));
    g.start_year = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { g.cur_year } else { g.cur_year - cast::<i32, _>(r.range(1, 10)) };
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
    g.exchange_rate = cast::<f64, _>(if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { Ext::from_f64(1.0) } else { f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(2.0)) });
    g.loan_limit_multiple = cast::<f64, _>((cast::<Ext, _>(r.range(20, 35))) * Ext::from_f64(0.1));
    g.corp_tax_rate = cast::<i32, _>(r.range(15, 45));
    g.bank_cd_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(9.0)));
    g.long_bond_desc = st(&format!("{} LONG T-BOND {:5.2}", g.cur_year + cast::<i32, _>(r.range(8, 30)), f32r(&mut r, Ext::from(Ext::from_f64(2.0)), Ext::from(Ext::from_f64(12.0)))));
    g.short_bond_desc = st(&format!("{} SHORT T-NOTE {:5.2}", g.cur_year + cast::<i32, _>(r.range(0, 9)), f32r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(10.0)))));
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
    g.million_prefix_uc = st(["M", "B", "M"][cast::<usize, _>(r.range(0, 2))]);
    g.rand_seed = cast::<i32, _>(r.range(0, 32600));
    g.game_in_progress = if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) { 1 } else { 0 };
    g.active_corp_count = cast::<i32, _>(r.range(700, 1300));
    g.cur_player = 1;
    for p in 1..=5 {
        g.player_name[p] = if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) { st("...OUT...") } else { st(&format!("{__wsr_text_13556}{p}",__wsr_text_13556=__content.string(13556))) };
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
        g.ent_name[e] = st(&format!("{__wsr_text_13758}{e}",__wsr_text_13758=__content.string(13758)));
        g.ent_symbol[e] = st(["XYZ", "BKH", "GOOG", "AAPL", "AMZN", "ABC", "QRS"][cast::<usize, _>(r.range(0, 6))]);
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
                _ => {
                    if r.chance(cast::<f64, _>(Ext::from_f64(0.04))) {
                        [69, 70][cast::<usize, _>(r.range(0, 1))]
                    } else {
                        cast::<i16, _>(r.range(3, 40))
                    }
                }
            };
            g.market_cap[e] = f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(300.0));
            g.shares_outstanding[e] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(50.0));
            if r.chance(cast::<f64, _>(Ext::from_f64(0.03))) {
                g.shares_outstanding[e] = Ext::from_f64(0.0);
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.03))) {
                g.market_cap[e] = -Ext::from_f64(1.0);
            }
            g.dividend_per_share[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(3.0)) } else { Ext::from_f64(0.0) };
            g.tax_loss_carry[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { f64r(&mut r, -Ext::from_f64(100.0), Ext::from_f64(0.0)) } else { Ext::from_f64(0.0) };
            g.accrued_pl[e] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(200.0));
            g.consol_tax_inc_ytd[e] = f64r(&mut r, -Ext::from_f64(30.0), Ext::from_f64(100.0));
            g.tax_credits[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(20.0)) } else { Ext::from_f64(0.0) };
            g.contingency_reserve[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(50.0)) } else { Ext::from_f64(0.0) };
            let zero = |r: &mut Rand| r.chance(cast::<f64, _>(Ext::from_f64(0.08)));
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
            g.ent_misc_data[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) {
                let n = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { 30 } else { 12 };
                let fields: Vec<String> = (1..=n)
                    .map(|k| match k {
                        1 | 3 => format!("{:.2}", f64r(&mut r, Ext::from(-Ext::from_f64(10.0)), Ext::from(Ext::from_f64(40.0)))),
                        12 => format!("{:.3}", f64r(&mut r, Ext::from(Ext::from_f64(0.0)), Ext::from(Ext::from_f64(5.0)))),
                        29 => r.range(0, 25).to_string(),
                        _ => r.range(0, 99).to_string(),
                    })
                    .collect();
                st(&fields.join("|"))
            } else {
                st(&format!("{:.2}", f64r(&mut r, Ext::from(-Ext::from_f64(5.0)), Ext::from(Ext::from_f64(20.0)))))
            };
            if e >= g.sim_co_count && r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                g.bonds_issued[e] = r.range(1, 1500);
                let mat = g.cur_year + cast::<i32, _>(r.range(1, 25));
                let coupon = f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(15.0));
                g.bond_issue_info[e] = st(&format!("{mat}|0||{coupon:5.2}"));
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
        if r.chance(cast::<f64, _>(Ext::from_f64(0.05))) {
            continue; // no history yet
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
            if r.chance(cast::<f64, _>(Ext::from_f64(0.05))) {
                // flat month
            } else {
                v *= Ext::from_f64(1.0) - trend + f64r(&mut r, -Ext::from_f64(0.05), Ext::from_f64(0.05));
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
