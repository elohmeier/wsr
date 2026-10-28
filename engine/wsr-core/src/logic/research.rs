//! The corporate research report `ShowResearchReport` (0x448ed3): background, earnings trend, analyst
//! projection, stock behaviour, subsidiaries, bank / insurance / industry data, hedges, swaps, special
//! items, management, credit standing, dividends, lawsuits, bonds, taxes, control, the analyst's rating
//! and target price, and recent headlines. Plus its callees `BankRegulatorLendingCheck` (0x6a117d),
//! `GetEtfDescription` (0x64a767) and `GetEtfLargestHoldingSlot` (0x64a5fe).
//!
//! Numeric conventions: `EXT` uses software x87 arithmetic; SINGLE locals are `f32` (every `fstp dword` rounds); SINGLE pool
//! constants (`fmul dword [0x75e…]`) are written `sf(0.05)`; every float → integer store uses
//! `to_long` / `to_integer` / `to_quad`. The operand order of the x87 code is kept. Comparisons are
//! written so that the branch taken for unordered (NaN) operands matches the machine code: `jbe skip` →
//! `if a > b`, `jae skip` → `if !(a >= b)`, `jb skip` → `if a >= b`, `je skip` → `if !eq87(a, b)`.

// Statements keep the original BASIC shape on purpose.
#![allow(
    clippy::assign_op_pattern,
    clippy::manual_range_contains,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::too_many_lines,
    clippy::nonminimal_bool,
    clippy::if_same_then_else,
    clippy::neg_multiply
)]
use crate::pb::real::{Ext, cast};


use crate::logic::cashflow::{get_etf_adviser_human_status, show_cash_flow_projection};
use crate::logic::control::{find_option_positions, update_corp_control};
use crate::logic::earnings::{balance_cash_t_bills, calc_adviser_fee_income, get_funds_advised_by};
use crate::logic::finance::{
    calc_capital_taxes, calc_entity_financials, get_controller_type, get_industry_category, get_phys_commod_value,
};
use crate::logic::futures::count_futures_positions;
use crate::logic::news::{is_in_recent_headlines, post_news};
use crate::logic::options::get_plr_naked_liab;
use crate::logic::report::{describe_defunct_entity, number_to_words, show_quick_message, show_text_report};
use crate::logic::reports_corp::{
    assign_etf_adviser, check_cross_ownership, consolidate_phys_commod_recs, get_entity_sole_commodity,
};
use crate::logic::shorts::find_short_positions;
use crate::logic::swaps::{process_swap_payments, sum_swap_notional, swap_pnl_accumulator};
use crate::logic::valuation::{
    calc_analyst_rating, calc_dividend_yield, calc_industry_market_share, calc_industry_outlook,
    calc_stock_price, calc_stock_volatility, calc_working_capital_ratio, get_mgmt_margin_vs_industry,
    price_pct_change_months, price_trend_streak,
};
use crate::pb::numeric::format;
use crate::pb::numeric::{str_num, val};
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{
    instr, lcase, ltrim, mcase, mid, parse_any, remove, replace, right, rtrim, string_fill, ucase, CaseMode,
};
use crate::pb::numeric::{int, sqr, to_integer, to_long, to_quad};
use crate::state::Globals;
use crate::ui::Ui;

// ------------------------------------------------------------------------------------------- helpers

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// A SINGLE constant of the original (`dword [0x75e…]`), widened to EXT.
fn sf(x: f32) -> Ext {
    cast::<Ext, _>(x)
}

/// `FORMAT$(x, mask)`.
fn fmt(x: Ext, mask: &str) -> PbStr {
    PbStr(format(x, mask.as_bytes()))
}

/// `RTRIM$(LTRIM$(x))`.
fn trim(x: &PbStr) -> PbStr {
    rtrim(ltrim(x.as_bytes()).as_bytes())
}

/// `STR$(x)` of an EXT / LONG / QUAD.
fn str15(x: Ext) -> PbStr {
    PbStr::from(str_num(x, 15).as_str())
}

/// `STR$(x)` of an INTEGER / SINGLE (PB_StrNum7).
fn str7(x: Ext) -> PbStr {
    PbStr::from(str_num(x, 7).as_str())
}

/// The x87 "equal" outcome (ZF = 1): equal, or unordered.
fn eq87(a: Ext, b: Ext) -> bool {
    a == b || a.is_nan() || b.is_nan()
}

/// `PARSE$(text, ANY "|", n)`.
fn field(text: &PbStr, n: i32) -> PbStr {
    parse_any(text.as_bytes(), b"|", n)
}

/// Error 9 (subscript out of range) in the original: its ON ERROR handler (0x4581e0) shows
/// `ShowRuntimeError` and leaves the routine — no report.
struct Abort;

type R = Result<(), Abort>;

/// Index check for an array access that can be out of range in practice.
fn chk(i: i32, lo: i32, hi: i32) -> Result<i32, Abort> {
    if i < lo || i > hi { Err(Abort) } else { Ok(i) }
}

/// The routine's local `DIM lines(1 TO 2500) AS STRING`.
struct Lines {
    v: Vec<PbStr>,
}

impl Lines {
    fn new() -> Self {
        Lines { v: vec![PbStr::new(); 2501] }
    }
    fn get(&self, i: i32) -> Result<PbStr, Abort> {
        chk(i, 1, 2500)?;
        Ok(self.v[cast::<usize, _>(i)].clone())
    }
    fn set(&mut self, i: i32, x: PbStr) -> R {
        chk(i, 1, 2500)?;
        self.v[cast::<usize, _>(i)] = x;
        Ok(())
    }
    /// `lines(i) = lines(i) + x`.
    fn app(&mut self, i: i32, x: &str) -> R {
        let t = self.get(i)? + x;
        self.set(i, t)
    }
    /// `lines(i) = lines(i) + x`.
    fn app_s(&mut self, i: i32, x: &PbStr) -> R {
        let t = self.get(i)? + x;
        self.set(i, t)
    }
    /// `lines(i) = REPLACE$(lines(i), a, b)`.
    fn repl(&mut self, i: i32, a: &[u8], b: &[u8]) -> R {
        let t = replace(self.get(i)?.as_bytes(), a, b);
        self.set(i, t)
    }
}

// ------------------------------------------------------------------------------------------- callees

/// Port of GetEtfDescription (0x64a767).
/// The investment focus of fund `etf_id` (1581..1600) from a local table `descs(0 TO 20)`.
///
/// An id outside 1580..1600 indexes the table out of range (error 9, no handler: the shared dummy
/// element, normally "", is returned and the caller's ON ERROR handler fires).
pub fn get_etf_description(etf_id: i32) -> PbStr {
    let __content = crate::content::current();
    let descs: [&str; 21] = [
        "",
        __content.string(8856),
        __content.string(8857),
        __content.string(8858),
        __content.string(8859),
        __content.string(8860),
        __content.string(8861),
        __content.string(8862),
        __content.string(8863),
        __content.string(8864),
        __content.string(8865),
        __content.string(8866),
        __content.string(8867),
        __content.string(8868),
        __content.string(8869),
        __content.string(8870),
        __content.string(8871),
        __content.string(8872),
        __content.string(8873),
        __content.string(8874),
        __content.string(8875),
    ];
    let fund_idx = etf_id.wrapping_sub(1580);
    if (0..=20).contains(&fund_idx) { s(descs[cast::<usize, _>(fund_idx)]) } else { PbStr::new() }
}

/// Port of GetEtfLargestHoldingSlot (0x64a5fe).
/// The holding slot (1..15) of fund `etf_id` (> 1585, active) with the largest `MarketCap * pct` among
/// its stock holdings (ids > 10); 0 if none is positive or for other ids.
pub fn get_etf_largest_holding_slot(g: &Globals, etf_id: i32) -> i32 {
    let i = etf_id;
    let mut result = 0;
    if 1585 < i && g.entity_status[i] == 1 {
        let mut max_value: Ext = Ext::from_f64(0.0);
        let mut best_slot = 0;
        for slot in 1..=15 {
            if 10 < g.hold_stock_id[(slot, i)] {
                // fld MarketCap; fimul HoldStockPct
                let v = g.market_cap[cast::<i32, _>(g.hold_stock_id[(slot, i)])] * cast::<Ext, _>(g.hold_stock_pct[(slot, i)]);
                if v > max_value {
                    best_slot = slot;
                    max_value = v;
                }
            }
        }
        result = best_slot;
    }
    result
}

/// Port of BankRegulatorLendingCheck (0x6a117d).
/// Regulators' view of bank `bank_ref` (11..85, active, deposits > 0): the loan-to-deposit ratio
/// `ROUND(100 * loans / deposits)` (loans = `BankLoansTotal` + mortgage + subprime + consumer loans +
/// `TmpCorpBondsValue`; deposits = demand + customer deposits + CDs). If it is at least 95 plus half the
/// credit rating and the rating is below 7, the excess capital (`ROUND(100 * (NetWorth +
/// BadDebtReserve) / deposits) - 5`, at least 0) is deducted; a remaining ratio of 94+ returns 1
/// ("restricted"; with `post_news = 1` and a non-human, not yet frozen bank: "Regulators force … to
/// restrict lending" unless already in the headlines). Otherwise, with `post_news = 1`, a frozen
/// non-human bank gets "… [runtime resource wording] ". Returns 0 except when restricted.
///
/// Uses the `Tmp*` totals of the last `CalcEntityFinancials` (the caller's bank).
pub fn bank_regulator_lending_check(g: &mut Globals, ui: &mut dyn Ui, bank_ref: i32, post_news_flag: i32) -> i32 {
    let __content = crate::content::current();
    let mut restricted = 0;
    let bank_id = bank_ref;
    if g.entity_status[bank_id] != 1 || bank_id < 11 || 85 < bank_id {
        return restricted;
    }
    let total_deposits: Ext =
        (g.bank_demand_deposits[bank_id] + g.bank_cust_deposits[bank_id]) + g.bank_c_ds[bank_id];
    if !(total_deposits > Ext::from_f64(0.0)) {
        return restricted;
    }
    let loan_assets: Ext = (((cast::<Ext, _>(g.bank_mortgage_loans[bank_id]) + g.bank_loans_total)
        + cast::<Ext, _>(g.subprime_secs[bank_id]))
        + cast::<Ext, _>(g.bank_consumer_loans[bank_id]))
        + g.tmp_corp_bonds_value;
    let mut loan_to_dep_pct = to_long(Ext::from_f64(100.0) * (loan_assets / total_deposits));
    // fild CreditRatingNum; fmul 0.5; fisubr loanToDepPct; compare with 95
    let adj = cast::<Ext, _>(loan_to_dep_pct) - cast::<Ext, _>(g.credit_rating_num[bank_id]) * sf(0.5f32);
    if adj >= Ext::from_f64(95.0) && g.credit_rating_num[bank_id] < 7 {
        let capital_ratio: Ext = (g.bad_debt_reserve[bank_id] + g.net_worth) / total_deposits;
        let mut capital_adj = to_long(Ext::from_f64(100.0) * capital_ratio) - 5;
        if capital_adj < 0 {
            capital_adj = 0;
        }
        loan_to_dep_pct = loan_to_dep_pct - capital_adj;
        if loan_to_dep_pct >= 94 && g.credit_rating_num[bank_id] < 7 {
            restricted = 1;
            if post_news_flag == 1
                && get_controller_type(g, bank_id) != 1
                && g.credit_frozen[bank_id] == 0
                && is_in_recent_headlines(g, __content.bytes(8876)) != 1
            {
                g.news_msg = s(__content.string(8877))
                    + &g.ent_name[bank_id]
                    + " ("
                    + &g.ent_symbol[bank_id]
                    + __content.string(8878);
                post_news(g, ui, bank_id, 0);
            }
        }
        return restricted;
    }
    if post_news_flag != 1 || get_controller_type(g, bank_id) == 1 || g.credit_frozen[bank_id] != 1 {
        return restricted;
    }
    let mut regulator_text = s(__content.string(8879));
    if g.currency_code == "USA" || g.currency_code == "UK" || g.currency_code == "EUR" {
        regulator_text = g.central_bank_name.clone() + " allows";
    }
    if is_in_recent_headlines(g, __content.bytes(8880)) != 1 {
        g.news_msg = regulator_text + " " + &g.ent_symbol[bank_id] + __content.string(8881);
        post_news(g, ui, bank_id, 0);
    }
    restricted
}

// ------------------------------------------------------------------------------------------- report

/// Locals of ShowResearchReport (PB zero-initialises them on entry).
#[derive(Default)]
struct F {
    corp_id: i32,
    report_corp_id: i32,
    industry: i16,
    corp_name: PbStr,
    dash_line: PbStr,
    amt_fmt: &'static str,
    eps_fmt: &'static str,
    line_no: i32,
    phrase: PbStr,
    phrase2: PbStr,
    phrase3: PbStr,
    line_text: PbStr,
    fee_pct_text: PbStr,
    sym_dot_pat: PbStr,
    tmp_id: i32,
    tmp_int: i32,
    trend_pct: i32,
    idx: i32,
    year: i32,
    cash_flow_code: i32,
    tmp_long: i64,
    tmp_amount: i64,
    futures_value: i64,
    suit_damages: i64,
    antitrust_damages: i64,
    calc_value: Ext,
    best_value: Ext,
    bond_unreal_loss: Ext,
    unused_credit: Ext,
    ytd_earnings: Ext,
    prior_ytd_earnings: Ext,
    market_share: f32,
}

impl F {
    fn sym(&self, g: &Globals) -> PbStr {
        g.ent_symbol[self.corp_id].clone()
    }
}

/// Port of ShowResearchReport (0x448ed3).
/// The research report on `corp_id_ref`, shown through [`show_text_report`] (title "=== RESEARCH REPORT
/// PREPARED FOR <player> on <corp> ===", `TextBoxMode = "84"`, report lines 1..90). Players (< 11) get
/// "… is not a corporation" and defunct entities [`describe_defunct_entity`] instead.
///
/// State effects of the original kept here: `CheckCrossOwnership(corp, 1)` (unless the game has ended),
/// `ConsolidatePhysCommodRecs`, `CalcEntityFinancials` (also of every borrower of a bank), a negative
/// cash balance is covered from T-bills / the unused credit line (borrowing `cash * 1.05` from
/// `LenderId` if the line allows) / working capital, `CalcStockPrice` in mode 0 and (for public corps)
/// mode 4 with `CalcAnalystRating` into `CurAnalystRating`, `UpdateCorpControl`, the swap / option /
/// short / futures queries, `GetPhysCommodValue` into `PhysCommodValue`, `BankLargestLoan` reduced by a
/// quarter of `BankLoansTotal` (floor 0), `SharesOutstanding` of 0 set to 1, a fund's `TaxLossCarry`
/// set to its `ExtraordinaryPL`, `AssignEtfAdviser` for a fund with an invalid fee or no adviser,
/// `ResearchReportCorp` (set while the report is built), `TmpExt1..3` cleared, and the analyst's target
/// left in the global `CalcStockPrice`. Draws `RND` (trend wording, extraordinary items, fund
/// performance, target price of weak credits, and inside the callees).
///
/// For corps > 85 the credit wording uses the cash-flow outlook of `ShowCashFlowProjection(corp, 1)`
/// (with its side effects).
///
/// An out-of-range subscript (e.g. no problem-loan debtor, a non-bank lender id) ends the routine like
/// its ON ERROR handler (`ShowRuntimeError`, not shown by the port): no report.
pub fn show_research_report(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, corp_id_ref: i32) {
    let mut lines = Lines::new();
    let mut f = F::default();
    let _ = research_report(g, ui, rng, &mut f, &mut lines, corp_id_ref);
}

