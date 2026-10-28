//! Shared helpers of the corp_finance tests (copied from tests/sim_entity/common.rs and extended with
//! INPUTBOX / choose answers): recording Ui, a plausible random world, running the original.
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
    pub titles: Vec<String>,
    pub news: Vec<String>,
    /// Popped (from the front) for successive message boxes; default YES/OK.
    pub answers: std::collections::VecDeque<i32>,
    /// Popped for successive INPUTBOX$ calls; default: the box's default text.
    pub inputs: std::collections::VecDeque<PbStr>,
    pub prompts: Vec<String>,
    /// Popped for successive `choose` dialogs (`None` = cancel); default cancel.
    pub choices: std::collections::VecDeque<Option<usize>>,
    /// Popped for successive reports; default OK.
    pub report_actions: std::collections::VecDeque<ReportAction>,
}

impl Ui for Rec {
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 {
        self.msgs.push(text.to_display());
        self.titles.push(title.to_display());
        // ShowQuickMessage pop-ups (style 0, no title) run qm.exe in the original and take no answer
        if !(style == 0 && title.is_empty())
            && let Some(a) = self.answers.pop_front()
        {
            return a;
        }
        if style & 0xf == 3 || style & 0xf == 4 { 6 } else { 1 }
    }
    fn inputbox(&mut self, prompt: &PbStr, _title: &PbStr, default: &PbStr) -> PbStr {
        self.prompts.push(prompt.to_display());
        self.inputs.pop_front().unwrap_or_else(|| default.clone())
    }
    fn report(&mut self, title: &PbStr, lines: &[PbStr], _yes_no: bool) -> ReportAction {
        self.reports.push((title.clone(), lines.to_vec()));
        self.report_actions.pop_front().unwrap_or(ReportAction::Ok)
    }
    fn choose(&mut self, _title: &PbStr, _prompt: &[PbStr], _options: &[PbStr]) -> Option<usize> {
        self.choices.pop_front().flatten()
    }
    fn news(&mut self, headline: &PbStr) {
        self.news.push(headline.to_display());
    }
}

/// MSGBOX calls and ShowQuickMessage pop-ups (qm.exe) in the original's log.
pub fn orig_msgs(o: &Original) -> usize {
    o.log.iter().filter(|l| l.starts_with("MSGBOX") || l.contains("CreateProcessA(0x0, 'qm.exe")).count()
}

pub fn st(x: &str) -> PbStr {
    PbStr::from(x)
}

pub fn word(r: &mut Rand, n: i64) -> PbStr {
    PbStr((0..r.range(1, n)).map(|_| b'A' + cast::<u8, _>(r.range(0, 25))).collect())
}

/// UI-only state the ports don't model.
pub fn cmp() -> Compare {
    Compare::default().ignore("TickerPauseUntil").ignore("hReportDlg")
}

/// Run the original at `addr` with harness arg specs `args`, RND state `rng`, MSGBOX `answers` and
/// routines to skip `(address, stdcall dword args)`.
pub fn run(addr: u32, args: &[&str], g: &Globals, rng: u32, answers: &[&str], skips: &[(u32, u32)]) -> Option<Original> {
    run_hooked(addr, args, g, rng, answers, skips, &[])
}

/// [`run`] that also logs the calls of the routines `hooks` (`CALL Name(...)`). `WSR_HOOKS=0x..,..` adds
/// hooks, `WSR_LOG=1` prints the original's log (debugging).
pub fn run_hooked(
    addr: u32,
    args: &[&str],
    g: &Globals,
    rng: u32,
    answers: &[&str],
    skips: &[(u32, u32)],
    hooks: &[u32],
) -> Option<Original> {
    let mut hooks = hooks.to_vec();
    if let Ok(v) = std::env::var("WSR_HOOKS") {
        hooks.extend(v.split(',').filter_map(|a| u32::from_str_radix(a.trim().trim_start_matches("0x"), 16).ok()));
    }
    let opts = Opts {
        args: args.iter().map(|a| a.to_string()).collect(),
        rng: Some(rng),
        answers,
        skips: skips.to_vec(),
        hooks,
        ..Opts::default()
    };
    let o = run_original_with(addr, &opts, g)?;
    if std::env::var_os("WSR_LOG").is_some() {
        for l in o.log.iter().filter(|l| !l.starts_with("API")) {
            eprintln!("orig: {l}");
        }
    }
    Some(o)
}

pub fn f64r(r: &mut Rand, lo: Ext, hi: Ext) -> Ext {
    lo + (hi - lo) * cast::<Ext, _>(r.range(0, 1_000_000_000)) / Ext::from_f64(1e9)
}

pub fn f32r(r: &mut Rand, lo: Ext, hi: Ext) -> f32 {
    cast::<f32, _>(f64r(r, lo, hi))
}