fn research_report(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut F, lines: &mut Lines, corp_id_ref: i32) -> R {
    let __content = crate::content::current();
    // ON ERROR GOTO 0x4581e0 (ShowRuntimeError, exit)
    // (UI) DIALOG END DlgWhoOwnsWhat
    f.dash_line = string_fill(60, b"-");
    f.amt_fmt = "#################,";
    f.eps_fmt = "##############,.00";
    f.corp_name = g.ent_name[chk(corp_id_ref, 1, 1600)?].clone();
    f.corp_id = corp_id_ref;
    f.report_corp_id = f.corp_id;
    let corp = f.corp_id;
    if !(g.game_end_flag == "Y") {
        check_cross_ownership(g, ui, rng, corp, 1);
    }
    if corp < 11 {
        if g.entity_status[chk(corp, 1, 1600)?] == 2 {
            describe_defunct_entity(g, ui, corp);
        } else {
            let mut msg =
                g.cur_player_name.clone() + __content.string(8882);
            show_quick_message(g, ui, &mut msg);
        }
        return Ok(());
    }
    f.industry = g.industry_num[corp];
    if g.entity_status[corp] != 1 {
        if g.entity_status[corp] == 2 {
            describe_defunct_entity(g, ui, corp);
        }
        return Ok(());
    }
    g.research_report_corp = corp;
    consolidate_phys_commod_recs(g, corp);
    calc_entity_financials(g, ui, rng, corp);
    if !(g.cash[corp] >= Ext::from_f64(0.0)) {
        balance_cash_t_bills(g, corp);
    }
    if !(g.cash[corp] >= Ext::from_f64(0.0)) && g.unused_credit_line > g.cash[corp].abs() {
        let mut cash_shortfall: Ext = g.cash[corp].abs();
        if g.unused_credit_line > cash_shortfall * sf(1.05f32) {
            cash_shortfall = sf(1.05f32) * cash_shortfall;
        }
        g.cash[corp] = cash_shortfall + g.cash[corp];
        let lender = chk(g.lender_id, 1, 1600)?;
        g.cash[lender] = g.cash[lender] - cash_shortfall;
        g.bank_debt[corp] = cash_shortfall + g.bank_debt[corp];
    }
    if 130 < corp && !(g.cash[corp] >= Ext::from_f64(0.0)) && g.working_capital[corp] > g.cash[corp].abs() {
        g.working_capital[corp] = g.working_capital[corp] + g.cash[corp];
        g.cash[corp] = Ext::from_f64(0.0);
    }
    calc_entity_financials(g, ui, rng, corp);
    f.bond_unreal_loss = g.bond_unrealized_loss;
    f.unused_credit = g.unused_credit_line;
    f.futures_value = to_quad(g.futures_net_value);
    g.valuation_mode = 0;
    calc_stock_price(g, ui, rng, corp, 0); // TIME$ seconds: see calc_stock_price
    lines.set(2, s("     ") + &f.dash_line)?;
    lines.set(3, s(__content.string(8883)) + &f.corp_name)?;
    lines.set(
        4,
        s(__content.string(8884)) + &f.sym(g) + __content.string(8885) + &g.stock_price_str,
    )?;
    lines.set(5, s("     ") + &f.dash_line)?;
    if g.industry_num[corp] != 71 {
        operating_sections(g, ui, rng, f, lines)?;
    }
    calc_entity_financials(g, ui, rng, corp);
    if g.industry_num[corp] == 71 {
        fund_sections(g, ui, rng, f, lines)?;
    }
    common_sections(g, ui, rng, f, lines)?;
    Ok(())
}

/// Everything for companies other than funds (0x449562 .. 0x451ff9).
fn operating_sections(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut F, lines: &mut Lines) -> R {
    let __content = crate::content::current();
    earnings_section(g, ui, rng, f, lines)?;
    holdings_section(g, f, lines)?;
    if f.industry < 3 {
        financial_rates_section(g, f, lines)?;
    }
    if f.industry == 1 {
        bank_section(g, ui, rng, f, lines)?;
    }
    if f.industry == 2 {
        insurer_section(g, f, lines)?;
    }
    if f.industry != 1 && f.industry != 2 {
        if f.industry == 70 {
            lines.set(f.line_no, s("<< ") + &f.sym(g) + __content.string(8886))?;
            f.line_no += 1;
        } else {
            industry_section(g, ui, rng, f, lines)?;
            hedge_section(g, f, lines)?;
        }
    }
    swaps_and_items_section(g, ui, rng, f, lines)?;
    management_section(g, f, lines)?;
    Ok(())
}

/// Earnings history and projection, price behaviour (0x449562 .. 0x44af30).
fn earnings_section(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut F, lines: &mut Lines) -> R {
    let __content = crate::content::current();
    let corp = f.corp_id;
    let sym = f.sym(g);
    f.year = g.cur_year - 1;
    if g.cur_quarter == 4 && f.report_corp_id < g.sim_co_count {
        f.year = g.cur_year;
    }
    f.tmp_long = to_quad(cast::<Ext, _>(g.cur_quarter) - Ext::from_f64(1.0));
    if f.report_corp_id < g.sim_co_count {
        f.tmp_long = to_quad(cast::<Ext, _>(g.cur_quarter));
    }
    f.ytd_earnings = sf(0.01f32) * g.earnings_ytd[corp];
    f.prior_ytd_earnings = (cast::<Ext, _>(f.tmp_long) * g.earnings_y1[corp]) * sf(0.0025f32);
    f.phrase = s(__content.string(8887));
    if !(g.earnings_y1[corp] >= Ext::from_f64(0.0)) {
        f.phrase = s(" lost ");
    }
    f.phrase2 = g.currency_symbol.clone();
    f.line_text = s("<< ")
        + &sym
        + &f.phrase
        + &f.phrase2
        + &trim(&fmt((sf(0.01f32) * g.earnings_y1[corp]) / g.shares_outstanding[corp], f.eps_fmt))
        + &g.currency_suffix;
    lines.set(7, f.line_text.clone() + __content.string(8888) + &fmt(cast::<Ext, _>(f.year), "####") + ".")?;
    let ytd = f.ytd_earnings;
    let prior = f.prior_ytd_earnings;
    if eq87(ytd, Ext::from_f64(0.0)) || eq87(cast::<Ext, _>(f.tmp_long), Ext::from_f64(0.0)) {
        f.line_no = 8;
    } else if ytd > Ext::from_f64(0.0) {
        f.phrase = s("up");
        if !(ytd >= prior) {
            f.phrase = s("down");
        }
        f.phrase2 = trim(&fmt(ytd / g.shares_outstanding[corp], f.eps_fmt));
        f.phrase3 = trim(&fmt(prior / g.shares_outstanding[corp], f.eps_fmt));
        if f.phrase2 == f.phrase3 {
            f.phrase = s(__content.string(7716));
        }
        f.line_text = s("<< ") + &sym + __content.string(8889) + &g.currency_symbol;
        f.line_text = f.line_text.clone() + &f.phrase2 + &g.currency_suffix + __content.string(2818);
        lines.set(8, f.line_text.clone() + " are ")?;
        f.line_text = s("   ") + &f.phrase + " from " + &g.currency_symbol + &f.phrase3 + &g.currency_suffix;
        f.phrase = s("s");
        if eq87(Ext::from_f64(1.0), cast::<Ext, _>(f.tmp_long)) {
            f.phrase = PbStr::new();
        }
        lines.set(
            9,
            f.line_text.clone() + __content.string(8890) + &str15(cast::<Ext, _>(f.tmp_long)) + __content.string(8891) + &f.phrase + ".",
        )?;
        f.line_no = 10;
    } else {
        f.phrase = PbStr::new();
        if !(g.earnings_y1[corp] >= Ext::from_f64(0.0)) {
            f.phrase = s("still ");
        }
        f.phrase2 = s(".");
        if 10 < g.disc_spend_pct[corp] {
            f.phrase2 = s(",");
        }
        lines.set(
            8,
            s(__content.string(8892)) + &f.phrase + __content.string(8893) + &f.phrase2,
        )?;
        f.idx = get_industry_category(f.industry);
        if 0 < f.idx && 10 < g.disc_spend_pct[corp] {
            f.line_text = s(__content.string(8894));
            f.phrase = s(__content.string(8895));
            if f.idx == 2 {
                f.phrase = s(__content.string(8896));
            }
            lines.set(9, f.line_text.clone() + &f.phrase)?;
            f.line_no = 10;
        } else {
            f.line_no = 9;
        }
    }
    let (y1, y2, y3, y4) =
        (g.earnings_y1[corp], g.earnings_yr2_ago[corp], g.earnings_yr3_ago[corp], g.earnings_yr4_ago[corp]);
    if eq87(y1, Ext::from_f64(0.0)) && eq87(y2, Ext::from_f64(0.0)) && eq87(y3, Ext::from_f64(0.0)) {
        lines.set(
            7,
            s("<< ") + &sym + __content.string(8897) + &str15(cast::<Ext, _>(g.cur_year) - Ext::from_f64(1.0)) + ".",
        )?;
        f.line_no = 8;
    }
    f.phrase = PbStr::new();
    f.phrase2 = PbStr::new();
    if y1 >= Ext::from_f64(0.0) || y2 >= Ext::from_f64(0.0) {
        // LAB_00449ff1: strong growth
        if (ytd > prior || eq87(ytd, Ext::from_f64(0.0))) && y1 > y2 && y2 > y3 && y1 > y3 * sf(1.35f32) && y3 > Ext::from_f64(0.0) {
            if y3 > y4 {
                f.phrase2 = s(__content.string(8898));
            }
            lines.set(f.line_no, s("<< ") + &sym + __content.string(8899) + &f.phrase2 + __content.string(8900))?;
            f.line_text = s(__content.string(8901));
            f.tmp_id = if f.phrase2.is_empty() { 1 } else { 2 };
            if f.phrase2.is_empty() {
                let t = lines.get(f.line_no)? + &trim(&f.line_text);
                lines.set(f.line_no, t)?;
            } else {
                lines.set(f.line_no + 1, f.line_text.clone())?;
            }
            f.line_no = f.line_no + f.tmp_id;
        }
    } else {
        f.phrase = s("two");
        if !(y3 >= Ext::from_f64(0.0)) {
            f.phrase = s("three");
            if !(y4 >= Ext::from_f64(0.0)) {
                f.phrase = s("four");
            }
        }
        lines.set(f.line_no, s("<< ") + &sym + __content.string(8902) + &f.phrase)?;
        lines.set(f.line_no + 1, s(__content.string(8903)))?;
        f.line_no += 2;
    }
    if (!(ytd >= prior) || eq87(ytd, Ext::from_f64(0.0))) && !(y1 >= y2) && !(y2 >= y3) {
        f.phrase = s(__content.string(8904));
        if !(y3 >= y4) {
            f.phrase = s(__content.string(8905));
        }
        lines.set(f.line_no, s("<< ") + &sym + &f.phrase + __content.string(8906))?;
        f.line_no += 1;
    }
    if !(g.shares_outstanding[corp] > Ext::from_f64(0.0)) {
        g.shares_outstanding[corp] = Ext::from_f64(1.0);
    }
    f.calc_value = (sf(0.01f32) * g.proj_qtr_earnings[corp]) / g.shares_outstanding[corp];
    f.phrase = s("this");
    if corp < g.sim_co_count {
        f.phrase = s("next");
    }
    lines.set(
        f.line_no,
        s(__content.string(8907))
            + &f.phrase
            + __content.string(8908)
            + &trim(&fmt(f.calc_value, f.eps_fmt))
            + __content.string(8489),
    )?;
    f.line_no += 1;
    update_corp_control(g, ui, rng, corp);
    if 0 < g.public_pct {
        price_section(g, ui, rng, f, lines)?;
    }
    Ok(())
}

/// Valuation vs. net worth, volatility, price trend and 52-week range (0x44a5f1 .. 0x44af30).
fn price_section(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut F, lines: &mut Lines) -> R {
    let __content = crate::content::current();
    let corp = f.corp_id;
    let sym = f.sym(g);
    let mc = g.market_cap[corp];
    let nw = g.net_worth;
    if !(mc > nw * sf(0.0085f32)) || mc > nw * sf(0.04f32) {
        // LAB_0044a663
        if nw >= Ext::from_f64(0.0) {
            f.phrase = s(__content.string(8909));
            if mc >= nw * sf(0.04f32) {
                f.phrase = s(__content.string(8910));
            }
            lines.set(f.line_no, s(__content.string(8911)) + &f.phrase + __content.string(8912))?;
            lines.set(f.line_no + 1, s(__content.string(8913)))?;
            f.line_no += 2;
        }
    }
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_ext2 = Ext::from_f64(0.0);
    let mut volatility: f32 = cast::<f32, _>(calc_stock_volatility(g, ui, rng, corp, 1));
    let price_high: Ext = g.tmp_ext1;
    let price_low: Ext = g.tmp_ext2;
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_ext2 = Ext::from_f64(0.0);
    f.phrase = PbStr::new();
    if !(Ext::from_f32(0.0f32) >= volatility) {
        volatility = cast::<f32, _>(cast::<Ext, _>(volatility) * sf(0.01f32));
        volatility = cast::<f32, _>(sqr(cast::<Ext, _>(volatility)));
        let v = cast::<Ext, _>(volatility);
        if !(Ext::from_f64(1.25) > v) && price_high > price_low * sf(1.8f32) {
            f.phrase = sym.clone() + __content.string(8914);
        } else if !(v.abs() >= sf(0.85f32)) && price_low > price_high * sf(0.75f32) {
            f.phrase = sym.clone() + __content.string(8915);
        }
        if !f.phrase.is_empty() {
            lines.set(f.line_no, s("<< ") + &f.phrase)?;
            f.line_no += 1;
        }
        f.phrase = PbStr::new();
    }
    f.trend_pct = price_trend_streak(g, corp);
    if (cast::<Ext, _>(f.trend_pct)).abs() > Ext::from_f64(3.0) {
        f.phrase = s(__content.string(8916));
        if !(rng.rnd() * Ext::from_f64(2.0) >= Ext::from_f64(1.0)) {
            f.phrase = s("climbed");
        }
        if f.trend_pct < 0 {
            f.phrase = s(__content.string(8917));
            if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                f.phrase = s("fallen");
            }
        }
        lines.set(
            f.line_no,
            s("<< ")
                + &sym
                + __content.string(8918)
                + &f.phrase
                + __content.string(8919)
                + &number_to_words(f.trend_pct, 4, 0)
                + __content.string(8920),
        )?;
        f.line_no += 1;
    } else {
        f.trend_pct = price_pct_change_months(g, corp, 3);
        if (cast::<Ext, _>(f.trend_pct)).abs() > Ext::from_f64(10.0) {
            f.phrase = s(__content.string(8921));
            if f.trend_pct < 0 {
                f.phrase = s(__content.string(8922));
            }
            lines.set(
                f.line_no,
                s("<< ")
                    + &sym
                    + __content.string(8918)
                    + &f.phrase
                    + &number_to_words(f.trend_pct, 4, 1)
                    + __content.string(8923),
            )?;
            f.line_no += 1;
        } else {
            f.trend_pct = price_pct_change_months(g, corp, 30);
            f.tmp_int = price_pct_change_months(g, corp, 59);
            if 100 < f.trend_pct && f.trend_pct < f.tmp_int && 200 < f.tmp_int && f.tmp_int < 1000 {
                lines.set(
                    f.line_no,
                    s(__content.string(8924)) + &number_to_words(f.trend_pct, 4, 1) + __content.string(8925),
                )?;
                f.line_no += 1;
            }
        }
    }
    if f.phrase.is_empty() {
        let mc = g.market_cap[corp];
        if !(mc >= price_high * sf(0.75f32)) && !(mc >= price_low * sf(1.08f32)) {
            f.phrase = s("low");
        } else if mc > price_high * sf(0.92f32) && mc > price_low * sf(1.35f32) {
            f.phrase = s("high");
        }
        if !f.phrase.is_empty() && price_high > Ext::from_f64(0.0) && price_low > Ext::from_f64(0.0) {
            lines.set(f.line_no, s(__content.string(8926)) + &f.phrase + __content.string(8927))?;
            f.line_no += 1;
        }
    }
    Ok(())
}

/// Largest 20%+ subsidiary and its earnings contribution (0x44af30 .. 0x44b95f).
fn holdings_section(g: &mut Globals, f: &mut F, lines: &mut Lines) -> R {
    let __content = crate::content::current();
    let corp = f.corp_id;
    let sym = f.sym(g);
    let mut holdings_count: i16 = 0;
    let mut major_stakes: i16 = 0;
    let mut affiliated_subs: i16 = 0;
    let mut sub_pct: i16 = 0;
    f.best_value = Ext::from_f64(0.0);
    f.tmp_id = 0;
    for idx in 1..=15 {
        f.idx = idx;
        f.tmp_int = cast::<i32, _>(g.hold_stock_id[(idx, corp)]);
        if f.tmp_int == 0 {
            continue;
        }
        let pct_value: f32 = cast::<f32, _>(cast::<f32, _>(g.hold_stock_pct[(idx, corp)]));
        if !(Ext::from_f32(0.0f32) >= pct_value) {
            holdings_count += 1;
        }
        if !(Ext::from_f32(19.0f32) >= pct_value) {
            major_stakes += 1;
            f.tmp_long = to_quad(g.market_cap[chk(f.tmp_int, 11, 1600)?] * cast::<Ext, _>(pct_value));
            if g.corp_controller[corp] == g.corp_controller[chk(f.tmp_int, 1, 1600)?] {
                affiliated_subs += 1;
            }
            if cast::<Ext, _>(f.tmp_long) > f.best_value {
                f.best_value = cast::<Ext, _>(f.tmp_long);
                f.tmp_id = cast::<i32, _>(g.hold_stock_id[(idx, corp)]);
                sub_pct = to_integer(cast::<Ext, _>(pct_value));
            }
        }
    }
    f.idx = 16;
    if !(0 < f.tmp_id) {
        return Ok(());
    }
    let sub = chk(f.tmp_id, 11, 1600)?;
    if eq87(g.earnings_y1[sub], Ext::from_f64(0.0)) {
        return Ok(());
    }
    f.phrase3 = s("only");
    if 1 < major_stakes {
        f.phrase3 = s(__content.string(8928));
    }
    if f.phrase3 == "only" && 1 < holdings_count {
        f.phrase3 = f.phrase3.clone() + __content.string(8929);
    }
    if sub_pct < 80 && 0 < instr(1, f.phrase3.as_bytes(), __content.bytes(8930)) {
        if g.corp_controller[corp] == g.corp_controller[sub] {
            f.phrase3 = replace(f.phrase3.as_bytes(), __content.bytes(8930), b"affili");
            if affiliated_subs == 1 && 0 < instr(1, f.phrase3.as_bytes(), b"largest") {
                f.phrase3 = replace(f.phrase3.as_bytes(), b"largest", b"only");
            }
        } else if major_stakes != 1 || holdings_count != 1 {
            f.phrase3 = replace(f.phrase3.as_bytes(), __content.bytes(8931), __content.bytes(8932));
        }
    }
    f.line_text = s("<< ") + &g.ent_name[sub] + " is " + &sym + "'s " + &f.phrase3 + __content.string(8933);
    f.phrase = PbStr::new();
    if f.line_text.len() < 53 {
        f.phrase = s(__content.string(8934));
    }
    lines.set(f.line_no, f.line_text.clone() + &f.phrase)?;
    f.line_no += 1;
    f.tmp_long = to_quad((cast::<Ext, _>(sub_pct) * g.earnings_y1[sub]) * sf(0.01f32));
    if cast::<Ext, _>(f.tmp_long) > Ext::from_f64(0.0) {
        f.calc_value = Ext::from_f64(100.0) * (cast::<Ext, _>(f.tmp_long) / g.earnings_y1[corp]);
        f.phrase2 = trim(&fmt(f.calc_value, f.amt_fmt));
        f.phrase2 = remove(f.phrase2.as_bytes(), b",");
        f.tmp_int = to_long(val(f.phrase2.as_bytes()));
        if f.calc_value > Ext::from_f64(900.0) {
            f.phrase2 = s(__content.string(8935));
        }
        f.phrase = sym.clone() + "'s ";
        f.phrase3 = s(__content.string(8936));
        if 0 < f.tmp_int {
            lines.set(
                f.line_no,
                s(__content.string(8937)) + &f.phrase2 + "% of " + &f.phrase + __content.string(8938) + &f.phrase3,
            )?;
        } else if !(g.earnings_y1[corp] >= Ext::from_f64(0.0)) {
            lines.set(f.line_no, s(__content.string(8939)) + &f.phrase + __content.string(8940))?;
        } else if !eq87(g.earnings_y1[corp], Ext::from_f64(0.0)) {
            lines.set(f.line_no, s(__content.string(8941)) + &f.phrase + &f.phrase3)?;
        }
    } else if !(cast::<Ext, _>(f.tmp_long) >= Ext::from_f64(0.0)) {
        f.phrase = g.currency_symbol.clone();
        lines.set(
            f.line_no,
            s(__content.string(8942))
                + &f.phrase
                + &trim(&fmt((cast::<Ext, _>(f.tmp_long)).abs(), f.eps_fmt))
                + " "
                + &g.million_prefix_lc
                + __content.string(8943),
        )?;
    }
    if g.earnings_y1[sub].abs() >= Ext::from_f64(25.0) * g.exchange_rate && !eq87(g.earnings_y1[corp], Ext::from_f64(0.0)) {
        lines.app(f.line_no - 1, ",")?;
        f.line_no += 1;
    } else {
        lines.app(f.line_no - 1, ".")?;
        lines.set(f.line_no, s(__content.string(8944)))?;
    }
    Ok(())
}

/// Banks and insurers: industry, interest-rate outlook, bond carrying values (0x44b973 .. 0x44c123).
fn financial_rates_section(g: &mut Globals, f: &mut F, lines: &mut Lines) -> R {
    let __content = crate::content::current();
    let corp = f.corp_id;
    let sym = f.sym(g);
    f.phrase = g.ent_name[corp].clone();
    if f.phrase.len() > 15 {
        f.phrase = sym.clone();
    }
    lines.set(
        f.line_no,
        s("<< ") + &f.phrase + __content.string(1546) + &g.ind_name[cast::<i32, _>(g.industry_num[corp])] + __content.string(1547),
    )?;
    f.line_no += 1;
    let bir = cast::<Ext, _>(g.base_interest_rate);
    if Ext::from_f64(7.0) >= bir || Ext::from_f64(9.0) <= bir {
        f.phrase2 = s("harm");
        let case_val = bir;
        if case_val > Ext::from_f64(8.0) {
            f.phrase = s("high");
            if f.industry == 2 && !(g.bank_debt[corp] >= g.t_bills[corp] + g.cash[corp]) {
                f.phrase2 = s("help");
            }
        } else {
            f.phrase = s("low");
            if f.industry == 2 && g.bank_debt[corp] > g.t_bills[corp] + g.cash[corp] {
                f.phrase2 = s("help");
            }
            if f.industry == 1 {
                f.phrase2 = s("help");
            }
        }
        lines.set(
            f.line_no,
            s(__content.string(8945)) + &f.phrase + __content.string(8946) + &f.phrase2 + __content.string(8947),
        )?;
        lines.set(f.line_no + 1, s(__content.string(8948)))?;
        f.line_no += 2;
        if g.monetary_policy != 1 {
            if g.monetary_policy == 0 {
                f.phrase = s("lower");
            } else {
                f.phrase = s("higher");
            }
            let skip = (!(Ext::from_f64(8.0) >= bir) && f.phrase == "higher") || (Ext::from_f64(8.0) > bir && f.phrase == "lower");
            if !skip {
                f.phrase2 = g.central_bank_name.clone();
                if g.country_idx[corp] != 0 && g.currency_code == "USA" {
                    f.phrase2 = s("U.S. ") + &f.phrase2;
                }
                lines.set(
                    f.line_no,
                    s("<< The ")
                        + &mcase(f.phrase2.as_bytes(), CaseMode::Ansi)
                        + __content.string(8949)
                        + &f.phrase
                        + __content.string(8950),
                )?;
                f.line_no += 1;
            }
        }
    }
    // LAB_0044bef8
    let bonds = g.tmp_gov_bonds_value + g.tmp_corp_bonds_value;
    if bonds > Ext::from_f64(100.0) && f.bond_unreal_loss.abs() > bonds * sf(0.1f32) {
        f.phrase = s("<< ") + &sym + __content.string(8951);
        f.phrase2 = s(__content.string(8952));
        f.phrase3 = s(__content.string(8953))
            + &trim(&g.currency_symbol)
            + &trim(&fmt(f.bond_unreal_loss.abs(), "* ###,"))
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + ".";
        if !(f.bond_unreal_loss >= Ext::from_f64(0.0)) {
            f.phrase3 = replace(f.phrase3.as_bytes(), b"exceeds", __content.bytes(8954));
        }
        lines.set(f.line_no, f.phrase.clone())?;
        f.line_no += 1;
        lines.set(f.line_no, f.phrase2.clone())?;
        f.line_no += 1;
        lines.set(f.line_no, f.phrase3.clone())?;
        f.line_no += 1;
    }
    Ok(())
}

/// Banks: loan book quality, reserves, interbank borrowing, regulators (0x44c137 .. 0x44d244).
fn bank_section(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut F, lines: &mut Lines) -> R {
    let __content = crate::content::current();
    let corp = chk(f.corp_id, 11, 85)?; // BadDebtReserve(11 TO 85)
    let sym = f.sym(g);
    let pct_value: f32 = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(100.0) * (g.bad_debt_reserve[corp] / g.tmp_bank_total_loans)));
    let mut non_perf_loans: i64 = 0;
    f.tmp_amount = 0;
    f.best_value = Ext::from_f64(0.0);
    let mut worst_debtor_id = 0;
    for idx in 11..=1600 {
        f.idx = idx;
        if cast::<i32, _>(g.lending_bank[idx]) != corp || g.entity_status[idx] != 1 {
            continue;
        }
        calc_entity_financials(g, ui, rng, idx);
        if g.credit_rating_str == "D" {
            non_perf_loans = to_quad(g.bank_debt[idx] + cast::<Ext, _>(non_perf_loans));
        }
        if g.credit_rating_str == "C" {
            f.tmp_amount = to_quad(g.bank_debt[idx] + cast::<Ext, _>(f.tmp_amount));
        }
        if !(g.net_worth >= Ext::from_f64(0.0)) && g.bank_debt[idx] > f.best_value {
            f.best_value = g.bank_debt[idx];
            worst_debtor_id = idx;
        }
    }
    f.idx = 1601;
    calc_entity_financials(g, ui, rng, corp);
    let tbtl = g.tmp_bank_total_loans;
    if g.bank_largest_loan > tbtl * sf(0.25f32) && tbtl > Ext::from_f64(0.0) {
        f.calc_value = Ext::from_f64(100.0) * (g.bank_largest_loan / tbtl);
        lines.set(f.line_no, s("<< ") + &sym + __content.string(8955))?;
        lines.set(
            f.line_no + 1,
            s(__content.string(8956)) + &trim(&fmt(f.calc_value, "###")) + "% of",
        )?;
        lines.set(f.line_no + 2, s(__content.string(8957)))?;
        if 0 < g.bank_largest_debtor[corp] {
            let t = lines.get(f.line_no + 2)?
                + __content.string(8958)
                + &g.ent_name[chk(g.bank_largest_debtor[corp], 1, 1600)?]
                + ")";
            lines.set(f.line_no + 2, t)?;
        }
        lines.app(f.line_no + 2, ".")?;
        f.line_no += 3;
    } else if tbtl > Ext::from_f64(0.0) && cast::<Ext, _>(g.bank_consumer_loans[corp]) > Ext::from_f64(300.0) {
        f.calc_value = Ext::from_f64(100.0) * (cast::<Ext, _>(g.bank_consumer_loans[corp]) / tbtl);
        if f.calc_value > Ext::from_f64(40.0) && !(f.calc_value >= Ext::from_f64(99.0)) {
            lines.set(f.line_no, s(__content.string(8959)))?;
            lines.set(
                f.line_no + 1,
                s(__content.string(8960))
                    + &trim(&fmt(f.calc_value, "###"))
                    + __content.string(8961),
            )?;
            f.line_no += 2;
        }
    } else if cast::<Ext, _>(g.subprime_secs[corp]) > Ext::from_f64(500.0) && cast::<Ext, _>(g.subprime_secs[corp]) > tbtl * sf(0.2f32) {
        f.calc_value = Ext::from_f64(100.0)
            * (cast::<Ext, _>(g.subprime_secs[corp]) / (cast::<Ext, _>(g.bank_mortgage_loans[corp]) + cast::<Ext, _>(g.subprime_secs[corp])));
        f.calc_value = f.calc_value - Ext::from_f64(1.0);
        if f.calc_value > Ext::from_f64(50.0) && !(f.calc_value >= Ext::from_f64(98.0)) {
            lines.set(
                f.line_no,
                s("<< ") + &sym + __content.string(8962) + &trim(&fmt(f.calc_value, "###")) + __content.string(8963),
            )?;
            lines.set(f.line_no + 1, s(__content.string(8964)))?;
            f.line_no += 2;
        }
    }
    // LAB_0044c8b1
    g.bank_largest_loan = g.bank_largest_loan - g.bank_loans_total * sf(0.25f32);
    if !(g.bank_largest_loan >= Ext::from_f64(0.0)) {
        g.bank_largest_loan = Ext::from_f64(0.0);
    }
    let mut expected_losses: Ext = ((((cast::<Ext, _>(non_perf_loans) * sf(0.2f32) + tbtl * sf(0.005f32))
        + cast::<Ext, _>(f.tmp_amount) * sf(0.03f32))
        + cast::<Ext, _>(g.bank_consumer_loans[corp]) * sf(0.06f32))
        + g.bank_largest_loan * sf(0.1f32))
        + cast::<Ext, _>(g.subprime_secs[corp]) * sf(0.03f32);
    if 0 < g.subprime_crisis_state {
        expected_losses = cast::<Ext, _>(g.subprime_secs[corp]) * sf(0.1f32) + expected_losses;
    }
    // Quirk: BankDebt(corpId + 1), the next entity's borrowing (see BUGS.md)
    if g.bank_debt[corp + 1] > g.total_assets * sf(0.1f32) {
        f.calc_value = g.bank_debt[corp + 1] - g.total_assets * sf(0.1f32);
        expected_losses = f.calc_value * sf(0.15f32) + expected_losses;
    }
    let bdr = g.bad_debt_reserve[corp];
    if bdr > expected_losses {
        lines.set(f.line_no, s(__content.string(8965)))?;
        lines.set(f.line_no + 1, s(__content.string(8966)))?;
        lines.set(
            f.line_no + 2,
            s(__content.string(8967)) + &trim(&fmt(cast::<Ext, _>(pct_value), "##.00")) + __content.string(8968),
        )?;
        f.line_no += 3;
    } else if expected_losses > bdr * sf(1.05f32) {
        lines.set(f.line_no, s(__content.string(8969)))?;
        lines.set(f.line_no + 1, s(__content.string(8970)) + &sym + __content.string(8971))?;
        lines.set(
            f.line_no + 2,
            s(__content.string(8972)) + &trim(&fmt(cast::<Ext, _>(pct_value), "##.00")) + __content.string(8973),
        )?;
        f.line_no += 3;
    }
    f.tmp_id = to_long(Ext::from_f64(100.0) * (cast::<Ext, _>(non_perf_loans) / tbtl));
    if 3 < f.tmp_id {
        lines.set(
            f.line_no,
            s(__content.string(8974))
                + &sym
                + __content.string(8975)
                + &g.currency_symbol
                + &trim(&fmt(cast::<Ext, _>(non_perf_loans), f.amt_fmt)),
        )?;
        lines.set(
            f.line_no + 1,
            s("   ")
                + &g.million_prefix_lc
                + __content.string(8976)
                + &number_to_words(f.tmp_id, 5, 1)
                + " of its",
        )?;
        // EntName(worstDebtorId): 0 when no borrower with a negative net worth was found -> error 9
        lines.set(
            f.line_no + 2,
            s(__content.string(8977)) + &g.ent_name[chk(worst_debtor_id, 1, 1600)?] + ".",
        )?;
        f.line_no += 3;
    }
    let liquid = ((g.t_bills[corp] + g.cash[corp]) + g.tmp_gov_bonds_value) + g.tmp_corp_bonds_value;
    if !eq87(g.bank_debt[corp], Ext::from_f64(0.0)) || !(liquid > g.total_assets * sf(0.1f32)) {
        // LAB_0044ceb4
        if g.bank_debt[corp] > Ext::from_f64(0.0) {
            lines.set(f.line_no, s(__content.string(8978)))?;
            lines.set(f.line_no + 1, s(__content.string(8979)))?;
            f.line_no += 2;
        }
    }
    let mut frozen_lines = false;
    if g.credit_frozen[corp] == 1
        && bank_regulator_lending_check(g, ui, corp, 0) == 1
        && g.credit_rating_num[corp] < 7
        && get_controller_type(g, corp) != 1
    {
        frozen_lines = true;
        lines.set(f.line_no, s(__content.string(8980)))?;
        lines.set(f.line_no + 1, s(__content.string(8981)))?;
        if 4 < g.credit_rating_num[corp] {
            lines.repl(f.line_no + 1, __content.bytes(8982), __content.bytes(8983))?;
        }
        f.line_no += 2;
    }
    if !frozen_lines {
        // LAB_0044d0c3
        let debtor = g.bank_largest_debtor[corp];
        if 0 < debtor && g.bank_debt[chk(debtor, 1, 1600)?] > g.exchange_rate * Ext::from_f64(10000.0) {
            lines.set(f.line_no, s(__content.string(8984)))?;
            lines.set(f.line_no + 1, s(__content.string(8985)) + &g.ent_name[debtor] + ".")?;
            f.line_no += 2;
        }
    }
    if !(g.bad_debt_reserve[corp] + g.net_worth >= Ext::from_f64(0.0)) {
        lines.set(f.line_no, s(__content.string(8986)))?;
        f.line_no += 1;
    }
    Ok(())
}