/// A plausible mid-game world (players 1..NumPlayers, banks 11..85, insurers 86..130, operating corps
/// 131..1580, funds 1581..1583) with balance sheets, holdings, bonds, loans, earnings data, industry
/// data, the economy and price histories.
pub fn world(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut r = Rand(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut g = Globals::default();
    g.world_generated = 1;
    g.game_end_flag = st("N");
    g.num_players = cast::<i32, _>(r.range(1, 4));
    g.difficulty_level = cast::<i32, _>(r.range(1, 4));
    g.cur_year = cast::<i32, _>(r.range(2000, 2030));
    g.start_year = if r.chance(Ext::from_f64(0.2)) { g.cur_year } else { g.cur_year - cast::<i32, _>(r.range(1, 10)) };
    g.game_end_year = g.start_year + 35;
    g.cur_quarter = cast::<i32, _>(r.range(1, 4));
    g.sim_co_count = cast::<i32, _>(r.range(10, 1500));
    g.base_interest_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(12.0)));
    g.prime_rate = cast::<f32, _>(g.base_interest_rate + f32r(&mut r, Ext::from_f64(0.0), Ext::from_f64(3.0)));
    g.long_bond_yield = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(12.0)));
    g.short_bond_yield = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(10.0)));
    g.sofr_rate = cast::<f32, _>(g.base_interest_rate + f32r(&mut r, -Ext::from_f64(0.5), Ext::from_f64(0.5)));
    g.t_bill_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.5), Ext::from_f64(8.0)));
    g.gdp_growth = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(8.0), Ext::from_f64(8.0)));
    g.gdp_growth_target = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(5.0), Ext::from_f64(8.0)));
    g.gdp_level = r.range(5_000_000, 30_000_000);
    g.monetary_policy = cast::<i16, _>(r.range(0, 2));
    g.monetary_stance = cast::<i32, _>(r.range(-1, 2));
    g.exchange_rate = cast::<f64, _>(if r.chance(Ext::from_f64(0.5)) { Ext::from_f64(1.0) } else { f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(2.0)) });
    g.loan_limit_multiple = cast::<f64, _>((cast::<Ext, _>(r.range(20, 35))) * Ext::from_f64(0.1));
    g.corp_tax_rate = cast::<i32, _>(r.range(15, 45));
    g.cap_gains_tax_rate = cast::<i32, _>(r.range(0, 30));
    g.bank_cd_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(9.0)));
    g.long_bond_desc = st(&format!("{} LONG T-BOND {:5.2}", g.cur_year + cast::<i32, _>(r.range(8, 30)), f32r(&mut r, Ext::from(Ext::from_f64(2.0)), Ext::from(Ext::from_f64(12.0)))));
    g.short_bond_desc = st(&format!("{} SHORT T-NOTE {:5.2}", g.cur_year + cast::<i32, _>(r.range(0, 9)), f32r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(10.0)))));
    g.long_bond_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(70.0), Ext::from_f64(130.0)));
    g.short_bond_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(85.0), Ext::from_f64(110.0)));
    g.oil_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.3), Ext::from_f64(3.0)));
    g.oil_target_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.3), Ext::from_f64(3.0)));
    g.gold_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(300.0), Ext::from_f64(2500.0)));
    g.silver_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(4.0), Ext::from_f64(40.0)));
    g.wheat_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(12.0)));
    g.corn_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(9.0)));
    g.stock_index = f64r(&mut r, Ext::from_f64(500.0), Ext::from_f64(20000.0));
    g.stock_index_divisor = f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(5.0));
    g.market_valuation = cast::<f32, _>([Ext::from_f64(0.8), Ext::from_f64(1.0), Ext::from_f64(1.2), Ext::from_f64(0.9)][cast::<usize, _>(r.range(0, 3))]);
    g.market_val_target = cast::<f64, _>(cast::<Ext, _>(g.market_valuation));
    g.econ_cycle_counter = cast::<i16, _>(r.range(0, 30));
    g.hot_stock_corp = cast::<i16, _>(r.range(131, 1580));
    g.currency_symbol = st("$");
    g.currency_name = st(__content.string(6643));
    g.million_prefix_uc = st("M");
    g.million_prefix_lc = st("m");
    g.rand_seed = cast::<i32, _>(r.range(0, 32000));
    g.game_in_progress = 1;
    g.active_corp_count = cast::<i32, _>(r.range(700, 1300));
    g.cur_player = 1;
    g.max_stock_offering_amt = r.range(500, 5000);
    g.max_bond_issue = r.range(500, 5000);
    g.superfund_end_year = g.cur_year + cast::<i32, _>(r.range(-2, 5));
    for p in 1..=5 {
        g.player_name[p] = st(&format!("Player Number {p}"));
        g.is_computer_player[p] = if p > 1 && r.chance(Ext::from_f64(0.5)) { 1 } else { 0 };
    }
    for i in 0..=55 {
        g.country_name[i] = word(&mut r, 12);
    }
    for ind in 1..=71 {
        g.ind_name[ind] = st(&format!("{__wsr_text_13675}{ind}",__wsr_text_13675=__content.string(13675)));
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
        g.ent_name[e] = PbStr::from(format!("{} {}{__wsr_text_13676}", word(&mut r, 8).to_display(), e,__wsr_text_13676=__content.string(13676)).as_str());
        g.ent_symbol[e] = word(&mut r, 5);
        g.cash[e] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(3000.0));
        g.t_bills[e] = if r.chance(Ext::from_f64(0.5)) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(1000.0)) } else { Ext::from_f64(0.0) };
        g.bank_debt[e] = if r.chance(Ext::from_f64(0.6)) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(2000.0)) } else { Ext::from_f64(0.0) };
        g.lending_bank[e] = cast::<i16, _>(r.range(11, 85));
        g.credit_rating_num[e] = cast::<i32, _>(r.range(0, 10));
        g.corp_controller[e] = match r.range(0, 5) {
            0 => 0,
            1 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
            _ => cast::<i32, _>(r.range(11, 1600)),
        };
        for slot in 1..=15 {
            if r.chance(Ext::from_f64(0.2)) {
                g.hold_stock_id[(slot, e)] = cast::<i16, _>(r.range(11, 1600));
                g.hold_stock_pct[(slot, e)] = cast::<i16, _>(r.range(1, 100));
                g.hold_stock_basis[(slot, e)] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(500.0));
            }
        }
        g.hold_stock_id[(0, e)] = cast::<i16, _>((1..=15).filter(|&s| g.hold_stock_id[(s, e)] != 0).count());
        if e > 10 {
            g.industry_num[e] = match e {
                11..=85 => 1,
                86..=130 => 2,
                1581..=1583 => 71,
                _ => {
                    if r.chance(Ext::from_f64(0.04)) {
                        [69, 70][cast::<usize, _>(r.range(0, 1))]
                    } else {
                        cast::<i16, _>(r.range(3, 68))
                    }
                }
            };
            g.market_cap[e] = f64r(&mut r, Ext::from_f64(5.0), Ext::from_f64(3000.0));
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
            g.ent_misc_data[e] = if r.chance(Ext::from_f64(0.7)) {
                let fields: Vec<String> = (1..=41)
                    .map(|k| match k {
                        1 | 3 => format!("{:.2}", f64r(&mut r, Ext::from(-Ext::from_f64(10.0)), Ext::from(Ext::from_f64(40.0)))),
                        14 | 30 => format!("{:.1}", f64r(&mut r, Ext::from(-Ext::from_f64(10.0)), Ext::from(Ext::from_f64(40.0)))),
                        20 => format!("{}", r.range(0, 2000)),
                        _ => r.range(0, 99).to_string(),
                    })
                    .collect();
                st(&(fields.join("|") + "|"))
            } else {
                st(&format!("{:.2}", f64r(&mut r, Ext::from(-Ext::from_f64(5.0)), Ext::from(Ext::from_f64(20.0)))))
            };
            if r.chance(Ext::from_f64(0.3)) {
                g.bonds_issued[e] = r.range(1, 1500);
                let mat = g.cur_year + cast::<i32, _>(r.range(0, 25));
                let coupon = f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(15.0));
                let cv = if r.chance(Ext::from_f64(0.15)) { "cv" } else { "" };
                g.bond_issue_info[e] = st(&format!("{mat} {cv}  {coupon:5.2}"));
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
    // bond holdings of banks, insurers, players and funds
    for h in (1..=130).chain(1581..=1583) {
        let mut n = 0;
        for slot in 1..=15 {
            if r.chance(Ext::from_f64(0.15)) {
                let b = cast::<i32, _>(r.range(11, 1600));
                if g.bonds_issued[b] > 0 {
                    g.hold_bond_id[(slot, h)] = b;
                    g.hold_bond_face[(slot, h)] = r.range(1, g.bonds_issued[b].max(1));
                    g.bond_hold_cost[(slot, h)] = cast::<Ext, _>(g.hold_bond_face[(slot, h)]) * f64r(&mut r, Ext::from_f64(0.8), Ext::from_f64(1.1));
                    n += 1;
                }
            }
        }
        g.hold_bond_id[(0, h)] = n;
    }
    // price histories
    for series in 0..=1606 {
        let cur = cast::<i32, _>(r.range(1, 60));
        g.cap_hist_close[(0, series)] = cast::<Ext, _>(cur);
        let mut v = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(500.0));
        for k in 0..60 {
            let mut slot = cur - k;
            if slot < 1 {
                slot += 60;
            }
            g.cap_hist_close[(slot, series)] = v;
            g.price_hist_high[(slot, series)] = v * f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(1.15));
            g.price_hist_low[(slot, series)] = v * f64r(&mut r, Ext::from_f64(0.85), Ext::from_f64(1.0));
            v *= Ext::from_f64(1.0) + f64r(&mut r, -Ext::from_f64(0.05), Ext::from_f64(0.05));
        }
    }
    g
}