/// `InsurUwProfitYTD(corpId)` (`DIM (86 TO 130)`).
fn uw_profit(g: &Globals, corp: i32) -> Result<Ext, Abort> {
    Ok(g.insur_uw_profit_ytd[chk(corp, 86, 130)?])
}

/// Insurers: growth, underwriting, subprime write-downs, ETF advisory (0x44d258 .. 0x44e17d).
fn insurer_section(g: &mut Globals, f: &mut F, lines: &mut Lines) -> R {
    let __content = crate::content::current();
    let corp = f.corp_id;
    let sym = f.sym(g);
    let gr = g.growth_rate[corp];
    if 8 < gr || gr < -5 {
        // LAB_0044d2ae
        f.phrase = s("expand");
        f.phrase2 = s("depress");
        if gr < 0 {
            f.phrase = s("shrink");
            f.phrase2 = s("improv");
        }
        lines.set(f.line_no, s("<< ") + &sym + __content.string(8987) + &f.phrase + __content.string(8988))?;
        lines.set(f.line_no + 1, s(__content.string(8989)) + &f.phrase2 + __content.string(8990))?;
        f.phrase = s("in");
        if gr < 0 {
            f.phrase = s("de");
        }
        lines.set(f.line_no + 2, s(__content.string(8991)) + &f.phrase + __content.string(8992))?;
        lines.set(f.line_no + 3, s(__content.string(8993)))?;
        if (cast::<i32, _>(g.disc_spend_pct[corp]) + 3) < cast::<i32, _>(gr)
            && !(uw_profit(g, corp)? >= g.exchange_rate * -Ext::from_f64(20.0))
            && !(val(field(&g.ent_misc_data[corp], 18).as_bytes()) >= Ext::from_f64(0.0))
        {
            lines.app(f.line_no + 3, __content.string(8994))?;
            f.line_no += 1;
            lines.set(f.line_no + 3, s(__content.string(8995)))?;
            f.line_no += 1;
            lines.set(f.line_no + 3, s(__content.string(8996)))?;
        }
        f.line_no += 4;
    } else if (cast::<Ext, _>(gr)).abs() <= Ext::from_f64(5.0) {
        f.phrase = s("w");
        f.phrase2 = s(__content.string(8997));
        if gr < 0 {
            f.phrase2 = s(__content.string(8998));
        }
        if gr == 0 {
            f.phrase = s("t");
            f.phrase2 = s("ll");
        }
        lines.set(f.line_no, s(__content.string(8999)) + &f.phrase + __content.string(9000))?;
        lines.set(f.line_no + 1, s("   a") + &f.phrase2 + ".")?;
        f.line_no += 2;
    }
    f.phrase = s("NUL");
    f.phrase2 = PbStr::new();
    if (cast::<Ext, _>(g.profit_margin[corp])).abs() > Ext::from_f64(7.0) && uw_profit(g, corp)? >= Ext::from_f64(0.0) {
        f.phrase = PbStr::new();
    }
    if g.profit_margin[corp] < 0 {
        f.phrase = s("un");
        if g.growth_rate[corp] < -3 {
            f.phrase2 = s(__content.string(9001));
        }
    }
    if !(f.phrase == "NUL") {
        lines.set(
            f.line_no,
            s("<< ") + &sym + __content.string(9002) + &f.phrase + __content.string(9003),
        )?;
        lines.set(f.line_no + 1, s(__content.string(9004)) + &f.phrase2 + __content.string(9005))?;
        f.line_no += 2;
    }
    let warn = if f.phrase == "un" {
        true
    } else {
        f.phrase == "NUL" && !(uw_profit(g, corp)? >= g.exchange_rate * -Ext::from_f64(20.0))
    };
    if warn && uw_profit(g, corp)?.abs() > g.market_cap[corp] {
        // LAB_0044dad5
        lines.set(f.line_no, s(__content.string(9006)))?;
        lines.set(f.line_no + 1, s(__content.string(9007)))?;
        f.line_no += 2;
    }
    f.tmp_amount = to_quad(val(field(&g.ent_misc_data[corp], 22).as_bytes()));
    if cast::<Ext, _>(f.tmp_amount) > Ext::from_f64(80.0) {
        lines.set(f.line_no, s(__content.string(9008)) + &sym + __content.string(9009))?;
        lines.set(
            f.line_no + 1,
            s(__content.string(9010))
                + &trim(&g.currency_symbol)
                + &trim(&fmt(cast::<Ext, _>(f.tmp_amount), f.amt_fmt))
                + " "
                + &g.million_prefix_uc
                + ".,",
        )?;
        lines.set(f.line_no + 2, s(__content.string(9011)))?;
        lines.set(f.line_no + 3, s(__content.string(9012)))?;
        f.line_no += 4;
    }
    if cast::<Ext, _>(g.subprime_secs[corp]) > Ext::from_f64(100.0) && g.total_assets > Ext::from_f64(0.0) {
        f.tmp_int = to_long((cast::<Ext, _>(g.subprime_secs[corp]) / g.total_assets) * Ext::from_f64(100.0));
        f.tmp_int = f.tmp_int.wrapping_sub(1);
        if 40 < f.tmp_int && f.tmp_int < 98 {
            lines.set(f.line_no, s(__content.string(9013)))?;
            lines.set(f.line_no + 1, s(__content.string(9014)))?;
            lines.set(
                f.line_no + 2,
                s(__content.string(9015)) + &str15(cast::<Ext, _>(f.tmp_int)) + __content.string(9016),
            )?;
            lines.set(f.line_no + 3, s(__content.string(9017)))?;
            f.line_no += 4;
        }
    }
    f.trend_pct = get_funds_advised_by(g, corp);
    if 0 < f.trend_pct {
        lines.set(f.line_no, s("<< ") + &sym + " ")?;
        if f.trend_pct < 1581 {
            // Quirk: the count shown is tmpId (the subsidiary id left from the holdings scan), not the
            // number of funds in trendPct (see BUGS.md).
            let t = lines.get(f.line_no)?
                + __content.string(9018)
                + &number_to_words(f.tmp_id, 5, 0)
                + __content.string(9019);
            lines.set(f.line_no, t)?;
        } else {
            let t = lines.get(f.line_no)?
                + __content.string(9020)
                + &g.ent_name[f.trend_pct]
                + " ("
                + &g.ent_symbol[f.trend_pct]
                + ").";
            lines.set(f.line_no, t)?;
        }
        f.line_no += 1;
        f.calc_value = calc_adviser_fee_income(g, corp);
        if f.calc_value > Ext::from_f64(2.0) {
            f.fee_pct_text = trim(&g.currency_symbol)
                + &trim(&fmt(f.calc_value, f.amt_fmt))
                + " "
                + &g.million_prefix_uc
                + ".";
            lines.set(
                f.line_no,
                s(__content.string(9021)) + &f.fee_pct_text,
            )?;
            f.line_no += 1;
        }
    }
    Ok(())
}

/// Operating companies: industry, operating assets, profitability and supply/demand outlook, growth,
/// working capital, market share, dominance (0x44e226 .. 0x44f657).
fn industry_section(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut F, lines: &mut Lines) -> R {
    let __content = crate::content::current();
    let corp = f.corp_id;
    let sym = f.sym(g);
    let industry = f.industry;
    let ind = cast::<i32, _>(industry);
    lines.set(f.line_no, s(__content.string(9022)) + &g.ind_name[cast::<i32, _>(g.industry_num[corp])] + __content.string(1547))?;
    f.line_no += 1;
    let ba = g.business_assets[chk(corp, 131, 1600)?];
    let pct_value: f32 = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(100.0) * (ba / g.total_assets)));
    if Ext::from_f64(20.0) > cast::<Ext, _>(pct_value) && g.total_assets > Ext::from_f64(50.0) {
        lines.set(f.line_no, s(__content.string(9023)) + &trim(&fmt(cast::<Ext, _>(pct_value), "##.0")) + __content.string(9024))?;
        f.line_no += 1;
    }
    calc_industry_outlook(g, rng, industry);
    let outlook_a: f32 = cast::<f32, _>(cast::<f32, _>(g.tmp_ext1));
    let _outlook_b: f32 = cast::<f32, _>(cast::<f32, _>(g.tmp_ext2));
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_ext2 = Ext::from_f64(0.0);
    g.tmp_ext3 = Ext::from_f64(0.0);
    let mut avg_ind_margin: f32 = cast::<f32, _>(Ext::from_f32(0.0f32));
    let mut peer_count = 0;
    for idx in 131..=1600 {
        f.idx = idx;
        if g.industry_num[idx] == industry && g.entity_status[idx] == 1 {
            // fild ProfitMargin; fadd dword avg; fisub DiscSpendPct; fstp dword
            avg_ind_margin =
                cast::<f32, _>((cast::<Ext, _>(g.profit_margin[idx]) + cast::<Ext, _>(avg_ind_margin)) - cast::<Ext, _>(g.disc_spend_pct[idx]));
            peer_count += 1;
        }
    }
    f.idx = 1601;
    if 0 < peer_count {
        avg_ind_margin = cast::<f32, _>(cast::<Ext, _>(avg_ind_margin) / cast::<Ext, _>(peer_count));
    } else {
        avg_ind_margin = 0.0f32;
    }
    avg_ind_margin = cast::<f32, _>(cast::<Ext, _>(avg_ind_margin) + cast::<Ext, _>(outlook_a));
    f.phrase3 = s("fairly ");
    if !(sf(19.4f32) >= cast::<Ext, _>(avg_ind_margin)) {
        f.phrase3 = s("very ");
    }
    if sf(10.4f32) > cast::<Ext, _>(avg_ind_margin) {
        f.phrase3 = s("un");
    }
    f.phrase = PbStr::new();
    if g.ind_name[ind].len() < 16 {
        f.phrase = s(__content.string(9025));
    }
    lines.set(
        f.line_no,
        s(__content.string(9026))
            + &mcase(g.ind_name[ind].as_bytes(), CaseMode::Ansi)
            + __content.string(1975)
            + &f.phrase
            + __content.string(9027),
    )?;
    lines.set(f.line_no + 1, s("   ") + &f.phrase3 + __content.string(9028))?;
    f.phrase2 = s("improv");
    calc_industry_outlook(g, rng, industry);
    let supply_ratio: Ext = g.ind_total_assets / cast::<Ext, _>(g.ind_demand[ind]);
    g.tmp_ext1 = Ext::from_f64(100.0) * (Ext::from_f64(1.0) - supply_ratio);
    let demand = cast::<Ext, _>(g.ind_demand[ind]);
    let proj_demand: Ext = demand + (cast::<Ext, _>(g.ind_growth_rate[ind]) * sf(0.005f32)) * demand;
    let proj_supply_ratio: Ext = g.tmp_ext3 / proj_demand;
    g.tmp_ext2 = Ext::from_f64(100.0) * (Ext::from_f64(1.0) - proj_supply_ratio);
    if g.tmp_ext1 > g.tmp_ext2 {
        f.phrase2 = s(__content.string(9029));
        if f.phrase3 == "un" {
            f.phrase = s("still ");
        }
    }
    if eq87(int(g.tmp_ext1), int(g.tmp_ext2)) {
        f.phrase2 = s("stable");
    }
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_ext2 = Ext::from_f64(0.0);
    g.tmp_ext3 = Ext::from_f64(0.0);
    if !(f.phrase == "still ") {
        f.phrase = PbStr::new();
    }
    if f.phrase2 == "improv" && !(f.phrase3 == "un") {
        f.phrase = s("still ");
    }
    if f.phrase2 == __content.string(9029) && f.phrase3 == "un" {
        f.phrase2 = s("worsen");
    }
    if f.phrase2 == "stable" {
        if f.phrase3 == "un" {
            f.phrase2 = f.phrase2.clone() + __content.string(9030);
        }
        lines.set(f.line_no + 2, s("   is ") + &f.phrase2 + ".")?;
    } else {
        lines.set(f.line_no + 2, s("   is ") + &f.phrase + &f.phrase2 + "ing.")?;
    }
    f.line_no += 3;
    if 30 < g.growth_rate[corp] {
        lines.set(
            f.line_no,
            s(__content.string(9031)) + &str7(cast::<Ext, _>(g.growth_rate[corp])) + "% rate",
        )?;
        f.phrase = s(".");
        if g.working_capital[corp] > Ext::from_f64(0.0) {
            let ba = g.business_assets[corp];
            let need = ba * calc_working_capital_ratio(g, corp);
            if !(g.liquid_funds + g.working_capital[corp] >= need) && eq87(g.unused_credit_line, Ext::from_f64(0.0)) {
                f.phrase = s(__content.string(9032));
            }
        }
        let p = f.phrase.clone();
        lines.app_s(f.line_no, &p)?;
        f.line_no += 1;
        if !(f.phrase == ".") {
            lines.set(f.line_no, s(__content.string(9033)))?;
            f.line_no += 1;
        }
    } else {
        let need = (calc_working_capital_ratio(g, corp) * sf(0.95f32)) * g.business_assets[corp];
        if !(g.liquid_funds + g.working_capital[corp] >= need) {
            lines.set(f.line_no, s(__content.string(9034)))?;
            f.line_no += 1;
        }
    }
    if 14 < g.growth_rate[corp] && get_industry_category(industry) != 1 {
        lines.set(f.line_no, s("<< ") + &sym + __content.string(9035))?;
        f.line_no += 1;
    }
    if industry == 37 {
        f.trend_pct = get_funds_advised_by(g, corp);
        if 0 < f.trend_pct {
            lines.set(f.line_no, s("<< ") + &sym + " ")?;
            if f.trend_pct < 1581 {
                let t = lines.get(f.line_no)?
                    + __content.string(9036)
                    + &str15(cast::<Ext, _>(f.trend_pct))
                    + __content.string(9019);
                lines.set(f.line_no, t)?;
            } else {
                let t = lines.get(f.line_no)?
                    + __content.string(9037)
                    + &g.ent_name[f.trend_pct]
                    + " ("
                    + &g.ent_symbol[f.trend_pct]
                    + ").";
                lines.set(f.line_no, t)?;
            }
            f.line_no += 1;
            f.calc_value = calc_adviser_fee_income(g, corp);
            if f.calc_value > Ext::from_f64(2.0) {
                f.fee_pct_text = trim(&g.currency_symbol)
                    + &trim(&fmt(f.calc_value, f.amt_fmt))
                    + " "
                    + &g.million_prefix_uc
                    + ".";
                lines.set(
                    f.line_no,
                    s(__content.string(9038)) + &f.fee_pct_text,
                )?;
                f.line_no += 1;
            }
        }
    }
    f.market_share = cast::<f32, _>(Ext::from_f64(100.0) * (g.business_assets[corp] / g.ind_total_assets));
    let market_share = cast::<Ext, _>(f.market_share);
    if Ext::from_f64(2.0) > market_share {
        lines.set(f.line_no, s("<< ") + &f.corp_name + __content.string(9039))?;
        f.line_no += 1;
    } else {
        lines.set(
            f.line_no,
            s("<< ") + &f.corp_name + __content.string(9040) + &trim(&fmt(market_share, "##.0")) + "%",
        )?;
        lines.set(f.line_no + 1, s(__content.string(9041)))?;
        f.line_no += 2;
    }
    f.tmp_id = 0;
    f.best_value = Ext::from_f64(0.0);
    for idx in 131..=1600 {
        f.idx = idx;
        // Quirk: no EntityStatus check (see BUGS.md)
        if g.industry_num[idx] == industry {
            let corp_share: Ext = Ext::from_f64(100.0) * (g.business_assets[idx] / g.ind_total_assets);
            if !(f.best_value >= corp_share) {
                f.tmp_id = idx;
                f.best_value = corp_share;
            }
        }
    }
    f.idx = 1601;
    if f.best_value > Ext::from_f64(20.0) && corp == f.tmp_id {
        lines.set(f.line_no - 1, s(__content.string(9042)))?;
    } else if corp != f.tmp_id {
        let top = chk(f.tmp_id, 1, 1600)?;
        lines.set(f.line_no, s("<< ") + &g.ent_name[top] + __content.string(9043))?;
        lines.set(
            f.line_no + 1,
            s(__content.string(9044)) + &trim(&fmt(f.best_value, "##.0")) + __content.string(9045),
        )?;
        if g.corp_controller[corp] == g.corp_controller[top] {
            lines.app_s(f.line_no + 1, &(s(__content.string(9046)) + &sym + ".)"))?;
        }
        f.line_no += 2;
    }
    g.tmp_ext1 = cast::<Ext, _>(g.corp_controller[corp]);
    f.tmp_int = calc_industry_market_share(g, industry);
    f.phrase = trim(&str15(cast::<Ext, _>(f.tmp_int)));
    let top_ctrl = g.corp_controller[chk(f.tmp_id, 1, 1600)?];
    if g.corp_controller[corp] == top_ctrl
        && corp != f.tmp_id
        && f.best_value + market_share > cast::<Ext, _>(f.tmp_int)
    {
        f.phrase = trim(&fmt(f.best_value + market_share, "##.0"));
    }
    if !(Ext::from_f64(1.0) + market_share >= cast::<Ext, _>(f.tmp_int)) && 25 < f.tmp_int {
        lines.set(f.line_no, s("<< ") + &sym + __content.string(9047))?;
        lines.set(f.line_no + 1, s("   of ") + &f.phrase + __content.string(2068) + &g.ind_name[ind] + __content.string(1547))?;
        f.line_no += 2;
    }
    let _ = ui;
    Ok(())
}

/// Futures hedges / speculation and stored physical commodities (0x44f657 .. 0x450374).
fn hedge_section(g: &mut Globals, f: &mut F, lines: &mut Lines) -> R {
    let __content = crate::content::current();
    let corp = f.corp_id;
    let sym = f.sym(g);
    let industry = f.industry;
    let fv = cast::<Ext, _>(f.futures_value);
    f.phrase = s("profit");
    if !(fv >= Ext::from_f64(0.0)) {
        f.phrase = s("loss");
    }
    if matches!(industry, 7 | 8 | 11 | 15 | 16 | 23 | 25 | 28 | 37 | 50 | 52 | 69) {
        if fv.abs() > g.market_cap[corp] {
            f.phrase3 = get_entity_sole_commodity(g, corp, 1);
            if f.phrase3 == "NO" || f.phrase3 == "STOCKS" {
                f.phrase3 = s(__content.string(2644));
            } else {
                f.phrase3 = lcase(f.phrase3.as_bytes(), CaseMode::Ansi);
                if f.phrase3 == "oil" {
                    f.phrase3 = s(__content.string(2588));
                }
            }
            lines.set(f.line_no, s(__content.string(9048)) + &f.phrase3 + __content.string(9049))?;
            if industry == 37 {
                lines.set(f.line_no, s(__content.string(9050)))?;
                f.phrase3 = get_entity_sole_commodity(g, corp, 1);
                if f.phrase3 == "STOCKS" {
                    lines.repl(f.line_no, __content.bytes(9051), __content.bytes(2547))?;
                }
            }
            lines.set(f.line_no + 1, s(__content.string(9052)) + &f.phrase + ".")?;
            f.tmp_amount = to_quad(val(field(&g.ent_misc_data[corp], 38).as_bytes()));
            f.tmp_amount = to_quad((Ext::from_f64(1.0) - sf(0.01f32) * cast::<Ext, _>(g.corp_tax_rate)) * cast::<Ext, _>(f.tmp_amount));
            if cast::<Ext, _>(f.tmp_amount) > g.exchange_rate * Ext::from_f64(10.0) {
                let last_qtr_earnings = to_quad(val(field(&g.ent_misc_data[corp], 3).as_bytes()));
                f.tmp_id = 0;
                if !eq87(cast::<Ext, _>(last_qtr_earnings), Ext::from_f64(0.0)) {
                    f.tmp_id = to_long(Ext::from_f64(100.0) * (cast::<Ext, _>(f.tmp_amount) / cast::<Ext, _>(last_qtr_earnings)));
                }
                f.tmp_id = to_long((cast::<Ext, _>(f.tmp_id)).abs());
                if 10 <= f.tmp_id || !(cast::<Ext, _>(last_qtr_earnings) >= Ext::from_f64(0.0)) {
                    lines.app(f.line_no + 1, __content.string(9053))?;
                    if industry == 37 {
                        lines.repl(f.line_no + 1, __content.bytes(9054), b"Futures")?;
                        f.phrase2 = s("trading");
                    }
                    if last_qtr_earnings < 0 || !(f.tmp_amount < last_qtr_earnings) {
                        lines.set(f.line_no + 2, s(__content.string(9055)))?;
                        if industry == 37 {
                            let p2 = f.phrase2.clone();
                            lines.repl(f.line_no + 2, __content.bytes(9056), p2.as_bytes())?;
                        }
                    } else {
                        lines.set(
                            f.line_no + 2,
                            s(__content.string(9057))
                                + &trim(&fmt(cast::<Ext, _>(f.tmp_id), "* ####"))
                                + __content.string(9058),
                        )?;
                        if industry == 37 {
                            let p2 = f.phrase2.clone();
                            lines.repl(f.line_no + 2, b"program", p2.as_bytes())?;
                        }
                    }
                    f.line_no += 1;
                }
            }
            f.line_no += 2;
        }
    } else if fv.abs() > g.market_cap[corp] * Ext::from_f64(2.0) {
        lines.set(f.line_no, s(__content.string(9059)))?;
        lines.set(f.line_no + 1, s(__content.string(9060)) + &f.phrase + __content.string(9061))?;
        f.phrase3 = get_entity_sole_commodity(g, corp, 1);
        if f.phrase3 == "NO" {
            f.phrase = s(__content.string(9062));
        } else {
            if f.phrase3 == "OIL" {
                f.phrase3 = s(__content.string(5722));
            }
            if f.phrase3 == "STOCKS" {
                f.phrase3 = s(__content.string(2499));
            }
            f.phrase3 = lcase(f.phrase3.as_bytes(), CaseMode::Ansi);
            if f.phrase3 == "bitcoin" || f.phrase3 == __content.string(9063) {
                f.phrase3 = mcase(f.phrase3.as_bytes(), CaseMode::Ansi);
            }
            f.phrase = s(__content.string(9064)) + &f.phrase3 + __content.string(9065);
        }
        lines.set(f.line_no + 2, f.phrase.clone())?;
        f.line_no += 3;
    }
    // LAB_00450044
    g.phys_commod_value = get_phys_commod_value(g, corp, &PbStr::new());
    let pcv = g.phys_commod_value;
    let cost = g.ci_phys_commod_cost;
    if pcv > g.market_cap[corp] * Ext::from_f64(3.0) && (pcv - cost).abs() > g.market_cap[corp] * Ext::from_f64(1.0) && cost > Ext::from_f64(15.0) {
        f.phrase3 = get_entity_sole_commodity(g, corp, 2);
        if f.phrase3 == "NO" {
            f.phrase3 = s(__content.string(9066));
        } else {
            f.phrase3 = lcase(f.phrase3.as_bytes(), CaseMode::Ansi);
            if f.phrase3 == "oil" {
                f.phrase3 = s(__content.string(2588));
            }
            if f.phrase3 == "bitcoin" || f.phrase3 == __content.string(9063) {
                f.phrase3 = mcase(f.phrase3.as_bytes(), CaseMode::Ansi);
            }
        }
        f.tmp_amount = to_quad(pcv - cost);
        f.phrase = s("gain");
        if !(cast::<Ext, _>(f.tmp_amount) >= Ext::from_f64(0.0)) {
            f.phrase = s("loss");
        }
        lines.set(f.line_no, s("<< ") + &sym + __content.string(9067) + &f.phrase3 + __content.string(9068))?;
        lines.set(f.line_no + 1, s(__content.string(9069)) + &f.phrase + __content.string(9070))?;
        f.line_no += 2;
    }
    Ok(())
}

/// Swaps, asset sale offers, extraordinary items, bank overhead and deposits (0x450374 .. 0x4515ee).
fn swaps_and_items_section(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut F, lines: &mut Lines) -> R {
    let __content = crate::content::current();
    let corp = f.corp_id;
    let sym = f.sym(g);
    let industry = f.industry;
    let mut swap_pnl = to_quad(process_swap_payments(g, ui, rng, corp, &s("R")));
    f.phrase2 = s("firm");
    if industry == 1 {
        f.phrase2 = s("bank");
    }
    if industry == 2 {
        f.phrase2 = s("insurer");
    }
    if industry == 37 {
        f.phrase2 = s(__content.string(9071));
    }
    if (cast::<Ext, _>(swap_pnl)).abs() > Ext::from_f64(10.0) {
        f.phrase = s("profit");
        if !(cast::<Ext, _>(swap_pnl) >= Ext::from_f64(0.0)) {
            f.phrase = s("loss");
        }
        lines.set(f.line_no, s("<< The ") + &f.phrase2 + __content.string(9072))?;
        lines.set(f.line_no + 1, s(__content.string(9073)))?;
        lines.set(
            f.line_no + 2,
            s(__content.string(9074))
                + &f.phrase
                + " of "
                + &trim(&g.currency_symbol)
                + &trim(&fmt((cast::<Ext, _>(swap_pnl)).abs(), "* ######,"))
                + " "
                + &g.million_prefix_lc
                + "illion"
                + &g.currency_suffix
                + __content.string(9075),
        )?;
        if g.sim_co_count < corp {
            lines.repl(f.line_no + 2, b"this", b"next")?;
        }
        f.trend_pct = 0;
        if !(cast::<Ext, _>(swap_pnl) >= Ext::from_f64(0.0))
            && (cast::<Ext, _>(swap_pnl)).abs() > g.net_worth
            && g.net_worth > Ext::from_f64(0.0)
            && lines.get(f.line_no + 2)?.len() < 49
        {
            f.trend_pct = 1;
        }
        if f.trend_pct == 1 {
            lines.app(f.line_no + 2, __content.string(9076))?;
        }
        f.line_no += 3;
        if f.trend_pct == 1 {
            lines.set(f.line_no, s(__content.string(9077)) + &sym + __content.string(9078))?;
            f.line_no += 1;
        }
    } else {
        f.tmp_amount = to_quad(sum_swap_notional(g, corp, 0));
        if g.net_worth > Ext::from_f64(50.0) && cast::<Ext, _>(f.tmp_amount) > g.net_worth * Ext::from_f64(10.0) {
            f.tmp_id = to_long(cast::<Ext, _>(f.tmp_amount) / g.net_worth);
            lines.set(f.line_no, s("<< The ") + &f.phrase2 + __content.string(9079))?;
            lines.set(f.line_no + 1, s(__content.string(9080)))?;
            lines.set(
                f.line_no + 2,
                s("   is") + &number_to_words(f.tmp_id, 0, 0) + __content.string(9081),
            )?;
            f.line_no += 3;
        }
    }
    if g.sim_co_count < corp {
        swap_pnl = to_quad(swap_pnl_accumulator(g, corp, 2, Ext::from_f64(0.0)));
        f.phrase = s("profits");
        if !(cast::<Ext, _>(swap_pnl) >= Ext::from_f64(0.0)) {
            f.phrase = s("losses");
        }
        if !eq87(cast::<Ext, _>(swap_pnl), Ext::from_f64(0.0)) {
            lines.set(
                f.line_no,
                s("<< ")
                    + &sym
                    + __content.string(9082)
                    + &f.phrase
                    + " of "
                    + &trim(&g.currency_symbol)
                    + &trim(&fmt((cast::<Ext, _>(swap_pnl)).abs(), "* ######,"))
                    + " "
                    + &g.million_prefix_lc
                    + "illion"
                    + &g.currency_suffix
                    + __content.string(9083),
            )?;
            lines.set(f.line_no + 1, s(__content.string(9084)))?;
            f.line_no += 2;
        }
    }
    if val(g.asset_sale_offers[0].as_bytes()) > Ext::from_f64(0.0) {
        f.phrase = PbStr::new();
        f.phrase2 = PbStr::new();
        for idx in 1..=100 {
            f.idx = idx;
            if g.asset_sale_offers[idx].is_empty() {
                continue;
            }
            f.phrase2 = g.asset_sale_offers[idx].clone();
            if eq87(val(field(&f.phrase2, 1).as_bytes()), cast::<Ext, _>(corp)) {
                if field(&f.phrase2, 2) == "CA" {
                    continue;
                }
                f.tmp_id = to_long(val(field(&f.phrase2, 2).as_bytes()));
                f.tmp_int = to_long(val(field(&f.phrase2, 3).as_bytes()));
                f.phrase = s(__content.string(9085))
                    + &str15(cast::<Ext, _>(f.tmp_int))
                    + __content.string(2163)
                    + &g.ent_symbol[chk(f.tmp_id, 1, 1600)?]
                    + __content.string(9086);
                break;
            }
        }
        if !f.phrase.is_empty() {
            lines.set(f.line_no, f.phrase.clone())?;
            f.line_no += 1;
        }
    }
    let epl = cast::<Ext, _>(g.extraordinary_pl[corp]);
    let mut extra_items = to_quad(g.extra_items_ytd[corp] + epl);
    extra_items = to_quad(cast::<Ext, _>(extra_items) - (cast::<Ext, _>(g.corp_tax_rate) * sf(0.01f32)) * epl);
    if !((cast::<Ext, _>(extra_items)).abs() >= g.proj_qtr_earnings[corp].abs() * sf(0.05f32)) {
        extra_items = 0;
    }
    if (cast::<Ext, _>(extra_items)).abs() > Ext::from_f64(25.0) {
        f.phrase = s(__content.string(9087));
        f.phrase2 = s("losses");
        if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
            f.phrase = s(__content.string(9088));
        }
        if cast::<Ext, _>(extra_items) > Ext::from_f64(0.0) {
            f.phrase = s(__content.string(9089));
            f.phrase2 = s(__content.string(9090));
        }
        lines.set(
            f.line_no,
            s("<< ") + &sym + " has " + &f.phrase + __content.string(9091) + &f.phrase2 + __content.string(9092),
        )?;
        lines.set(
            f.line_no + 1,
            s("   ")
                + &g.currency_symbol
                + &trim(&fmt((cast::<Ext, _>(extra_items)).abs(), f.amt_fmt))
                + " "
                + &g.million_prefix_lc
                + "illion"
                + &g.currency_suffix
                + __content.string(9093),
        )?;
        lines.set(f.line_no + 2, s("   ") + __content.string(9094))?;
        f.line_no += 3;
    }
    if industry == 1 {
        f.tmp_id = cast::<i32, _>(g.profit_margin[corp]) - cast::<i32, _>(g.disc_spend_pct[corp]);
        f.phrase = s(".");
        if (cast::<Ext, _>(f.tmp_id)).abs() > Ext::from_f64(5.0) {
            f.phrase = s(",");
        }
        f.calc_value = (Ext::from_f64(50.0) - cast::<Ext, _>(f.tmp_id)) / Ext::from_f64(25.0);
        if f.calc_value > Ext::from_f64(4.0) {
            f.calc_value = sf(3.8f32);
        }
        if !(f.calc_value >= sf(0.8f32)) {
            f.calc_value = sf(0.9f32);
        }
        f.phrase2 = s(" and ad");
        if g.disc_spend_pct[corp] == 0 {
            f.phrase2 = PbStr::new();
        }
        lines.set(
            f.line_no,
            s(__content.string(9095))
                + &f.phrase2
                + __content.string(9096)
                + &fmt(f.calc_value, "##.0")
                + __content.string(9097)
                + &f.phrase,
        )?;
        f.line_no += 1;
        if (cast::<Ext, _>(f.tmp_id)).abs() > Ext::from_f64(5.0) {
            f.phrase = s("lower");
            if f.tmp_id < -5 {
                f.phrase = s("higher");
            }
            lines.set(f.line_no, s(__content.string(9098)) + &f.phrase + __content.string(9099))?;
            f.line_no += 1;
        }
        if 5 < g.growth_rate[corp] {
            lines.set(
                f.line_no,
                s(__content.string(9100)) + &sym + __content.string(9101),
            )?;
            f.line_no += 1;
        } else if g.growth_rate[corp] < -2 {
            lines.set(f.line_no, s(__content.string(9102)))?;
            f.line_no += 1;
        }
        let cust = g.bank_cust_deposits[corp];
        if cust > g.bank_c_ds[corp] * sf(0.1f32) && cust > Ext::from_f64(100.0) {
            lines.set(
                f.line_no,
                s("<< ")
                    + &sym
                    + " has "
                    + &trim(&g.currency_symbol)
                    + &trim(&fmt(cust, "* ###,"))
                    + " "
                    + &g.million_prefix_lc
                    + "illion "
                    + __content.string(9103),
            )?;
            lines.set(f.line_no + 1, s(__content.string(9104)))?;
            lines.set(f.line_no + 2, s(__content.string(9105)))?;
            f.line_no += 3;
        }
    }
    Ok(())
}

/// Management, CEO, scenario allegations, R&D / advertising, tax credits, P/E (0x4515ee .. 0x451ff9).
fn management_section(g: &mut Globals, f: &mut F, lines: &mut Lines) -> R {
    let __content = crate::content::current();
    let corp = f.corp_id;
    let sym = f.sym(g);
    let industry = f.industry;
    if industry < 70 {
        f.tmp_id = get_mgmt_margin_vs_industry(g, corp, 0);
        if f.tmp_id < 9 && -7 < f.tmp_id {
            f.phrase2 = PbStr::new();
            f.phrase = s(__content.string(9106));
            if f.tmp_id < 0 {
                f.phrase = s(__content.string(9107));
            }
        } else {
            f.phrase2 = s("very ");
            f.phrase = s("capable");
            if f.tmp_id < -6 {
                f.phrase = s(__content.string(9108));
            }
            if f.tmp_id < -10 {
                f.phrase2 = s(__content.string(9109));
            }
        }
        lines.set(
            f.line_no,
            s(__content.string(9110)) + &sym + __content.string(9111) + &f.phrase2 + &f.phrase + ".",
        )?;
        f.line_no += 1;
        if g.new_mgmt_list.len() > 14 {
            f.phrase = right(g.new_mgmt_list.as_bytes(), 15);
            f.phrase2 = s("000") + &trim(&str15(cast::<Ext, _>(corp)));
            f.phrase2 = right(f.phrase2.as_bytes(), 4);
            if 0 < instr(1, f.phrase.as_bytes(), f.phrase2.as_bytes()) {
                lines.repl(f.line_no - 1, b"Current", b"Recent")?;
                lines.set(f.line_no, s(__content.string(9112)))?;
                f.line_no += 1;
            }
        }
    }
    let player_count = g.num_players;
    let mut ceo_player_id = 0;
    for idx in 1..=player_count {
        f.idx = idx;
        if corp == g.player_ceo_of[idx] {
            ceo_player_id = idx;
        }
    }
    if 0 < ceo_player_id {
        lines.set(f.line_no, s(__content.string(9113)) + &g.ent_name[ceo_player_id] + ".")?;
        f.line_no += 1;
    }
    let sn = (cast::<Ext, _>(g.scenario_num)).abs();
    if cast::<i32, _>(g.scenario_corp) == corp
        && !eq87(sn, Ext::from_f64(101.0))
        && !eq87(sn, Ext::from_f64(102.0))
        && !eq87(sn, Ext::from_f64(104.0))
        && (g.scenario_stage < 1 || 5 < g.scenario_stage)
    {
        lines.set(f.line_no, s(__content.string(9114)))?;
        lines.set(f.line_no + 1, s(__content.string(9115)))?;
        lines.set(f.line_no + 2, s(__content.string(9116)))?;
        f.line_no += 3;
    }
    if 7 < g.disc_spend_pct[corp] && industry < 70 {
        f.idx = get_industry_category(industry);
        f.phrase = s(__content.string(9117));
        if f.idx == 2 {
            f.phrase = s(__content.string(9118));
        }
        if industry == 1 {
            f.phrase = s(__content.string(9119));
        }
        if industry == 2 {
            f.phrase = s(__content.string(9120));
        }
        lines.set(
            f.line_no,
            s("<< ") + &sym + " spends" + &str7(cast::<Ext, _>(g.disc_spend_pct[corp])) + __content.string(9121) + &f.phrase + ".",
        )?;
        f.line_no += 1;
        if 10 < g.disc_spend_pct[corp] && f.idx == 1 {
            lines.set(f.line_no, s(__content.string(9122)))?;
            f.line_no += 1;
        }
    }
    if g.tax_credits[corp] > g.market_cap[corp] * Ext::from_f64(3.0) {
        lines.set(f.line_no, s(__content.string(9123)))?;
        if !(g.tax_loss_carry[corp] >= Ext::from_f64(0.0)) {
            lines.repl(f.line_no, b"unused", b"accrued")?;
        }
        f.line_no += 1;
    }
    let pe_ratio: Ext =
        if g.earnings_y1[corp] > Ext::from_f64(0.0) { (g.market_cap[corp] * Ext::from_f64(100.0)) / g.earnings_y1[corp] } else { Ext::from_f64(0.0) };
    if (Ext::from_f64(7.0) >= pe_ratio || Ext::from_f64(28.0) <= pe_ratio) && g.earnings_y1[corp] > Ext::from_f64(0.0) {
        f.phrase = PbStr::new();
        if !(Ext::from_f64(28.0) >= pe_ratio) && g.net_worth > g.market_cap[corp] * Ext::from_f64(200.0) {
            f.phrase = s("high");
        }
        if Ext::from_f64(7.0) > pe_ratio {
            f.phrase = s("low");
        }
        if !f.phrase.is_empty() {
            lines.set(f.line_no, s(__content.string(9124)) + &sym + __content.string(9125) + &f.phrase + __content.string(9126))?;
            f.line_no += 1;
        }
    }
    Ok(())
}

/// Funds (industry 71): strategy, NAV, performance, fees and adviser, holdings, gains (0x452035 ..
/// 0x4538b7).
fn fund_sections(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut F, lines: &mut Lines) -> R {
    let __content = crate::content::current();
    let corp = f.corp_id;
    let sym = f.sym(g);
    f.line_no = 7;
    lines.set(f.line_no, s("<< ") + &sym + __content.string(9127))?;
    f.line_no += 1;
    lines.set(f.line_no, s(__content.string(9128)) + &sym + __content.string(9129))?;
    f.line_no += 1;
    lines.set(f.line_no, s(__content.string(9130)))?;
    f.line_no += 1;
    let desc = get_etf_description(corp);
    chk(corp - 1580, 0, 20)?; // descs(fundIdx) out of range -> error 9
    lines.set(f.line_no, s("   in: ") + &desc)?;
    f.line_no += 1;
    if g.covered_call_fund_id == corp {
        lines.set(f.line_no, s(__content.string(9131)))?;
        if find_option_positions(g, ui, corp, 0, 1, 0) != -11 && find_option_positions(g, ui, corp, 0, 1, 0) != 12 {
            let with = sym.clone() + __content.string(9132);
            lines.repl(f.line_no, __content.bytes(9133), with.as_bytes())?;
        }
        f.line_no += 1;
    }
    if corp == 1581 {
        for t in [
            s("<< ") + &sym + __content.string(9134),
            s(__content.string(9135)),
            s(__content.string(9136)),
            s(__content.string(9137)),
            s(__content.string(9138)),
        ] {
            lines.set(f.line_no, t)?;
            f.line_no += 1;
        }
    }
    if corp == 1582 || corp == 1583 {
        f.phrase = s("high");
        f.phrase2 = s(__content.string(9139));
        if corp == 1583 {
            f.phrase = s("\"junk\"");
            f.phrase2 = s(__content.string(9140));
        }
        lines.set(f.line_no, s("<< ") + &sym + __content.string(9141))?;
        f.line_no += 1;
        lines.set(f.line_no, s("   ") + &f.phrase + __content.string(9142) + &f.phrase2 + __content.string(9143))?;
        f.line_no += 1;
        f.phrase3 = s(__content.string(9144));
        if corp == 1583 {
            f.phrase3 = s(__content.string(9145));
        }
        lines.set(f.line_no, s(__content.string(9146)) + &f.phrase3)?;
        f.line_no += 1;
        if corp == 1583 {
            lines.set(f.line_no, s(__content.string(9147)))?;
            f.line_no += 1;
        }
    }
    if corp == 1584 || corp == 1585 {
        f.phrase = s("long");
        if corp == 1585 {
            f.phrase = s("short");
        }
        for t in [
            s("<< ") + &sym + __content.string(9148),
            s(__content.string(9149)),
            s(__content.string(9150)) + &f.phrase + __content.string(2546),
            s(__content.string(9151)),
            s(__content.string(9152)),
            s(__content.string(9153)),
        ] {
            lines.set(f.line_no, t)?;
            f.line_no += 1;
        }
        f.phrase2 = s(__content.string(9154));
        f.phrase3 = s("same");
        if corp == 1585 {
            f.phrase2 = s("an \"inverse\"");
            f.phrase3 = s(__content.string(9155));
        }
        lines.set(f.line_no, s(__content.string(9156)) + &f.phrase2 + __content.string(9157) + &f.phrase3)?;
        f.line_no += 1;
        lines.set(f.line_no, s(__content.string(9158)))?;
        f.line_no += 1;
        if !(g.net_worth >= g.exchange_rate * Ext::from_f64(11500.0)) && count_futures_positions(g, corp, 0) == 0 {
            lines.set(f.line_no, s(__content.string(9159)))?;
            f.line_no += 1;
            lines.set(f.line_no, s(__content.string(9160)))?;
            f.line_no += 1;
        }
    }
    let nav_per_share: Ext = (sf(0.01f32) * g.net_worth) / g.shares_outstanding[corp];
    lines.set(
        f.line_no,
        s(__content.string(9161))
            + &trim(&g.currency_symbol)
            + &trim(&fmt(nav_per_share, f.eps_fmt))
            + &g.currency_suffix
            + ".",
    )?;
    f.line_no += 1;
    f.tmp_int = price_pct_change_months(g, corp, 12);
    f.phrase = PbStr::new();
    if 12 < f.tmp_int {
        f.phrase = s(__content.string(9162)) + &str15(cast::<Ext, _>(f.tmp_int)) + "%";
        if 100 < f.tmp_int {
            f.phrase = replace(f.phrase.as_bytes(), __content.bytes(9163), b"superb");
            if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) {
                f.phrase = replace(f.phrase.as_bytes(), b"superb", __content.bytes(9164));
            }
        } else if 29 < f.tmp_int {
            f.phrase = replace(f.phrase.as_bytes(), __content.bytes(9163), __content.bytes(9165));
        }
    } else if f.tmp_int < -8 {
        f.phrase = s(__content.string(9166))
            + &sym
            + __content.string(9167)
            + &number_to_words(f.tmp_int, 12, 1);
    }
    if !f.phrase.is_empty() {
        lines.set(f.line_no, s("<< ") + &f.phrase + ".")?;
        f.line_no += 1;
    }
    let fund = chk(corp, 1581, 1600)?; // FundFeePct(1581 TO 1600)
    let ffp = cast::<Ext, _>(g.fund_fee_pct[fund]);
    if sf(0.2f32) > ffp || !(Ext::from_f64(1.0) >= ffp) || g.fund_adviser[fund] == 0 {
        assign_etf_adviser(g, ui, rng, corp, 0, 0);
    }
    let ffp = cast::<Ext, _>(g.fund_fee_pct[fund]);
    f.fee_pct_text = s("0") + &trim(&str7(ffp));
    if eq87(Ext::from_f64(1.0), ffp) {
        f.fee_pct_text = s("1.0");
    }
    f.tmp_amount = to_quad((sf(0.01f32) * ffp) * g.net_worth);
    if cast::<Ext, _>(f.tmp_amount) > Ext::from_f64(2.0) {
        let fee_amt_text = trim(&g.currency_symbol)
            + &trim(&fmt(cast::<Ext, _>(f.tmp_amount), f.amt_fmt))
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix;
        lines.set(
            f.line_no,
            s(__content.string(9168)) + &f.fee_pct_text + __content.string(9169),
        )?;
        lines.set(f.line_no + 1, s(__content.string(9170)) + &fee_amt_text + __content.string(9171))?;
        f.line_no += 2;
        if sf(0.21f32) > cast::<Ext, _>(g.fund_fee_pct[fund])
            && 0 < g.fund_bonus_year[fund]
            && 1585 < corp
            && get_etf_adviser_human_status(g, ui, rng, corp) == 2
        {
            lines.app(f.line_no - 1, __content.string(9172))?;
            lines.set(f.line_no, s(__content.string(9173)))?;
            if corp % 2 == g.cur_year % 2 {
                f.phrase = s("this");
            } else {
                f.phrase = s("next");
            }
            let t = lines.get(f.line_no)? + &f.phrase + " year.";
            lines.set(f.line_no, t)?;
            f.line_no += 1;
            if price_pct_change_months(g, corp, 12) <= 0 && price_pct_change_months(g, corp, 24) <= 0 {
                lines.set(f.line_no, s(__content.string(9174)))?;
                f.line_no += 1;
            }
        }
    }
    f.trend_pct = g.fund_adviser[fund];
    if 85 < f.trend_pct {
        let adv = chk(f.trend_pct, 1, 1600)?;
        lines.set(
            f.line_no,
            s(__content.string(9175)) + &g.ent_name[adv] + " (" + &g.ent_symbol[adv] + ").",
        )?;
        f.line_no += 1;
    }
    let etf_top_slot = get_etf_largest_holding_slot(g, corp);
    if 0 < etf_top_slot {
        f.tmp_int = cast::<i32, _>(g.hold_stock_id[(etf_top_slot, corp)]);
        f.phrase = g.ent_name[f.tmp_int].clone();
        lines.set(f.line_no, s(__content.string(9176)) + &f.phrase + ".")?;
        f.line_no += 1;
    }
    let cap_gains = g.extraordinary_pl[corp];
    if cast::<Ext, _>(cap_gains) > g.exchange_rate * Ext::from_f64(5.0) {
        lines.set(f.line_no, s("<< ") + &sym + __content.string(9177))?;
        f.line_no += 1;
        lines.set(
            f.line_no,
            s("   ")
                + &trim(&g.currency_symbol)
                + &trim(&fmt(cast::<Ext, _>(cap_gains), f.amt_fmt))
                + " "
                + &g.million_prefix_lc
                + __content.string(9178),
        )?;
        f.line_no += 1;
        lines.set(f.line_no, s("   ") + __content.string(9179))?;
        f.line_no += 1;
    }
    if g.sim_co_count < 1595 {
        let swap_pnl = to_quad(swap_pnl_accumulator(g, corp, 2, Ext::from_f64(0.0)));
        f.phrase = s(__content.string(9180));
        if !(cast::<Ext, _>(swap_pnl) >= Ext::from_f64(0.0)) {
            f.phrase = s(__content.string(9181));
        }
        if (cast::<Ext, _>(swap_pnl)).abs() > Ext::from_f64(10.0) {
            lines.set(
                f.line_no,
                s("<< ")
                    + &sym
                    + " "
                    + &f.phrase
                    + " of "
                    + &trim(&g.currency_symbol)
                    + &trim(&fmt((cast::<Ext, _>(swap_pnl)).abs(), "* ######,"))
                    + " "
                    + &g.million_prefix_lc
                    + "illion"
                    + &g.currency_suffix
                    + __content.string(9182),
            )?;
            lines.set(f.line_no + 1, s(__content.string(9183)))?;
            f.line_no += 2;
        }
    }
    if 4 < g.credit_rating_num[corp] && !(g.net_worth >= g.tmp_total_debt * Ext::from_f64(3.0)) && g.bank_debt[corp] > Ext::from_f64(0.0) {
        lines.set(
            f.line_no,
            s("<< ") + __content.string(9184),
        )?;
        f.line_no += 1;
        lines.set(f.line_no, s("   ") + __content.string(9185))?;
        if 7 < g.credit_rating_num[corp] {
            lines.app(f.line_no, __content.string(9186))?;
        }
        lines.app(f.line_no, ".")?;
        f.line_no += 1;
    }
    Ok(())
}

/// Credit standing, dividends, special situations, lawsuits, bonds, taxes, control, shares, options,
/// analyst rating and target, headlines, and the report itself (0x4538b7 .. 0x4581db).
fn common_sections(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut F, lines: &mut Lines) -> R {
    credit_section(g, ui, rng, f, lines)?;
    situations_section(g, f, lines)?;
    bonds_section(g, f, lines)?;
    taxes_and_control_section(g, ui, rng, f, lines)?;
    rating_and_news_section(g, ui, rng, f, lines)?;
    Ok(())
}

/// Credit standing, cash flow, naked options, dividends (0x4538b7 .. 0x454aae).
fn credit_section(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut F, lines: &mut Lines) -> R {
    let __content = crate::content::current();
    let corp = f.corp_id;
    let sym = f.sym(g);
    let industry = f.industry;
    if 85 < corp {
        f.cash_flow_code = to_long(show_cash_flow_projection(g, ui, rng, corp, 1));
    }
    f.phrase = s(__content.string(9187));
    f.phrase2 = s(__content.string(9188));
    f.phrase3 = mid(g.credit_rating_str.as_bytes(), 1, 1);
    if f.phrase3 == "A" || f.phrase3 == "B" {
        f.phrase2 = PbStr::new();
    }
    if f.phrase3 == "B" {
        f.phrase = s(__content.string(9189));
    }
    if f.phrase3 == "A" || g.credit_rating_str == "BBB" {
        f.phrase = s(__content.string(9190));
        if eq87(f.unused_credit, Ext::from_f64(0.0))
            && -Ext::from_f64(40.0) > g.t_bills[corp] + g.cash[corp]
            && 2 < industry
            && industry < 70
        {
            f.phrase = s(__content.string(9191));
        }
    }
    if f.phrase3 == "D" {
        f.phrase2 = s(__content.string(9192));
        if corp % 3 == 1 {
            f.phrase2 = s("labors");
        }
        f.phrase = f.phrase2.clone() + __content.string(9193);
        f.phrase2 = PbStr::new();
    }
    if industry == 1 || industry == 2 || 69 < industry {
        if f.phrase3 == "D" {
            f.phrase = s(__content.string(9194));
        }
        f.phrase2 = PbStr::new();
    } else if !f.phrase2.is_empty() && !(g.tmp_total_debt * sf(0.1f32) >= g.t_bills[corp] + g.cash[corp]) {
        f.phrase2 = PbStr::new();
        if g.net_worth > Ext::from_f64(0.0) {
            f.phrase2 = s(__content.string(9195));
        }
        if g.net_worth + g.cash[corp] > g.tmp_total_debt * sf(0.25f32) {
            f.phrase2 = s(__content.string(9196));
        }
    }
    if f.phrase3 == "C" && f.phrase.len() < 18 {
        f.phrase = f.phrase.clone() + __content.string(9197);
    }
    lines.set(f.line_no, s("<< ") + &sym + " " + &f.phrase + &f.phrase2 + __content.string(9198))?;
    f.line_text = s("   ") + __content.string(9199) + &g.credit_rating_str;
    if 0 < instr(1, f.phrase.as_bytes(), __content.bytes(9200)) {
        f.line_text = f.line_text.clone() + __content.string(9201);
        f.phrase = PbStr::new();
    } else {
        f.phrase = PbStr::new();
        let not_fund = g.industry_num[corp] != 71;
        if f.phrase3 == "D" {
            f.phrase = s(__content.string(9202));
            if g.credit_rating_num[corp] == 0 && !(g.net_worth >= Ext::from_f64(0.0)) {
                f.phrase = s(__content.string(9203));
            }
            if !(g.net_worth >= Ext::from_f64(0.0)) && !(g.total_assets >= Ext::from_f64(0.0)) {
                f.phrase = s(__content.string(9204));
            }
        } else if not_fund && f.cash_flow_code == 1 {
            f.phrase = s(__content.string(9205));
        } else if not_fund && 6 < f.cash_flow_code {
            f.phrase = s(__content.string(9206));
            if f.phrase3 == "A" || f.phrase3 == "B" {
                f.phrase = replace(f.phrase.as_bytes(), b". It", b", but");
            }
        } else if not_fund && 4 < f.cash_flow_code {
            f.phrase = s(__content.string(9207));
            if 6 < g.credit_rating_num[corp] {
                f.phrase = replace(f.phrase.as_bytes(), b"and", b"but");
            }
            f.trend_pct = 0;
            if cast::<Ext, _>(g.bonds_issued[corp]) > Ext::from_f64(0.0) {
                f.trend_pct = to_long(val(mid(g.bond_issue_info[corp].as_bytes(), 1, 4).as_bytes()));
            }
            if g.cur_year == f.trend_pct {
                f.phrase = s(".");
            }
            if g.cur_year + 1 == f.trend_pct && g.cur_quarter == 4 && corp < g.sim_co_count {
                f.phrase = s(".");
            }
        } else {
            f.phrase = s(".");
        }
    }
    // LAB_004542d0
    lines.set(f.line_no + 1, f.line_text.clone() + &f.phrase)?;
    if g.net_worth > Ext::from_f64(0.0) && f.phrase3 == "D" && 40 < find_option_positions(g, ui, corp, 0, 0, 0) {
        f.line_no += 1;
        lines.set(f.line_no + 1, s(__content.string(9208)))?;
    }
    f.line_no += 2;
    if eq87(g.tmp_ext1, -Ext::from_f64(99.0)) {
        lines.set(f.line_no, s(__content.string(9209)))?;
        f.line_no += 1;
    }
    g.tmp_ext1 = Ext::from_f64(0.0);
    let limit = g.net_worth * sf(0.45f32);
    let naked = get_plr_naked_liab(g, ui, rng, corp, 0, 0);
    if naked > limit && g.net_worth > Ext::from_f64(0.0) {
        f.phrase = sym.clone();
        if 6 < g.credit_rating_num[corp] {
            f.phrase = s(__content.string(9210));
        }
        lines.set(f.line_no, s("<< ") + &f.phrase + __content.string(9211))?;
        f.line_no += 1;
        lines.set(f.line_no, s("   ") + __content.string(9212))?;
        f.line_no += 1;
    }
    if g.dividend_per_share[corp] > Ext::from_f64(0.0) && !eq87(g.proj_qtr_earnings[corp], Ext::from_f64(0.0)) {
        f.calc_value = calc_dividend_yield(g, corp);
        f.tmp_amount = to_quad((g.shares_outstanding[corp] * g.dividend_per_share[corp]) * Ext::from_f64(25.0));
        if f.calc_value > Ext::from_f64(10.0) {
            lines.set(
                f.line_no,
                s(__content.string(9213))
                    + &trim(&fmt(f.calc_value, "########.0"))
                    + "%",
            )?;
            let crn = g.credit_rating_num[corp];
            if cast::<Ext, _>(f.tmp_amount) > g.proj_qtr_earnings[corp] * Ext::from_f64(2.0) || crn < 7 {
                // LAB_00454715
                lines.app(f.line_no, ",")?;
                f.line_no += 1;
                lines.set(f.line_no, s(__content.string(9214)))?;
                if g.credit_rating_num[corp] < 7 {
                    lines.app(f.line_no, "due to")?;
                } else {
                    lines.app(f.line_no, "as the")?;
                }
                f.line_no += 1;
                if g.credit_rating_num[corp] < 7 {
                    lines.set(f.line_no, s(__content.string(9215)))?;
                    if f.unused_credit > Ext::from_f64(0.0) {
                        lines.app(f.line_no, __content.string(9216))?;
                    } else {
                        lines.app(f.line_no, __content.string(9217))?;
                    }
                } else {
                    lines.set(f.line_no, s(__content.string(9218)))?;
                }
            } else if g.credit_rating_num[corp] < 8 {
                lines.app(f.line_no, ".")?;
            } else {
                lines.app(f.line_no, ",")?;
                f.line_no += 1;
                lines.set(f.line_no, s(__content.string(9219)))?;
            }
            f.line_no += 1;
        }
    }
    Ok(())
}

/// Goodwill, asbestos, strike, superfund, malicious prosecution, frozen credit line, antitrust
/// lawsuits, pending merger (0x454aae .. 0x455890).
fn situations_section(g: &mut Globals, f: &mut F, lines: &mut Lines) -> R {
    let __content = crate::content::current();
    let corp = f.corp_id;
    let sym = f.sym(g);
    if 130 < corp && g.goodwill[corp] > Ext::from_f64(0.0) && g.net_worth > Ext::from_f64(0.0) && g.goodwill[corp] > g.net_worth {
        lines.set(f.line_no, s(__content.string(9220)))?;
        f.line_no += 1;
        lines.set(f.line_no, s(__content.string(9221)))?;
        f.line_no += 1;
    }
    if eq87((cast::<Ext, _>(g.asbestos_corp)).abs(), cast::<Ext, _>(corp)) {
        lines.set(f.line_no, s("<< ") + &sym + __content.string(9222))?;
        f.line_no += 1;
    }
    if cast::<i32, _>(g.strike_corp_code) % 10000 == corp {
        lines.set(f.line_no, s(__content.string(9223)))?;
        f.line_no += 1;
    }
    if cast::<i32, _>(g.superfund_corp) == corp {
        lines.set(f.line_no, s(__content.string(9224)))?;
        f.line_no += 1;
    }
    if g.malicious_suit_defendant == corp {
        lines.set(
            f.line_no,
            s("<< ")
                + &sym
                + __content.string(9225)
                + &g.ent_symbol[chk(g.malicious_suit_plaintiff, 1, 1600)?]
                + ".",
        )?;
        f.line_no += 1;
    }
    if f.industry != 1 {
        // BankLargestDebtor(LendingBank(corpId)): a lender outside 11..85 -> error 9
        let lender = chk(cast::<i32, _>(g.lending_bank[corp]), 11, 85)?;
        if g.bank_largest_debtor[lender] == corp
            && 6 < g.credit_rating_num[corp]
            && g.bank_debt[corp] > g.exchange_rate * Ext::from_f64(10000.0)
        {
            lines.set(f.line_no, s("<< ") + &sym + __content.string(9226))?;
            lines.set(f.line_no + 1, s(__content.string(9227)) + &g.ent_name[lender] + ".")?;
            f.line_no += 2;
        }
    }
    f.tmp_int = 0;
    f.trend_pct = 0;
    if corp == g.antitrust_plaintiff || corp == g.antitrust_defendant {
        f.antitrust_damages = g.antitrust_damages;
    }
    for idx in 1..=10 {
        f.idx = idx;
        f.phrase = g.lawsuit_rec[idx].clone();
        if mid(f.phrase.as_bytes(), 1, 1) == "1" {
            f.phrase2 = field(&f.phrase, 2);
            f.phrase3 = field(&f.phrase, 3);
            if eq87(val(f.phrase2.as_bytes()), cast::<Ext, _>(corp)) {
                f.tmp_int = to_long(-Ext::from_f64(1.0) * val(f.phrase3.as_bytes()));
                f.suit_damages = to_quad(val(field(&f.phrase, 5).as_bytes()));
                break;
            }
            if eq87(val(f.phrase3.as_bytes()), cast::<Ext, _>(corp)) {
                f.trend_pct = to_long(-Ext::from_f64(1.0) * val(f.phrase2.as_bytes()));
                f.suit_damages = to_quad(val(field(&f.phrase, 5).as_bytes()));
                break;
            }
        }
    }
    let damages_text = |g: &Globals, x: i64, amt_fmt: &str| {
        trim(&g.currency_symbol) + &trim(&fmt(cast::<Ext, _>(x), amt_fmt)) + " " + &g.million_prefix_lc + "illion."
    };
    if corp == g.antitrust_plaintiff && 10 < g.antitrust_defendant {
        lines.set(
            f.line_no,
            s("<< ")
                + &sym
                + __content.string(9228)
                + &g.ent_symbol[chk(g.antitrust_defendant, 1, 1600)?],
        )?;
        if !(cast::<Ext, _>(f.antitrust_damages) > Ext::from_f64(0.0)) {
            lines.app(f.line_no, ".")?;
        } else {
            lines.app(f.line_no, ",")?;
            f.line_no += 1;
            lines.set(f.line_no, s(__content.string(9229)) + &damages_text(g, f.antitrust_damages, f.amt_fmt))?;
        }
        f.line_no += 1;
    }
    if f.trend_pct < 0 {
        f.trend_pct = to_long((cast::<Ext, _>(f.trend_pct)).abs());
        lines.set(
            f.line_no,
            s(__content.string(9230)) + &g.ent_symbol[chk(f.trend_pct, 1, 1600)?],
        )?;
        if !(cast::<Ext, _>(f.suit_damages) > Ext::from_f64(0.0)) {
            lines.app(f.line_no, ".")?;
        } else {
            lines.app(f.line_no, ",")?;
            f.line_no += 1;
            lines.set(f.line_no, s(__content.string(9229)) + &damages_text(g, f.suit_damages, f.amt_fmt))?;
        }
        f.line_no += 1;
    }
    if corp == g.antitrust_defendant && 10 < g.antitrust_plaintiff {
        let plaintiff = chk(g.antitrust_plaintiff, 1, 1600)?;
        lines.set(
            f.line_no,
            s("<< ") + &sym + __content.string(9231) + &g.ent_symbol[plaintiff] + ".",
        )?;
        if cast::<Ext, _>(f.antitrust_damages) > Ext::from_f64(0.0) {
            f.line_no += 1;
            lines.set(
                f.line_no,
                s("   ") + &g.ent_name[plaintiff] + __content.string(9232) + &damages_text(g, f.antitrust_damages, f.amt_fmt),
            )?;
        }
        f.line_no += 1;
    }
    if f.tmp_int < 0 {
        f.tmp_int = to_long((cast::<Ext, _>(f.tmp_int)).abs());
        let plaintiff = chk(f.tmp_int, 1, 1600)?;
        lines.set(
            f.line_no,
            s(__content.string(9233)) + &g.ent_symbol[plaintiff] + ".",
        )?;
        if cast::<Ext, _>(f.suit_damages) > Ext::from_f64(0.0) {
            f.line_no += 1;
            lines.set(
                f.line_no,
                s("   ") + &g.ent_name[plaintiff] + __content.string(9234) + &damages_text(g, f.suit_damages, f.amt_fmt),
            )?;
        }
        f.line_no += 1;
    }
    if g.merger_acquirer == corp || g.merger_target == corp {
        lines.set(
            f.line_no,
            s(__content.string(9235)) + &g.ent_name[chk(g.merger_acquirer, 1, 1600)?] + __content.string(9236),
        )?;
        f.line_no += 1;
        lines.set(
            f.line_no,
            s(__content.string(9237)) + &g.ent_name[chk(g.merger_target, 1, 1600)?] + __content.string(9238),
        )?;
        f.line_no += 1;
    }
    Ok(())
}

/// The company's own bond issue: maturity, ability to repay, price, conversion (0x455890 .. 0x45662b).
fn bonds_section(g: &mut Globals, f: &mut F, lines: &mut Lines) -> R {
    let __content = crate::content::current();
    let corp = f.corp_id;
    let sym = f.sym(g);
    if !(cast::<Ext, _>(g.bonds_issued[corp]) > Ext::from_f64(0.0)) {
        return Ok(());
    }
    let maturity = || val(mid(g.bond_issue_info[corp].as_bytes(), 1, 4).as_bytes());
    f.year = to_long(maturity());
    f.tmp_id = f.year.wrapping_sub(g.cur_year);
    if !(f.tmp_id < 7) {
        return Ok(());
    }
    f.phrase = trim(&g.currency_suffix);
    if !f.phrase.is_empty() {
        f.phrase = s("(") + &f.phrase + ") ";
    }
    f.line_text = s("<< ")
        + &sym
        + " has a "
        + &trim(&g.currency_symbol)
        + &trim(&fmt(cast::<Ext, _>(g.bonds_issued[corp]), f.amt_fmt))
        + " "
        + &g.million_prefix_lc
        + "illion "
        + &f.phrase;
    f.phrase2 = s(__content.string(9239)) + &str15(cast::<Ext, _>(f.year));
    if eq87(maturity(), cast::<Ext, _>(g.cur_year)) && (1 < g.cur_quarter || corp < g.sim_co_count) {
        f.phrase2 = s(__content.string(9240));
    }
    lines.set(f.line_no, f.line_text.clone() + __content.string(9241) + &f.phrase2 + ".")?;
    f.line_no += 1;
    f.phrase3 = PbStr::new();
    let liquid = |g: &Globals| g.t_bills[corp] + g.cash[corp];
    if eq87(g.unused_credit_line, Ext::from_f64(0.0))
        && cast::<Ext, _>(g.bonds_issued[corp]) > liquid(g)
        && liquid(g) > Ext::from_f64(0.0)
        && f.phrase.is_empty()
    {
        f.phrase3 = s("only ");
    }
    if f.industry != 1 && f.tmp_id < 5 {
        if eq87(g.unused_credit_line, Ext::from_f64(0.0)) {
            if liquid(g) > Ext::from_f64(0.0) {
                f.sym_dot_pat = s(__content.string(9242));
            } else if eq87(liquid(g), Ext::from_f64(0.0)) {
                f.sym_dot_pat = PbStr::new();
            }
            if g.currency_suffix.is_empty() {
                f.sym_dot_pat = s(__content.string(9243)) + &f.sym_dot_pat;
            }
            if !(liquid(g) >= Ext::from_f64(0.0)) {
                f.sym_dot_pat = s(__content.string(9244));
            }
            lines.set(
                f.line_no,
                s(__content.string(9245))
                    + &f.sym_dot_pat
                    + " of "
                    + &f.phrase3
                    + &trim(&g.currency_symbol)
                    + &trim(&fmt(g.cash[corp] + g.t_bills[corp], f.amt_fmt))
                    + " "
                    + &g.million_prefix_lc
                    + "illion "
                    + &f.phrase
                    + "at",
            )?;
            if !(g.t_bills[corp] + g.cash[corp].abs() >= sf(0.1f32)) {
                lines.set(f.line_no, s(__content.string(9246)))?;
            }
        } else {
            lines.set(f.line_no, s(__content.string(9247)))?;
        }
        f.line_no += 1;
        f.line_text = s(__content.string(9248));
        if eq87(g.unused_credit_line, Ext::from_f64(0.0)) && cast::<Ext, _>(g.bonds_issued[corp]) > liquid(g) {
            lines.set(f.line_no, f.line_text.clone() + __content.string(9249))?;
        } else {
            // LAB_0045609b
            f.line_text = f.line_text.clone() + __content.string(9250);
            f.phrase2 = s(__content.string(9251));
            if eq87(maturity(), cast::<Ext, _>(g.cur_year)) {
                if g.cur_quarter == 1 && corp < g.sim_co_count {
                    f.phrase2 = s(__content.string(9252));
                }
                if 1 < g.cur_quarter {
                    f.phrase2 = s(__content.string(9075));
                }
            }
            lines.set(f.line_no, f.line_text.clone() + &f.phrase2)?;
            if eq87(maturity(), cast::<Ext, _>(g.cur_year))
                && g.bond_price[corp].get() > Ext::from_f64(135.0)
                && 1 < g.credit_rating_num[corp]
                && 0 < instr(1, g.bond_issue_info[corp].as_bytes(), b"cv")
            {
                f.line_no += 1;
                lines.set(f.line_no, s(__content.string(9253)))?;
            }
            if g.credit_rating_str == "D" {
                if g.credit_rating_num[corp] == 0 {
                    lines.set(f.line_no, s(__content.string(9254)))?;
                } else {
                    f.line_no += 1;
                    lines.set(
                        f.line_no,
                        s(__content.string(9255)) + &str15(cast::<Ext, _>(f.year)) + ".)",
                    )?;
                }
            }
        }
        f.line_no += 1;
    }
    lines.set(
        f.line_no,
        s("   ") + &sym + __content.string(9256) + &str15(int(g.bond_price[corp].get())) + __content.string(3014),
    )?;
    if 0 < instr(1, g.bond_issue_info[corp].as_bytes(), b"cv") {
        lines.repl(f.line_no, b"bonds", __content.bytes(9257))?;
    }
    if 0 < instr(1, g.bond_issue_info[corp].as_bytes(), b"cv")
        && g.bond_price[corp].get() > Ext::from_f64(150.0)
        && maturity() > cast::<Ext, _>(g.cur_year)
    {
        f.line_no += 1;
        lines.set(f.line_no, s(__content.string(9258)))?;
    }
    f.line_no += 1;
    if eq87(g.bank_debt[corp], Ext::from_f64(0.0)) && 85 < corp {
        lines.set(f.line_no, s(__content.string(9259)))?;
        f.line_no += 1;
    }
    Ok(())
}

/// Capital taxes, public float, control, tax loss carryover, shares, shorts, options, country
/// (0x45662b .. 0x45787b).
fn taxes_and_control_section(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut F, lines: &mut Lines) -> R {
    let __content = crate::content::current();
    let corp = f.corp_id;
    let sym = f.sym(g);
    let industry = f.industry;
    calc_entity_financials(g, ui, rng, corp);
    let _ = calc_capital_taxes(g, corp);
    if 0 < g.carbon_tax_rate && g.industry_num[corp] != 71 {
        f.phrase = s("<< ") + &sym + __content.string(9260);
        if matches!(industry, 3 | 7 | 8 | 9 | 11 | 12 | 16 | 18 | 20 | 23 | 25 | 33 | 50 | 51 | 52 | 55 | 67 | 68 | 69) {
            lines.set(f.line_no, f.phrase.clone())?;
            if eq87(g.carbon_tax_qtr, Ext::from_f64(0.0)) && !(g.business_assets[corp] >= g.exchange_rate * Ext::from_f64(1000.0)) {
                lines.set(
                    f.line_no,
                    s(__content.string(9261)) + &g.tmp_symbol + __content.string(9262),
                )?;
            } else if g.country_idx[corp] == 8 || g.country_idx[corp] == 21 {
                f.line_no += 1;
                lines.set(
                    f.line_no,
                    s(__content.string(9263))
                        + &sym
                        + __content.string(9264)
                        + &g.country_name[g.country_idx[corp]]
                        + __content.string(9265),
                )?;
            }
        } else {
            f.phrase = replace(f.phrase.as_bytes(), __content.bytes(9266), __content.bytes(9267));
            lines.set(f.line_no, f.phrase.clone())?;
        }
        f.line_no += 1;
    }
    if 0 < g.corp_shares_tax_country && g.corp_shares_tax_due > Ext::from_f64(0.0) {
        lines.set(f.line_no, s(__content.string(9268)) + &sym + " owns.")?;
        f.line_no += 1;
    }
    if 0 < g.health_care_tax_rate {
        lines.set(f.line_no, s(__content.string(9269)))?;
        if eq87(g.health_care_tax_qtr, Ext::from_f64(0.0)) {
            lines.repl(f.line_no, __content.bytes(9266), __content.bytes(9270))?;
        }
        if matches!(industry, 4 | 21 | 22 | 56) {
            f.line_no += 1;
        } else {
            lines.set(f.line_no, PbStr::new())?;
        }
    }
    if 0 < g.holding_co_tax_rate && g.holding_co_tax_due > Ext::from_f64(0.0) {
        lines.set(f.line_no, s("<< ") + &sym + __content.string(9271))?;
        f.line_no += 1;
    }
    if 0 < g.oil_windfall_tax_rate
        && !(sf(0.8f32) > cast::<Ext, _>(g.oil_price))
        && (industry == 7 || industry == 8 || industry == 63)
    {
        lines.set(f.line_no, s(__content.string(9272)))?;
        if eq87(g.oil_windfall_tax_qtr, Ext::from_f64(0.0)) {
            lines.set(
                f.line_no,
                s("<< ") + &g.tmp_symbol + __content.string(9273),
            )?;
        }
        f.line_no += 1;
    }
    update_corp_control(g, ui, rng, corp);
    if g.public_pct == 0 {
        lines.set(f.line_no, s(__content.string(9274)))?;
        f.line_no += 1;
    }
    f.tmp_id = cast::<i32, _>(g.wholly_owned_by);
    let vc = g.voting_controller;
    if 10 < f.tmp_id {
        lines.set(f.line_no, s("<< ") + &sym + __content.string(9275) + &g.ent_name[f.tmp_id] + ".")?;
        f.line_no += 1;
        if 0 < vc && f.tmp_id != vc {
            f.phrase = PbStr::new();
            if vc <= g.num_players {
                f.phrase = s(__content.string(9276));
            }
            lines.set(f.line_no, s(__content.string(9277)) + &f.phrase + &g.ent_name[chk(vc, 1, 1600)?] + ".")?;
            f.line_no += 1;
        }
    } else if 0 < vc && corp != vc {
        let vc = chk(vc, 1, 1600)?;
        f.phrase = PbStr::new();
        if vc <= g.num_players {
            f.phrase = s(__content.string(9278));
        }
        lines.set(f.line_no, s(__content.string(9279)) + &f.phrase + &g.ent_name[vc] + ".")?;
        f.line_no += 1;
        if get_controller_type(g, corp) == 1 {
            if g.auto_pilot[corp] == 1 {
                lines.set(f.line_no, s(__content.string(9280)) + &g.ent_name[vc] + ".)")?;
                f.line_no += 1;
            } else {
                lines.repl(f.line_no - 1, b"by", __content.bytes(9281))?;
            }
        }
        let largest = cast::<i32, _>(g.largest_holder);
        if 0 < largest {
            let largest = chk(largest, 1, 1600)?;
            lines.set(f.line_no, s(__content.string(9282)) + &g.ent_name[largest] + ".")?;
            if largest != g.voting_controller {
                let t = remove(lines.get(f.line_no)?.as_bytes(), b"also ");
                lines.set(f.line_no, t)?;
            }
            if largest == cast::<i32, _>(g.wholly_owned_by) {
                lines.repl(f.line_no, b"largest", b"sole")?;
            }
            f.line_no += 1;
        }
    }
    if g.industry_num[corp] == 71 {
        g.tax_loss_carry[corp] = cast::<Ext, _>(g.extraordinary_pl[corp]);
    }
    if !(g.tax_loss_carry[corp] >= -Ext::from_f64(300.0)) {
        f.tmp_long = to_quad(g.tax_loss_carry[corp].abs());
        f.line_text = s("<< ") + &sym + __content.string(9283);
        if industry == 71 {
            f.line_text = replace(f.line_text.as_bytes(), b"tax", b"capital");
        }
        lines.set(
            f.line_no,
            f.line_text.clone()
                + &g.currency_symbol
                + &trim(&fmt(cast::<Ext, _>(f.tmp_long), f.amt_fmt))
                + " "
                + &g.million_prefix_lc
                + "illion"
                + &g.currency_suffix
                + ".",
        )?;
        f.line_no += 1;
    }
    lines.set(
        f.line_no,
        s(__content.string(9284))
            + &trim(&fmt(Ext::from_f64(100.0) * g.shares_outstanding[corp], f.eps_fmt))
            + " "
            + &g.million_prefix_lc
            + __content.string(9285),
    )?;
    if find_short_positions(g, ui, rng, corp, 0) == 2 {
        lines.app(f.line_no, __content.string(9286))?;
        f.line_no += 1;
        lines.set(f.line_no, s(__content.string(9287)))?;
    } else {
        lines.app(f.line_no, ".")?;
    }
    f.line_no += 1;
    find_option_positions(g, ui, 0, corp, 0, 0);
    if g.has_options_outstanding[corp] == 1 {
        lines.set(f.line_no, s(__content.string(9288)) + &sym + __content.string(9289))?;
        if find_option_positions(g, ui, 1, corp, 0, 0) != 0 {
            let p1 = g.player_name[1].clone();
            lines.repl(f.line_no, __content.bytes(9290), p1.as_bytes())?;
        }
        f.tmp_id = find_option_positions(g, ui, g.cur_player, corp, 0, 0);
        if f.tmp_id != 0 {
            lines.set(
                f.line_no,
                s("<< ") + &g.cur_player_name + __content.string(9291) + &sym + __content.string(9289),
            )?;
        }
        f.line_no += 1;
    }
    lines.set(
        f.line_no,
        s("<< ") + &f.corp_name + __content.string(9292) + &g.country_name[chk(g.country_idx[corp], 0, 55)?] + ".",
    )?;
    f.line_no += 3;
    Ok(())
}

/// Analyst's recommendation and target price, recent headlines, the report (0x45787b .. 0x4581db).
fn rating_and_news_section(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut F, lines: &mut Lines) -> R {
    let __content = crate::content::current();
    let corp = f.corp_id;
    let sym = f.sym(g);
    if g.public_pct < 1 {
        g.cur_analyst_rating = 0;
    } else {
        g.valuation_mode = 4;
        calc_stock_price(g, ui, rng, corp, 0);
        g.cur_analyst_rating = calc_analyst_rating(g, ui, rng, corp);
        f.idx = g.cur_analyst_rating;
        f.phrase = s("HOLD");
        if f.idx == 2 {
            f.phrase = s("BUY");
        }
        if f.idx == 1 {
            f.phrase = s(__content.string(9293));
        }
        if f.idx == 4 {
            f.phrase = s("SELL");
        }
        if f.idx == 5 {
            f.phrase = s(__content.string(6179));
        }
        lines.set(f.line_no, s(__content.string(9294)) + &f.phrase)?;
        f.line_no += 1;
    }
    if 0 < g.public_pct && g.industry_num[corp] != 71 {
        g.calc_stock_price = sf(0.4f32) * (g.calc_stock_price / g.shares_outstanding[corp]);
        f.tmp_id = to_long(sf(2.6f32) * g.calc_stock_price);
        let same_parity = corp % 3 == (g.cur_quarter + 4) % 2;
        if f.idx < 3 {
            f.tmp_id = to_long(sf(1.05f32) * cast::<Ext, _>(f.tmp_id));
            if f.idx == 1 {
                f.tmp_id = to_long(sf(1.05f32) * cast::<Ext, _>(f.tmp_id));
            }
            if same_parity {
                f.tmp_id = to_long(sf(1.15f32) * cast::<Ext, _>(f.tmp_id));
            }
        } else if 3 < f.idx {
            f.tmp_id = to_long(sf(0.95f32) * cast::<Ext, _>(f.tmp_id));
            if f.idx == 5 {
                f.tmp_id = to_long(sf(0.95f32) * cast::<Ext, _>(f.tmp_id));
            }
            if same_parity {
                f.tmp_id = to_long(sf(0.9f32) * cast::<Ext, _>(f.tmp_id));
            }
        }
        g.tmp_share_price = g.market_cap[corp] / g.shares_outstanding[corp];
        f.tmp_int = 0;
        if f.idx < 4 && !(g.tmp_share_price * sf(1.05f32) >= cast::<Ext, _>(f.tmp_id)) {
            f.tmp_int = 1;
        }
        if 3 < f.idx && g.tmp_share_price * sf(0.95f32) > cast::<Ext, _>(f.tmp_id) {
            f.tmp_int = 1;
        }
        if g.credit_rating_num[corp] < 2 {
            f.tmp_int = 1;
            f.tmp_id = to_long(cast::<Ext, _>(f.tmp_id) * (sf(0.1f32) * rng.rnd() + sf(0.2f32)));
            if g.credit_rating_num[corp] == 0 {
                f.tmp_id = 0;
            }
        }
        if f.tmp_int == 1 {
            f.phrase = trim(&fmt(cast::<Ext, _>(f.tmp_id), f.amt_fmt));
            if f.tmp_id == 0 {
                f.phrase = s("ZERO");
            }
            lines.set(f.line_no, s(__content.string(9295)) + &f.phrase)?;
            if f.tmp_id < 10 && 1 < g.credit_rating_num[corp] {
                lines.set(f.line_no, PbStr::new())?;
            } else {
                f.line_no += 1;
            }
        }
    }
    // LAB_00457d66
    f.tmp_int = f.line_no + 3;
    f.line_no += 4;
    f.tmp_id = 0;
    f.phrase2 = s(" ") + &sym + " ";
    f.phrase3 = s("(") + &sym + ")";
    f.sym_dot_pat = s(" ") + &sym + ".";
    for idx in 1..=60 {
        f.idx = idx;
        f.phrase = s(" - ") + &g.news_headline[idx] + " ";
        let p = f.phrase.as_bytes();
        if (0 < instr(1, p, f.phrase2.as_bytes())
            || 0 < instr(1, p, f.phrase3.as_bytes())
            || 0 < instr(1, p, f.sym_dot_pat.as_bytes()))
            && instr(1, p, __content.bytes(1041)) == 0
        {
            f.tmp_id = 1;
            f.line_no += 1;
            lines.set(f.line_no, rtrim(f.phrase.as_bytes()))?;
        }
    }
    if f.tmp_id == 1 {
        lines.set(f.tmp_int, s(__content.string(8337)) + &f.corp_name + ":")?;
        let n = lines.get(f.tmp_int)?.len() - 3;
        lines.set(f.tmp_int + 1, s("   ") + &string_fill(n, b"-"))?;
    }
    f.line_no += 2;
    lines.set(f.line_no + 2, s("@END"))?;
    for line_no in 1..=90 {
        g.text_lines[line_no] = lines.get(line_no)?;
    }
    g.text_box_title = s(__content.string(9296))
        + &ucase(g.ent_name[g.cur_player].as_bytes(), CaseMode::Ansi)
        + " on "
        + &f.corp_name
        + " ===";
    g.text_has_links = 1;
    g.text_box_mode = s("84");
    show_text_report(g, ui, rng);
    g.research_report_corp = 0;
    Ok(())
}
