//! The 3-month cash-flow projection report and its uses: `ShowCashFlowProjection` (0x6a20e0, the
//! dispatcher; insurers 86..130, operating corps and funds are handled inline), the bank variant
//! `BankCashFlowProjection` (0x6a99f7), the players' annual portfolio cash flow `PlayerCashFlowProjection`
//! (0x6afc4d), the bank-loan repayment schedule `CalcBankLoanPrincipalDue` (0x6a8805) and the pop-up
//! `CashFlowAlert` (0x6a9490), plus small unported callees (`CalcPlayerIncomeTax`,
//! `GetEtfAdviserHumanStatus`, `MsgCorpOnlyFunction`).
//!
//! `mode` of the projections: `< 1` builds and shows the report; `1` only computes and returns the cash
//! status (1 = positive cash flow, 2 = negative, 3..5 = deficit over 25 % / 50 % of liquid funds with /
//! without a credit line, 6 = cash goes negative but the credit line covers it, 7 = it does not, 8 = no
//! credit line at all) and sets `TmpExt1 = -99` when the cash flow is positive but the status is > 1;
//! `2` additionally lets the AI adjust growth and dividends (corporations only; mode 2 also skips the
//! `CalcEntityFinancials` refresh). The report lines are built in every mode.
//!
//! Numeric conventions (see `logic::finance`): `EXT` uses software x87 arithmetic; SINGLE locals are rounded with `as f32`;
//! SINGLE constants of the original are written `s(0.25)`. The association order of every float
//! expression follows the machine code (the decompiled view reorders several sums).

// Statements keep the original BASIC shape on purpose.
#![allow(
    clippy::manual_range_contains,
    clippy::assign_op_pattern,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::if_same_then_else,
    clippy::neg_multiply,
    clippy::too_many_lines,
    clippy::int_plus_one,
    clippy::nonminimal_bool,
    clippy::field_reassign_with_default,
    clippy::collapsible_match
)]
use crate::pb::real::{Ext, cast};


use crate::logic::control::update_corp_control;
use crate::logic::earnings::{
    balance_cash_t_bills, calc_adviser_fee_income, calc_bank_deposit_cost, calc_ceo_pay, goodwill_write_down,
};
use crate::logic::finance::{
    calc_capital_taxes, calc_entity_financials, get_bank_cd_rate, get_controller_type, get_industry_category,
    get_managing_player, get_market_interest_rate,
};
use crate::logic::futures::calc_phys_commod_storage_cost;
use crate::logic::holdings::find_stock_holding;
use crate::logic::news::{is_in_recent_headlines, post_news};
use crate::logic::report::{describe_defunct_entity, number_to_words, show_quick_message, show_text_report};
use crate::logic::reports_corp::assign_etf_adviser;
use crate::logic::swaps::process_swap_payments;
use crate::logic::valuation::{calc_dividend_yield, calc_industry_outlook, calc_working_capital_ratio};
use crate::pb::numeric::format;
use crate::pb::numeric::{str_num, val};
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{instr, len, ltrim, mid, parse_any, remove_any, replace, right, rtrim, string_fill, ucase, CaseMode};
use crate::pb::numeric::{to_long, to_quad};
use crate::state::Globals;
use crate::ui::Ui;

/// A SINGLE constant of the original (`fld/fmul dword [const]`) as the x87 sees it.
#[inline]
fn s(x: f32) -> Ext {
    cast::<Ext, _>(x)
}

fn st(x: &str) -> PbStr {
    PbStr::from(x)
}

/// `FORMAT$(x, mask)`.
fn fmt(x: Ext, mask: &PbStr) -> PbStr {
    PbStr(format(x, mask.as_bytes()))
}

/// `STR$(n)` of a LONG.
fn str_l(n: i32) -> PbStr {
    PbStr::from(str_num(cast::<Ext, _>(n), 15).as_str())
}

// ---------------------------------------------------------------------------------------------------------
// Small callees
// ---------------------------------------------------------------------------------------------------------

/// Port of CalcPlayerIncomeTax (0x508797).
/// A player's estimated income tax for the year: ordinary income (`AccruedPL + ExtraordinaryPL +
/// DivRecvd1 + DivRecvd2`) at `CorpTaxRate + 5` % plus positive `PlayerCapGains` at `CapGainsTaxRate` %
/// (a net ordinary loss offsets the gains). Leaves the taxable total (ordinary + gains) in `TmpExt1`.
pub fn calc_player_income_tax(g: &mut Globals, player_id: i32) -> Ext {
    let i = player_id;
    let mut cap_gains = g.player_cap_gains[i];
    if cap_gains < Ext::from_f64(0.0) {
        cap_gains = Ext::from_f64(0.0);
    }
    // asm: ((ExtraordinaryPL + AccruedPL) + DivRecvd1) + DivRecvd2
    let mut ordinary_income = ((cast::<Ext, _>(g.extraordinary_pl[i]) + g.accrued_pl[i]) + g.div_recvd1[i]) + g.div_recvd2[i];
    let tax_due;
    let cgt = s(0.01f32) * cast::<Ext, _>(g.cap_gains_tax_rate);
    if ordinary_income >= Ext::from_f64(0.0) {
        tax_due = cap_gains * cgt + ordinary_income * (cast::<Ext, _>(5 + g.corp_tax_rate) * s(0.01f32));
        ordinary_income = cap_gains + ordinary_income;
    } else {
        ordinary_income = cap_gains + ordinary_income;
        if ordinary_income > Ext::from_f64(0.0) {
            tax_due = ordinary_income * cgt;
        } else {
            tax_due = Ext::from_f64(0.0);
        }
    }
    g.tmp_ext1 = ordinary_income;
    tax_due
}

/// Port of GetEtfAdviserHumanStatus (0x64ff35).
/// For an active fund (1581..1600, industry 71): 2 if its adviser is controlled by the current player,
/// 1 if by another human player, else 0. Sets a missing `CtrlSinceYear` to `CurYear - 3`; a fund whose
/// adviser is not an active corporation 86..1600 gets a new one (`AssignEtfAdviser(fund, 0, 0)`).
pub fn get_etf_adviser_human_status(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, etf_id: i32) -> i32 {
    let mut result = 0;
    let fund_id = etf_id;
    if 1580 < fund_id && fund_id < 1601 && g.entity_status[fund_id] == 1 && g.industry_num[fund_id] == 71 {
        if g.ctrl_since_year[fund_id] == 0 {
            g.ctrl_since_year[fund_id] = g.cur_year - 3;
        }
        let adviser_id = g.fund_adviser[fund_id];
        if adviser_id < 86 || 1600 < adviser_id {
            assign_etf_adviser(g, ui, rng, fund_id, 0, 0);
        } else if g.entity_status[adviser_id] != 1 {
            assign_etf_adviser(g, ui, rng, fund_id, 0, 0);
        } else if (g.industry_num[adviser_id] == 2 || g.industry_num[adviser_id] == 37)
            && g.corp_controller[adviser_id] <= 10
            && get_controller_type(g, adviser_id) == 1
        {
            let mut status = 1;
            if g.cur_player == g.corp_controller[adviser_id] {
                status += 1;
            }
            result = status;
        }
    }
    result
}

/// Port of MsgCorpOnlyFunction (0x5b8512).
/// Pop-up " [runtime resource wording]  [runtime resource wording] "
pub fn msg_corp_only_function(g: &mut Globals, ui: &mut dyn Ui) {
    let __content = crate::content::current();
    let mut msg_text =
        st(__content.string(2324)) + __content.string(2325);
    show_quick_message(g, ui, &mut msg_text);
}

// ---------------------------------------------------------------------------------------------------------
// CalcBankLoanPrincipalDue
// ---------------------------------------------------------------------------------------------------------

/// Port of CalcBankLoanPrincipalDue (0x6a8805).
/// The bank-loan principal corporation `entity_ref` (>= 86 with a bank loan, else 0) is expected to repay
/// next quarter: `(11 - rating) * 0.5 %` of the loan, 15 % for flush unmanaged corps, 20 % for ratings
/// below 2 (whose T-bills are moved to cash and whose repayment is capped at the cash), 15 % with a
/// covenant-violation headline for big weak corps (`NewsMsg`, only when `rating_override` is not -1; a
/// pending `NewsMsg` is posted first), 100 % for small cash-rich funds; plus extra paydowns for liquid
/// borrowers, capped at the loan. `rating_override` -1 = use `CreditRatingNum`. A lender controlled by
/// the same player as the borrower skips the rating rules.
pub fn calc_bank_loan_principal_due(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    entity_ref: i32,
    rating_override: i32,
) -> Ext {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError ("CalcBankLoanPrincipalDue") — error path not ported.
    let entity_id = entity_ref;
    let e = entity_id;
    if g.bank_debt[e] == Ext::from_f64(0.0) || e < 86 {
        return Ext::from_f64(0.0);
    }
    let mut rating = rating_override;
    if rating == -1 {
        rating = g.credit_rating_num[e];
    }
    let mut repay_pct: f32 = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(11 - rating) * s(0.005f32)));
    if !(g.bank_debt[e] * s(0.35f32) >= g.t_bills[e] + g.cash[e]) {
        if get_managing_player(g, e) == 0 {
            repay_pct = 0.15f32;
        }
    }
    g.lender_id = cast::<i32, _>(g.lending_bank[e]);
    let lender = g.lender_id;
    let same_controller = 0 < get_controller_type(g, lender) && g.corp_controller[e] == g.corp_controller[lender];
    if !same_controller {
        if rating < 2 {
            repay_pct = 0.2f32;
        } else if s(0.15f32) > cast::<Ext, _>(repay_pct) && g.credit_rating_num[e] < 5 && g.bank_debt[e] >= g.exchange_rate * Ext::from_f64(350.0) {
            let mut full = false;
            if 69 < g.industry_num[e]
                && g.hold_stock_id[(0, e)] == 0
                && !(g.bank_debt[e] >= g.t_bills[e] + g.cash[e])
                && !(g.bank_debt[e] >= g.exchange_rate * Ext::from_f64(100.0))
            {
                repay_pct = 1.0f32;
                full = true;
            }
            if !full {
                repay_pct = 0.15f32;
                if !g.news_msg.is_empty() {
                    post_news(g, ui, 0, 0);
                }
                if g.rand_seed % 3 == 1 {
                    let mut news_suffix = st(__content.string(2326));
                    if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
                        news_suffix = st(__content.string(2327));
                    }
                    if rating_override != -1 {
                        g.news_msg = g.ent_symbol[e].clone() + &news_suffix;
                        if 0 < instr(1, g.news_msg.as_bytes(), b"coven") && 0 < is_in_recent_headlines(g, __content.bytes(2328)) {
                            g.news_msg = PbStr::new();
                        }
                        if 0 < instr(1, g.news_msg.as_bytes(), b"accel") && 0 < is_in_recent_headlines(g, __content.bytes(2329))
                        {
                            g.news_msg = PbStr::new();
                        }
                    }
                }
            }
        }
    }
    let mut principal_due = g.bank_debt[e] * cast::<Ext, _>(repay_pct);
    if rating < 2 && g.t_bills[e] > Ext::from_f64(0.0) {
        g.cash[e] = g.t_bills[e] + g.cash[e];
        g.t_bills[e] = Ext::from_f64(0.0);
    }
    if rating < 2 && g.cash[e] > Ext::from_f64(0.0) && principal_due > g.cash[e] {
        principal_due = g.cash[e];
    }
    if !(Ext::from_f64(1500.0) * g.exchange_rate >= g.t_bills[e] + g.cash[e])
        && g.bank_debt[e] > Ext::from_f64(850.0) * g.exchange_rate
        && !(g.bank_debt[e] >= g.t_bills[e] + g.cash[e])
    {
        principal_due = principal_due + Ext::from_f64(500.0) * g.exchange_rate;
    } else if rating < 2
        && g.cash[e] > g.exchange_rate * Ext::from_f64(500.0)
        && !(principal_due >= g.cash[e] * s(0.8f32))
        && g.bonds_issued[e] == 0
    {
        principal_due = g.cash[e] * s(0.8f32);
    }
    if !(g.bank_debt[e] >= g.exchange_rate * Ext::from_f64(5.0)) && g.cash[e] > g.bank_debt[e] * s(0.6f32) {
        principal_due = g.cash[e];
    }
    if principal_due > g.bank_debt[e] {
        principal_due = g.bank_debt[e];
    }
    if g.bank_debt[e] > Ext::from_f64(0.0) && !(g.bank_debt[e] >= (g.t_bills[e] + g.cash[e]) * s(0.1f32)) {
        principal_due = g.bank_debt[e];
    }
    if g.industry_num[e] == 71 {
        if get_etf_adviser_human_status(g, ui, rng, e) == 0 {
            if !(g.bank_debt[e] * s(0.5f32) >= g.t_bills[e] + g.cash[e]) {
                principal_due = g.bank_debt[e] * s(0.5f32);
            }
            if !(g.bank_debt[e] * s(1.3f32) >= g.t_bills[e] + g.cash[e]) {
                principal_due = g.bank_debt[e];
            }
        } else {
            principal_due = g.bank_debt[e] * s(0.05f32);
        }
    }
    principal_due
}

// ---------------------------------------------------------------------------------------------------------
// Shared report machinery of ShowCashFlowProjection / BankCashFlowProjection
// ---------------------------------------------------------------------------------------------------------

/// The locals of the corporate projections (EXT locals are `f64`).
#[derive(Default)]
struct Cf {
    lines: Vec<PbStr>,
    line_no: i32,
    num_fmt: PbStr,
    single_rule: PbStr,
    double_rule: PbStr,
    fmt_text: PbStr,
    currency_label: PbStr,
    coupon_rate: f32,
    maturity_year: i32,
    quarters_left: i32,
    slot: i32,
    sub_count: i32,
    sub_id: i32,
    cash_status: i32,
    swap_pl: i64,
    bonds_due: i64,
    work_amt: Ext,
    subprime_interest: Ext,
    tbill_interest: Ext,
    gov_bond_interest: Ext,
    bond_discount: Ext,
    bond_amort: Ext,
    net_bond_amort: Ext,
    maturing_bonds: Ext,
    corp_bond_interest: Ext,
    bond_coupon: Ext,
    gross_dividends: Ext,
    holding_dividend: Ext,
    div_received_ded: Ext,
    etf_dist_yield: Ext,
    extraordinary_gain: Ext,
    subs_taxable_income: Ext,
    subs_tax_reimb_in: Ext,
    subs_loss_reimb_out: Ext,
    bond_discount_amort: Ext,
    bond_premium_amort: Ext,
    advances_interest: Ext,
    ceo_pay: Ext,
    tax_credits: Ext,
    total_income: Ext,
    total_expenses: Ext,
    net_taxable_income: Ext,
    tentative_tax: Ext,
    adjusted_tax: Ext,
    income_after_nol: Ext,
    tax_benefit: Ext,
    parent_tax_reimb: Ext,
    capital_taxes: Ext,
    total_taxes: Ext,
    after_tax_cash_flow: Ext,
    dividend_payout: Ext,
    total_adjustments: Ext,
    proj_cash_flow: Ext,
    net_cash_flow: Ext,
    cash_in_3_months: Ext,
}

impl Cf {
    fn new() -> Self {
        let mut cf = Cf::default();
        cf.lines = vec![PbStr::new(); 1501];
        cf
    }
    fn set(&mut self, i: i32, x: PbStr) {
        self.lines[cast::<usize, _>(i)] = x;
    }
    fn get(&self, i: i32) -> PbStr {
        self.lines[cast::<usize, _>(i)].clone()
    }
    /// `lines(lineNo) = text + FORMAT$(x, numFmt)`.
    fn set_amt(&mut self, text: &str, x: Ext) {
        let l = st(text) + &fmt(x, &self.num_fmt);
        self.set(self.line_no, l);
    }
    /// `fmtText = FORMAT$(x, numFmt): VAL(fmtText)`.
    fn fv(&mut self, x: Ext) -> Ext {
        self.fmt_text = fmt(x, &self.num_fmt);
        val(self.fmt_text.as_bytes())
    }
    fn replace_line(&mut self, find: &str, with: &str) {
        let l = replace(self.get(self.line_no).as_bytes(), st(find).as_bytes(), st(with).as_bytes());
        self.set(self.line_no, l);
    }
    fn rule(&mut self) {
        let r = self.single_rule.clone();
        self.set(self.line_no, r);
    }
    fn double(&mut self) {
        let r = self.double_rule.clone();
        self.set(self.line_no, r);
    }

    /// numFmt by market cap and the two rule lines.
    fn formats(&mut self, g: &Globals, entity_ref: i32) {
        self.num_fmt = st("* #############.00");
        if g.market_cap[entity_ref] > Ext::from_f64(1000.0) {
            self.num_fmt = st("* ################");
        }
        self.single_rule = string_fill(40, b" ") + "------------------";
        self.double_rule = string_fill(40, b" ") + "==================";
    }

    /// Title lines 1, 2 and the currency heading (line 4); `lineNo = 5`.
    fn header(&mut self, g: &Globals, e: i32) {
    let __content = crate::content::current();
        let n = len(g.ent_name[e].as_bytes());
        self.set(1, st(__content.string(2330)) + &g.ent_name[e]);
        self.set(2, st("     ---------------------------------") + &string_fill(n, b"-"));
        self.currency_label = g.currency_suffix.clone();
        if self.currency_label.is_empty() || 0 < instr(1, self.currency_label.as_bytes(), b"SwFr") {
            self.currency_label = st(" ") + &g.currency_name;
        }
        let cl = self.currency_label.as_bytes();
        if 0 < instr(1, cl, b"Pound") || 0 < instr(1, cl, b"Dollar") || 0 < instr(1, cl, b"Franc") {
            if right(cl, 1) != "s" {
                self.currency_label = self.currency_label.clone() + "s";
            }
        }
        self.set(
            4,
            st(__content.string(2331))
                + &g.million_prefix_uc
                + __content.string(2332)
                + &ucase(self.currency_label.as_bytes(), CaseMode::Ansi)
                + "):",
        );
        self.line_no = 5;
    }

    fn subprime(&mut self, g: &mut Globals, rng: &mut Rng, e: i32) {
    let __content = crate::content::current();
        if g.subprime_secs[e] > 0 {
            let rate = get_market_interest_rate(g, rng, 3);
            self.subprime_interest = cast::<Ext, _>(g.subprime_secs[e]) * (s(0.0025f32) * rate);
            self.subprime_interest = self.fv(self.subprime_interest);
            self.set_amt(__content.string(2333), self.subprime_interest);
            self.line_no += 1;
        }
    }

    fn tbills(&mut self, g: &Globals, e: i32) {
    let __content = crate::content::current();
        if g.t_bills[e] > Ext::from_f64(0.0) {
            self.tbill_interest = g.t_bills[e] * (s(0.0025f32) * cast::<Ext, _>(g.t_bill_rate));
            self.tbill_interest = self.fv(self.tbill_interest);
            self.set_amt(__content.string(2334), self.tbill_interest);
            self.line_no += 1;
        }
    }

    /// `(maturityYear - CurYear) * 4 - (CurQuarter - 1)`, at least 1.
    fn gov_quarters(&mut self, g: &Globals, desc: &PbStr) {
        self.maturity_year = to_long(val(mid(desc.as_bytes(), 1, 4).as_bytes()));
        self.quarters_left =
            self.maturity_year.wrapping_sub(g.cur_year).wrapping_mul(4).wrapping_sub(g.cur_quarter - 1);
    }

    fn gov_bonds(&mut self, g: &Globals, e: i32) {
    let __content = crate::content::current();
        if g.tmp_gov_bonds_value > Ext::from_f64(0.0) {
            if g.long_gov_bond_face[e] > 0 {
                let desc = g.long_bond_desc.clone();
                self.coupon_rate = cast::<f32, _>(val(right(desc.as_bytes(), 5).as_bytes()));
                self.gov_bond_interest =
                    ((cast::<Ext, _>(g.long_gov_bond_face[e]) * s(0.01f32)) * cast::<Ext, _>(self.coupon_rate)) * s(0.25f32);
                self.gov_quarters(g, &desc);
                self.bond_discount = cast::<Ext, _>(g.long_gov_bond_face[e]) - g.lt_gov_bond_cost[e];
                if self.quarters_left < 1 {
                    self.quarters_left = 1;
                }
                self.bond_amort = self.bond_discount / cast::<Ext, _>(self.quarters_left);
                self.net_bond_amort = self.bond_amort;
            }
            if g.short_gov_bond_face[e] > 0 {
                let desc = g.short_bond_desc.clone();
                self.coupon_rate = cast::<f32, _>(val(right(desc.as_bytes(), 5).as_bytes()));
                self.gov_bond_interest = self.gov_bond_interest
                    + ((cast::<Ext, _>(g.short_gov_bond_face[e]) * s(0.01f32)) * cast::<Ext, _>(self.coupon_rate)) * s(0.25f32);
                self.gov_quarters(g, &desc);
                self.bond_discount = cast::<Ext, _>(g.short_gov_bond_face[e]) - g.st_gov_bond_cost[e];
                if self.quarters_left < 1 {
                    self.quarters_left = 1;
                }
                self.bond_amort = self.bond_discount / cast::<Ext, _>(self.quarters_left);
                if g.cur_year + 1 == self.maturity_year && g.cur_quarter == 4 {
                    self.bond_amort = self.bond_discount;
                    self.maturing_bonds = cast::<Ext, _>(g.short_gov_bond_face[e]);
                }
                self.net_bond_amort = self.net_bond_amort + self.bond_amort;
            }
            self.gov_bond_interest = self.fv(self.gov_bond_interest);
            self.set_amt(__content.string(2335), self.gov_bond_interest);
            self.line_no += 1;
        }
    }

    fn corp_bonds(&mut self, g: &Globals, e: i32) {
    let __content = crate::content::current();
        if g.tmp_corp_bonds_value > Ext::from_f64(0.0) {
            for slot in 1..=15 {
                let h = g.hold_bond_id[(slot, e)];
                if h == 0 || g.credit_rating_num[h] < 2 {
                    continue;
                }
                let info = g.bond_issue_info[h].clone();
                self.coupon_rate = cast::<f32, _>(val(right(info.as_bytes(), 5).as_bytes()));
                self.bond_coupon = ((cast::<Ext, _>(g.hold_bond_face[(slot, e)]) * s(0.01f32)) * cast::<Ext, _>(self.coupon_rate)) * s(0.25f32);
                self.corp_bond_interest = self.corp_bond_interest + self.bond_coupon;
                self.maturity_year = to_long(val(mid(info.as_bytes(), 1, 4).as_bytes()));
                self.quarters_left = self.maturity_year.wrapping_sub(g.cur_year).wrapping_mul(4);
                if 0 < self.quarters_left {
                    self.quarters_left = self.quarters_left - (g.cur_quarter - 1);
                }
                if g.sim_co_count < h {
                    self.quarters_left += 1;
                }
                if self.quarters_left < 1 {
                    self.quarters_left = 1;
                }
                self.bond_discount = cast::<Ext, _>(g.hold_bond_face[(slot, e)]) - g.bond_hold_cost[(slot, e)];
                let mut skip_amort = 0;
                if self.bond_discount < Ext::from_f64(0.0) && 0 < instr(1, info.as_bytes(), b"cv") {
                    skip_amort = 1;
                }
                let mut convert_called = 0;
                if g.bond_price[h].get() > Ext::from_f64(100.0) && 0 < instr(1, info.as_bytes(), b"cv") {
                    if self.maturity_year == g.cur_year {
                        convert_called = 1;
                    } else if self.maturity_year - 1 == g.cur_year && g.cur_quarter == 4 && h < g.sim_co_count {
                        let conv_threshold: Ext = val(parse_any(info.as_bytes(), b"|", 2).as_bytes());
                        if g.market_cap[h] > conv_threshold {
                            convert_called = 1;
                        }
                    }
                }
                if skip_amort == 0 {
                    self.bond_amort = self.bond_discount / cast::<Ext, _>(self.quarters_left);
                    self.net_bond_amort = self.net_bond_amort + self.bond_amort;
                }
                if self.maturity_year <= g.cur_year + 1 && convert_called != 1 {
                    if g.cur_year + 1 == self.maturity_year {
                        if 3 < g.cur_quarter && (g.cur_quarter != 4 || h <= g.sim_co_count) {
                            self.maturing_bonds = self.maturing_bonds + cast::<Ext, _>(g.hold_bond_face[(slot, e)]);
                        }
                    } else if g.cur_year == self.maturity_year {
                        self.maturing_bonds = self.maturing_bonds + cast::<Ext, _>(g.hold_bond_face[(slot, e)]);
                    }
                }
            }
            if self.maturing_bonds > Ext::from_f64(0.0) {
                self.maturing_bonds = self.fv(self.maturing_bonds);
            }
            self.corp_bond_interest = self.fv(self.corp_bond_interest);
            self.set_amt(__content.string(2336), self.corp_bond_interest);
            self.line_no += 1;
        }
    }

    fn net_amort(&mut self) {
    let __content = crate::content::current();
        if self.net_bond_amort != Ext::from_f64(0.0) {
            self.net_bond_amort = self.fv(self.net_bond_amort);
        }
        if self.net_bond_amort > Ext::from_f64(0.0) {
            self.bond_discount_amort = self.net_bond_amort;
            self.set_amt(__content.string(2337), self.bond_discount_amort);
            self.line_no += 1;
        } else if self.net_bond_amort < Ext::from_f64(0.0) {
            self.bond_premium_amort = self.net_bond_amort;
        }
    }

    fn dividends(&mut self, g: &Globals, e: i32) {
    let __content = crate::content::current();
        if g.hold_stock_id[(0, e)] > 0 {
            for slot in 1..=15 {
                let h = cast::<i32, _>(g.hold_stock_id[(slot, e)]);
                if h == 0 {
                    continue;
                }
                // FCOM / JE also skips unordered operands. A damaged legacy dividend must not
                // turn the entire income/expense projection into NaN (and then formatted zero).
                if g.industry_num[h] < 71 && (g.dividend_per_share[h] == Ext::from_f64(0.0) || g.dividend_per_share[h].is_nan()) {
                    continue;
                }
                let pct = g.hold_stock_pct[(slot, e)];
                if g.industry_num[h] == 71 {
                    self.etf_dist_yield = val(parse_any(g.ent_misc_data[h].as_bytes(), b"|", 12).as_bytes());
                    self.holding_dividend = cast::<Ext, _>(pct) * (self.etf_dist_yield * s(0.01f32));
                } else {
                    self.holding_dividend = (g.shares_outstanding[h] * (g.dividend_per_share[h] * cast::<Ext, _>(pct))) * s(0.25f32);
                }
                self.gross_dividends = self.gross_dividends + self.holding_dividend;
                if pct > 79 {
                    self.div_received_ded = self.div_received_ded + self.holding_dividend;
                } else if pct > 19 {
                    self.div_received_ded = self.div_received_ded + self.holding_dividend * s(0.65f32);
                } else {
                    self.div_received_ded = self.div_received_ded + self.holding_dividend * s(0.5f32);
                }
            }
            if g.industry_num[e] == 71 {
                self.div_received_ded = Ext::from_f64(0.0);
            }
            if self.gross_dividends > Ext::from_f64(0.0) {
                self.gross_dividends = self.fv(self.gross_dividends);
                self.set_amt(__content.string(2338), self.gross_dividends);
                self.line_no += 1;
            }
        }
    }

    fn swaps(&mut self, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, e: i32) {
    let __content = crate::content::current();
        self.swap_pl = to_quad(process_swap_payments(g, ui, rng, e, &st("R")));
        if self.swap_pl != 0 {
            self.fmt_text = fmt(cast::<Ext, _>(self.swap_pl), &self.num_fmt);
            self.swap_pl = to_quad(val(self.fmt_text.as_bytes()));
            self.set_amt(__content.string(2339), cast::<Ext, _>(self.swap_pl));
            self.line_no += 1;
        }
    }

    /// Taxable income of the 80 %-owned subsidiaries (consolidated return) and the tax reimbursements.
    fn subs(&mut self, g: &Globals, e: i32) {
    let __content = crate::content::current();
        if g.hold_stock_id[(0, e)] > 0 {
            for slot in 1..=15 {
                let sub_id = cast::<i32, _>(g.hold_stock_id[(slot, e)]);
                self.sub_id = sub_id;
                if sub_id < 11 || g.hold_stock_pct[(slot, e)] < 80 {
                    continue;
                }
                if g.hold_stock_id[(0, e)] == 1 && g.consol_tax_inc_ytd[e] != Ext::from_f64(0.0) {
                    break;
                }
                let sim = g.sim_co_count;
                if (e < sim || sub_id <= e) && (e < sim || sim < sub_id) && (sim <= e || sim < sub_id || sub_id <= e) {
                    let mut w = g.proj_qtr_earnings[sub_id] * (Ext::from_f64(100.0) / cast::<Ext, _>(100 - g.corp_tax_rate));
                    if (g.accrued_pl[sub_id] + g.consol_tax_inc_ytd[sub_id]).abs() > w.abs() {
                        w = g.accrued_pl[sub_id] + g.consol_tax_inc_ytd[sub_id];
                    }
                    w = w + cast::<Ext, _>(g.extraordinary_pl[sub_id]);
                    if w > Ext::from_f64(0.0) && g.tax_loss_carry[sub_id] < Ext::from_f64(0.0) {
                        w = w + g.tax_loss_carry[sub_id];
                        if w < Ext::from_f64(0.0) {
                            w = Ext::from_f64(0.0);
                        }
                    }
                    self.work_amt = w;
                    self.subs_taxable_income = self.subs_taxable_income + w;
                    if w < Ext::from_f64(0.0) {
                        self.subs_loss_reimb_out = self.subs_loss_reimb_out + (w * cast::<Ext, _>(g.corp_tax_rate)) * s(0.01f32);
                    } else if w > Ext::from_f64(0.0) {
                        self.subs_tax_reimb_in = self.subs_tax_reimb_in + (w * cast::<Ext, _>(g.corp_tax_rate)) * s(0.01f32);
                    }
                }
            }
            self.subs_taxable_income = self.subs_taxable_income + g.consol_tax_inc_ytd[e];
        }
        if self.subs_taxable_income != Ext::from_f64(0.0) {
            self.subs_taxable_income = self.fv(self.subs_taxable_income);
            self.set_amt(__content.string(2340), self.subs_taxable_income);
            if self.subs_taxable_income < Ext::from_f64(0.0) {
                self.replace_line(__content.string(2341), __content.string(2342));
            }
            self.line_no += 1;
        }
    }

    fn extraordinary(&mut self, g: &Globals, e: i32) {
    let __content = crate::content::current();
        if g.extraordinary_pl[e] != 0 && g.industry_num[e] < 71 {
            self.extraordinary_gain = cast::<Ext, _>(g.extraordinary_pl[e]);
            self.extraordinary_gain = self.fv(self.extraordinary_gain);
            self.set_amt(__content.string(2343), self.extraordinary_gain);
            if self.extraordinary_gain < Ext::from_f64(0.0) {
                self.replace_line("Gain", "Loss");
            }
            self.line_no += 1;
        }
    }

    fn advances(&mut self, g: &Globals, e: i32) {
    let __content = crate::content::current();
        if g.tmp_player_advances > Ext::from_f64(0.0) {
            self.work_amt = (g.tmp_player_advances * s(0.0025f32)) * cast::<Ext, _>(g.prime_rate);
            self.advances_interest = self.fv(self.work_amt);
            self.set_amt(__content.string(2344), self.advances_interest);
            if g.credit_rating_num[e] < 2 {
                self.advances_interest = Ext::from_f64(0.0);
                self.set_amt(__content.string(2345), self.advances_interest);
            }
            self.line_no += 1;
        }
    }

    fn premium_and_drd(&mut self) {
    let __content = crate::content::current();
        if self.bond_premium_amort < Ext::from_f64(0.0) {
            self.set_amt(__content.string(2346), self.bond_premium_amort * -Ext::from_f64(1.0));
            self.line_no += 1;
        }
        if self.div_received_ded > Ext::from_f64(0.0) {
            self.div_received_ded = self.fv(self.div_received_ded);
            self.set_amt(__content.string(2347), self.div_received_ded);
            self.line_no += 1;
        }
    }

    fn ceo(&mut self, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, e: i32) {
    let __content = crate::content::current();
        let loop_limit = g.num_players;
        let mut slot = 1;
        while slot <= loop_limit {
            if e == g.player_ceo_of[slot] {
                let pay = calc_ceo_pay(g, ui, rng, e, 0);
                self.work_amt = Ext::from_f64(0.0);
                if g.cur_quarter == 4 && e < g.sim_co_count && g.ceo_bonus_multiple > Ext::from_f64(0.0) {
                    self.work_amt = g.ceo_bonus_multiple * pay;
                }
                self.ceo_pay = pay * s(0.25f32);
                self.ceo_pay = self.ceo_pay + self.work_amt;
                self.ceo_pay = self.fv(self.ceo_pay);
                self.set_amt(__content.string(2348), self.ceo_pay);
                self.sub_count = 0;
                if g.cur_quarter == 4 && e < g.sim_co_count {
                    self.set_amt(__content.string(2349), self.ceo_pay);
                }
                self.line_no += 1;
            }
            slot += 1;
        }
        self.slot = slot;
    }

    /// NET TAXABLE INCOME line through the tentative tax, loss carry-forward, tax credits, refunds and
    /// reimbursements, the adjusted tax and the capital taxes (`totalTaxes`).
    fn taxes(&mut self, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, e: i32) {
    let __content = crate::content::current();
        self.set_amt(__content.string(2350), self.net_taxable_income);
        self.line_no += 1;
        self.double();
        self.line_no += 2;
        let ctr = g.corp_tax_rate;
        self.tentative_tax = self.net_taxable_income * (cast::<Ext, _>(ctr) * s(0.01f32));
        if self.tentative_tax < Ext::from_f64(0.0) {
            self.tentative_tax = Ext::from_f64(0.0);
        }
        self.tentative_tax = self.fv(self.tentative_tax);
        let l = st(__content.string(2351)) + &str_l(ctr) + "%:        " + &fmt(self.tentative_tax, &self.num_fmt);
        self.set(self.line_no, l);
        self.line_no += 1;
        self.income_after_nol = self.net_taxable_income;
        self.adjusted_tax = self.tentative_tax;
        if g.tax_loss_carry[e] < Ext::from_f64(0.0) && self.income_after_nol > Ext::from_f64(0.0) {
            self.income_after_nol = self.income_after_nol + g.tax_loss_carry[e];
            if self.income_after_nol < Ext::from_f64(0.0) {
                self.tax_benefit = -Ext::from_f64(1.0) * self.adjusted_tax;
            } else {
                self.tax_benefit = g.tax_loss_carry[e] * (s(0.01f32) * cast::<Ext, _>(ctr));
            }
            self.set_amt(__content.string(2352), self.tax_benefit);
            self.line_no += 1;
            self.tax_benefit = self.fv(self.tax_benefit);
            self.adjusted_tax = self.adjusted_tax + self.tax_benefit;
        }
        update_corp_control(g, ui, rng, e);
        if self.tax_credits > Ext::from_f64(0.0) {
            if g.parent80_owner <= 10 && (self.adjusted_tax < self.tax_credits || self.adjusted_tax.is_nan()) {
                if self.adjusted_tax > Ext::from_f64(0.0) {
                    self.tax_credits = self.adjusted_tax;
                } else {
                    self.tax_credits = Ext::from_f64(0.0);
                }
            }
            if self.tax_credits > Ext::from_f64(0.0) {
                self.set_amt(__content.string(2353), self.tax_credits * -Ext::from_f64(1.0));
                self.line_no += 1;
                self.adjusted_tax = self.adjusted_tax - self.tax_credits;
            }
        }
        if g.tax_loss_carry[e] > Ext::from_f64(0.0) && self.income_after_nol < Ext::from_f64(0.0) {
            let skip = (g.cur_quarter == 4 && e <= g.sim_co_count) || (g.cur_quarter == 4 && e - g.sim_co_count < 1000);
            if !skip {
                self.work_amt = self.income_after_nol + g.tax_loss_carry[e];
                if self.work_amt < Ext::from_f64(0.0) {
                    self.tax_benefit = (g.tax_loss_carry[e] * (s(0.01f32) * cast::<Ext, _>(ctr))) * -Ext::from_f64(1.0);
                } else {
                    self.tax_benefit = self.income_after_nol * (s(0.01f32) * cast::<Ext, _>(ctr));
                }
                let l = st(__content.string(2354))
                    + &str_l(g.cur_year)
                    + __content.string(2355)
                    + &fmt(self.tax_benefit, &self.num_fmt);
                self.set(self.line_no, l);
                self.line_no += 1;
                self.tax_benefit = self.fv(self.tax_benefit);
                self.adjusted_tax = self.adjusted_tax + self.tax_benefit;
            }
        } else if 10 < g.parent80_owner && self.net_taxable_income < Ext::from_f64(0.0) {
            self.parent_tax_reimb = (self.net_taxable_income * s(0.01f32)) * cast::<Ext, _>(ctr);
            self.parent_tax_reimb = self.fv(self.parent_tax_reimb);
            self.set_amt(__content.string(2356), self.parent_tax_reimb);
            self.line_no += 1;
            self.adjusted_tax = self.adjusted_tax + self.parent_tax_reimb;
        }
        if self.subs_loss_reimb_out < Ext::from_f64(0.0) {
            self.subs_loss_reimb_out = self.fv(self.subs_loss_reimb_out);
            self.set_amt(__content.string(2357), self.subs_loss_reimb_out * -Ext::from_f64(1.0));
            self.line_no += 1;
            self.adjusted_tax = self.adjusted_tax - self.subs_loss_reimb_out;
        }
        if self.subs_tax_reimb_in > Ext::from_f64(0.0) {
            self.subs_tax_reimb_in = self.fv(self.subs_tax_reimb_in);
            self.set_amt(__content.string(2358), self.subs_tax_reimb_in * -Ext::from_f64(1.0));
            self.line_no += 1;
            self.adjusted_tax = self.adjusted_tax - self.subs_tax_reimb_in;
        }
        if self.tentative_tax == self.adjusted_tax {
            self.line_no += 1;
            self.slot = 0;
            self.set_amt(__content.string(2359), Ext::from_f64(0.0));
            self.line_no += 2;
        } else {
            self.rule();
            self.line_no += 1;
        }
        let l = st(__content.string(2360)) + __content.string(2361) + &fmt(self.adjusted_tax, &self.num_fmt);
        self.set(self.line_no, l);
        self.capital_taxes = calc_capital_taxes(g, e);
        if self.capital_taxes > Ext::from_f64(0.0) {
            self.capital_taxes = self.fv(self.capital_taxes);
            self.line_no += 2;
            self.set_amt(__content.string(2362), self.capital_taxes);
            self.line_no += 2;
            self.total_taxes = self.adjusted_tax + self.capital_taxes;
            self.set_amt(__content.string(2363), self.total_taxes);
        } else {
            self.total_taxes = self.adjusted_tax;
        }
    }

    fn after_tax(&mut self) {
    let __content = crate::content::current();
        self.line_no += 2;
        self.after_tax_cash_flow = self.net_taxable_income - self.total_taxes;
        self.after_tax_cash_flow = self.fv(self.after_tax_cash_flow);
        self.set_amt(__content.string(2364), self.after_tax_cash_flow);
        self.line_no += 2;
    }

    fn adjust_subs(&mut self) {
    let __content = crate::content::current();
        if self.subs_taxable_income != Ext::from_f64(0.0) {
            self.set_amt(__content.string(2365), self.subs_taxable_income * -Ext::from_f64(1.0));
            if self.subs_taxable_income < Ext::from_f64(0.0) {
                self.replace_line("Income", "Losses");
            }
            self.line_no += 1;
        }
    }

    fn adjust_extraordinary(&mut self) {
    let __content = crate::content::current();
        if self.extraordinary_gain != Ext::from_f64(0.0) {
            self.set_amt(__content.string(2366), self.extraordinary_gain * -Ext::from_f64(1.0));
            if self.extraordinary_gain < Ext::from_f64(0.0) {
                self.replace_line("Gain", "Loss");
            }
            self.line_no += 1;
        }
    }

    /// DRD, maturing bonds and bond amortization add-backs.
    fn adjust_drd_bonds(&mut self) {
    let __content = crate::content::current();
        if self.div_received_ded > Ext::from_f64(0.0) {
            self.set_amt(__content.string(2367), self.div_received_ded);
            self.line_no += 1;
        }
        if self.maturing_bonds > Ext::from_f64(0.0) {
            self.set_amt(__content.string(2368), self.maturing_bonds);
            self.line_no += 1;
        }
        if self.net_bond_amort != Ext::from_f64(0.0) {
            self.currency_label = fmt(self.bond_premium_amort * -Ext::from_f64(1.0), &self.num_fmt);
            self.fmt_text = fmt(self.bond_discount_amort * -Ext::from_f64(1.0), &self.num_fmt);
            self.set(self.line_no, st(__content.string(2369)));
            if self.bond_discount_amort > Ext::from_f64(0.0) {
                self.replace_line("Plus", "Less");
                self.replace_line(__content.string(2370), __content.string(2371));
                let l = self.get(self.line_no) + &self.fmt_text;
                self.set(self.line_no, l);
            } else {
                let l = self.get(self.line_no) + &self.currency_label;
                self.set(self.line_no, l);
            }
            self.line_no += 1;
        }
    }

    /// Bond issue maturing within the year (non-convertible or not worth converting): `bondsDue`.
    fn bonds_due(&mut self, g: &Globals, e: i32) {
        if g.bonds_issued[e] > 0 {
            self.maturity_year = to_long(val(mid(g.bond_issue_info[e].as_bytes(), 1, 4).as_bytes()));
        } else {
            self.maturity_year = 0;
        }
        let (cy, my) = (g.cur_year, self.maturity_year);
        if (cy + 1 == my || cy == my) && (cy == my || 3 < g.cur_quarter) && (cy == my || e <= g.sim_co_count) {
            if !(0 < instr(1, g.bond_issue_info[e].as_bytes(), b"cv") && g.bond_price[e].get() > Ext::from_f64(102.0)) {
                self.bonds_due = g.bonds_issued[e];
            }
        }
    }

    /// CURRENT CASH / PLUS-MINUS / ESTIMATED lines (LiquidFunds already set) and the initial status.
    fn cash_lines(&mut self, g: &Globals) {
    let __content = crate::content::current();
        self.set_amt(__content.string(2372), g.liquid_funds);
        self.line_no += 1;
        self.set_amt(__content.string(2373), self.net_cash_flow);
        self.line_no += 1;
        self.rule();
        self.line_no += 1;
        self.cash_in_3_months = self.net_cash_flow + g.liquid_funds;
        self.set_amt(__content.string(2374), self.cash_in_3_months);
        self.line_no += 1;
        self.double();
        self.line_no += 2;
        self.cash_status = if self.net_cash_flow > Ext::from_f64(0.0) { 1 } else { 2 };
    }

    /// Status 3..5 for a deficit that leaves the cash positive.
    fn deficit_status(&mut self, g: &Globals) {
        if self.cash_status == 2 {
            if self.net_cash_flow.abs() > g.liquid_funds * s(0.25f32) {
                self.cash_status = 3;
            }
            if self.net_cash_flow.abs() > g.liquid_funds * s(0.5f32) {
                if g.unused_credit_line > Ext::from_f64(0.0) {
                    self.cash_status = 4;
                } else {
                    self.cash_status = 5;
                }
            }
        }
    }

    /// The note on 80 %-owned subsidiaries at the end of the report.
    fn subs_note(&mut self, g: &Globals, e: i32) {
    let __content = crate::content::current();
        if g.hold_stock_id[(0, e)] > 0 {
            self.sub_count = 0;
            self.sub_id = 0;
            for slot in 1..=15 {
                if g.hold_stock_id[(slot, e)] != 0 && g.hold_stock_pct[(slot, e)] >= 80 {
                    self.sub_count += 1;
                    self.sub_id = cast::<i32, _>(g.hold_stock_id[(slot, e)]);
                }
            }
            self.fmt_text = PbStr::new();
            if 0 < self.sub_count && self.sub_count < 13 {
                self.fmt_text = number_to_words(self.sub_count, 0, 0);
            }
            if 12 < self.sub_count {
                self.fmt_text = str_l(self.sub_count);
            }
            if self.sub_count == 1 {
                self.currency_label = g.ent_name[self.sub_id].clone();
                if len(self.currency_label.as_bytes()) > 9 {
                    self.currency_label = g.ent_symbol[self.sub_id].clone();
                } else {
                    self.fmt_text = PbStr::new();
                }
            } else {
                self.currency_label = PbStr::new();
            }
            if 0 < self.sub_count {
                self.line_no += 2;
                self.set(self.line_no, st(__content.string(2375)));
                self.line_no += 1;
                let l = st(__content.string(2376)) + &g.ent_symbol[e] + __content.string(2377) + &self.fmt_text + __content.string(2378);
                self.set(self.line_no, l);
                if self.sub_count == 1 {
                    self.replace_line("diaries", "diary");
                    let l = self.get(self.line_no) + " " + &self.currency_label + ",";
                    self.set(self.line_no, l);
                }
                self.line_no += 1;
                let l = st(__content.string(2379)) + &g.ent_symbol[e] + __content.string(2380);
                self.set(self.line_no, l);
                if self.sub_count == 1 {
                    let l = replace(self.get(self.line_no).as_bytes(), __content.bytes(2381), self.currency_label.as_bytes());
                    self.set(self.line_no, l);
                }
            }
        }
    }

    /// `@END`, copy to `TextLines`, title, ShowTextReport.
    fn show(&mut self, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, e: i32) {
    let __content = crate::content::current();
        if !self.get(self.line_no + 2).is_empty() {
            self.line_no += 2;
        }
        self.set(self.line_no + 2, st("@END"));
        self.line_no += 2;
        let loop_limit = self.line_no + 3;
        for slot in 1..=loop_limit {
            g.text_lines[slot] = self.get(slot);
            if self.get(slot) == "@END" {
                break;
            }
        }
        g.text_box_title = st("  === ") + &g.ent_name[e] + __content.string(2382);
        g.text_has_links = 1;
        show_text_report(g, ui, rng);
    }
}

// ---------------------------------------------------------------------------------------------------------
// ShowCashFlowProjection
// ---------------------------------------------------------------------------------------------------------

/// Port of ShowCashFlowProjection (0x6a20e0).
/// 3-month cash-flow projection of entity `entity_ref`: players go to [`player_cash_flow_projection`]
/// (always for the current player), banks 11..85 to [`bank_cash_flow_projection`]; insurers, operating
/// corporations and funds are projected here. See the module docs for `mode` and the cash status
/// returned (0 when the report is shown or the entity is not active). Reads `TmpExt1` (rounded) as the
/// AI's cash-comfort threshold for mode 2 and clears it.
///
/// Side effects besides the report: `BalanceCashTBills`, `CalcEntityFinancials` (mode < 2),
/// `ProcessSwapPayments(e, "R")`, `CalcCeoPay`, `UpdateCorpControl`, `CalcCapitalTaxes`,
/// `CalcBankLoanPrincipalDue` (may move T-bills to cash, set/post `NewsMsg`), `CalcIndustryOutlook`
/// (corps without misc data), `LiquidFunds`, `UnusedCreditLine` (+ loan principal for status 7 with a
/// credit line), `TmpExt1..3`; in mode 2 `GrowthRate` / `DividendPerShare` of computer-run corps.
pub fn show_cash_flow_projection(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, entity_ref: i32, mode: i32) -> Ext {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError(ERR, "CashFlowProj", STR$(entityId)) — error path not ported.
    let mut entity_id = entity_ref;
    let mut industry: i16 = 0;
    if 10 < entity_id {
        industry = g.industry_num[entity_id];
    }
    let ext_arg = to_long(g.tmp_ext1);
    g.tmp_ext1 = Ext::from_f64(0.0);
    let cur_player = g.cur_player;
    if g.entity_status[entity_id] != 1 {
        return Ext::from_f64(0.0);
    }
    if entity_id < 1 || (g.num_players < entity_id && entity_id < 11) {
        return Ext::from_f64(0.0);
    }
    if 0 < entity_id && entity_id <= g.num_players {
        if cur_player != entity_ref {
            entity_id = g.cur_player;
        }
        return cast::<Ext, _>(player_cash_flow_projection(g, ui, rng, entity_id, mode));
    }
    if 10 < entity_id && entity_id < 86 {
        return cast::<Ext, _>(bank_cash_flow_projection(g, ui, rng, entity_ref, mode));
    }
    let e = entity_id;
    let mut cf = Cf::new();
    cf.formats(g, entity_ref);
    balance_cash_t_bills(g, e);
    if mode < 2 {
        calc_entity_financials(g, ui, rng, e);
    }
    cf.header(g, e);

    let mut operating_profit = Ext::from_f64(0.0);
    let mut ins_reserve_change = Ext::from_f64(0.0);
    let mut capex = Ext::from_f64(0.0);
    let mut working_cap_change = Ext::from_f64(0.0);
    if industry <= 69 {
        if e < 131 {
            // insurer
            cf.quarters_left = cast::<i32, _>(g.growth_rate[e]);
            cf.work_amt = -Ext::from_f64(1.0) * (s(0.7f32) * cast::<Ext, _>(cf.quarters_left));
            let d = (Ext::from_f64(1.0) + cast::<Ext, _>(g.disc_spend_pct[e])) + cast::<Ext, _>(g.difficulty_level);
            cf.work_amt = (cast::<Ext, _>(g.profit_margin[e]) + cf.work_amt) - d;
            operating_profit = (g.ins_reserves[e] * (s(0.01f32) * cf.work_amt)) * s(0.25f32);
            if g.insur_uw_profit_ytd[e] < Ext::from_f64(0.0) {
                if operating_profit > Ext::from_f64(0.0) || !(operating_profit.abs() >= g.insur_uw_profit_ytd[e].abs() * s(0.5f32)) {
                    operating_profit = g.insur_uw_profit_ytd[e] + operating_profit;
                }
            }
            operating_profit = cf.fv(operating_profit);
            cf.set_amt(__content.string(2383), operating_profit);
            cf.line_no += 1;
            cf.subprime(g, rng, e);
            if g.growth_rate[e] != 0 {
                ins_reserve_change = (cast::<Ext, _>(g.growth_rate[e]) * g.ins_reserves[e]) * s(0.0025f32);
                ins_reserve_change = cf.fv(ins_reserve_change);
            }
        } else {
            if len(g.ent_misc_data[e].as_bytes()) > 20 {
                let misc = g.ent_misc_data[e].clone();
                cf.currency_label = parse_any(misc.as_bytes(), b"|", 30);
                cf.work_amt = val(cf.currency_label.as_bytes());
                cf.quarters_left = to_long(val(parse_any(misc.as_bytes(), b"|", 29).as_bytes()));
                let disc_spend_pct = cast::<i32, _>(g.disc_spend_pct[e]);
                if (cast::<Ext, _>(cf.quarters_left) - cast::<Ext, _>(disc_spend_pct)).abs() > Ext::from_f64(4.0) {
                    let co_offset = e - g.sim_co_count;
                    let blend_weight: Ext = (cast::<Ext, _>(co_offset) / Ext::from_f64(1600.0)).abs();
                    cf.work_amt = cast::<Ext, _>(cf.quarters_left) + cf.work_amt;
                    let blended_disc_spend: Ext = if co_offset > 0 {
                        cast::<Ext, _>(disc_spend_pct) * (Ext::from_f64(1.0) - blend_weight) + cast::<Ext, _>(cf.quarters_left) * blend_weight
                    } else {
                        cast::<Ext, _>(cf.quarters_left) * (Ext::from_f64(1.0) - blend_weight) + cast::<Ext, _>(disc_spend_pct) * blend_weight
                    };
                    cf.work_amt = cf.work_amt - blended_disc_spend;
                }
            } else {
                calc_industry_outlook(g, rng, g.industry_num[e]);
                cf.work_amt = (g.tmp_ext1 + g.tmp_ext2) / Ext::from_f64(2.0);
                if cf.work_amt > Ext::from_f64(50.0) {
                    cf.work_amt = Ext::from_f64(50.0);
                } else if !(cf.work_amt >= -Ext::from_f64(20.0)) {
                    cf.work_amt = -Ext::from_f64(20.0);
                }
                cf.work_amt = cast::<Ext, _>(g.profit_margin[e]) + cf.work_amt;
                cf.work_amt = cf.work_amt - cast::<Ext, _>(g.disc_spend_pct[e]);
                g.tmp_ext1 = Ext::from_f64(0.0);
                g.tmp_ext2 = Ext::from_f64(0.0);
                g.tmp_ext3 = Ext::from_f64(0.0);
                cf.quarters_left = cast::<i32, _>(g.disc_spend_pct[e]);
            }
            operating_profit = g.business_assets[e] * ((cf.work_amt * s(0.01f32)) * s(0.25f32));
            operating_profit = cf.fv(operating_profit);
            if operating_profit.abs() > s(0.01f32) {
                cf.set_amt(__content.string(2384), operating_profit);
                cf.line_no += 1;
            } else {
                operating_profit = Ext::from_f64(0.0);
            }
            if cf.quarters_left > 10 && get_industry_category(industry) == 1 {
                cf.quarters_left -= 10;
                if cf.quarters_left > 0 {
                    cf.tax_credits = (g.business_assets[e] * (s(0.01f32) * cast::<Ext, _>(cf.quarters_left))) * s(0.2f32);
                    cf.tax_credits = cf.tax_credits * s(0.25f32);
                }
            }
            capex = (g.business_assets[e] * cast::<Ext, _>(g.growth_rate[e])) * s(0.0025f32);
            capex = cf.fv(capex);
            if capex > Ext::from_f64(0.0) {
                cf.tax_credits = cf.tax_credits + capex * s(0.07f32);
            }
            cf.tax_credits = cf.tax_credits + g.tax_credits[e];
            cf.tax_credits = cf.fv(cf.tax_credits);
            let ratio = calc_working_capital_ratio(g, e);
            let target_working_cap = ratio * (capex + g.business_assets[e]);
            working_cap_change = g.working_capital[e] - target_working_cap;
            if g.credit_rating_num[e] <= 7 {
                working_cap_change = working_cap_change * s(0.61f32);
            } else {
                working_cap_change = working_cap_change * s(0.95f32);
            }
            working_cap_change = cf.fv(working_cap_change);
        }
    }
    cf.subs(g, e);
    let mut adviser_fees = Ext::from_f64(0.0);
    if g.industry_num[e] == 37 || g.industry_num[e] == 2 {
        adviser_fees = calc_adviser_fee_income(g, e);
        adviser_fees = cf.fv(adviser_fees);
        if adviser_fees > Ext::from_f64(0.0) {
            cf.set_amt(__content.string(2385), adviser_fees);
            cf.line_no += 1;
        }
    }
    cf.tbills(g, e);
    cf.gov_bonds(g, e);
    cf.corp_bonds(g, e);
    cf.net_amort();
    cf.dividends(g, e);
    cf.swaps(g, ui, rng, e);
    cf.extraordinary(g, e);
    cf.rule();
    cf.line_no += 1;
    cf.total_income = (((((((((cf.subprime_interest + operating_profit) + cf.subs_taxable_income) + adviser_fees)
        + cf.tbill_interest)
        + cf.gov_bond_interest)
        + cf.corp_bond_interest)
        + cf.gross_dividends)
        + cast::<Ext, _>(cf.swap_pl))
        + cf.extraordinary_gain)
        + cf.bond_discount_amort;
    cf.total_income = cf.fv(cf.total_income);
    cf.set_amt(__content.string(2386), cf.total_income);
    if g.industry_num[e] == 71 {
        cf.set_amt(__content.string(2387), cf.total_income);
    }
    cf.line_no += 1;
    cf.rule();
    cf.line_no += 2;
    cf.set(cf.line_no, st(__content.string(2388)));
    cf.line_no += 1;

    let mut bank_loan_interest = Ext::from_f64(0.0);
    if g.bank_debt[e] > Ext::from_f64(0.0) {
        bank_loan_interest = (g.bank_debt[e] * (s(0.01f32) * cast::<Ext, _>(g.borrow_rate))) * s(0.25f32);
        bank_loan_interest = cf.fv(bank_loan_interest);
        cf.set_amt(__content.string(2389), bank_loan_interest);
        cf.line_no += 1;
    }
    let mut bond_interest_exp = Ext::from_f64(0.0);
    if g.bonds_issued[e] > 0 {
        cf.coupon_rate = cast::<f32, _>(val(right(g.bond_issue_info[e].as_bytes(), 5).as_bytes()));
        cf.work_amt = ((cast::<Ext, _>(g.bonds_issued[e]) * s(0.01f32)) * cast::<Ext, _>(cf.coupon_rate)) * s(0.25f32);
        bond_interest_exp = cf.fv(cf.work_amt);
        cf.set_amt(__content.string(2390), bond_interest_exp);
        if g.credit_rating_num[e] == 0 {
            bond_interest_exp = Ext::from_f64(0.0);
        }
        if g.credit_rating_num[e] == 1 {
            if g.bank_debt[e] > Ext::from_f64(0.0) || industry == 2 {
                bond_interest_exp = Ext::from_f64(0.0);
            }
            if bond_interest_exp > g.t_bills[e] + g.cash[e] {
                bond_interest_exp = Ext::from_f64(0.0);
            }
        }
        if bond_interest_exp == Ext::from_f64(0.0) {
            cf.set_amt(__content.string(2391), bond_interest_exp);
        }
        cf.line_no += 1;
    }
    cf.advances(g, e);
    let mut interest_expense = (bond_interest_exp + bank_loan_interest) + cf.advances_interest;
    if interest_expense == Ext::from_f64(0.0) {
        interest_expense = cf.fv(interest_expense);
        cf.set_amt(__content.string(2392), interest_expense);
        cf.line_no += 1;
    }
    cf.premium_and_drd();
    let mut goodwill_amort = Ext::from_f64(0.0);
    if 130 < e && g.goodwill[e] > Ext::from_f64(1.0) && g.industry_num[e] < 71 {
        cf.work_amt = goodwill_write_down(g, ui, rng, e, 1);
        if cf.work_amt > s(0.5f32) {
            goodwill_amort = cf.fv(cf.work_amt);
            cf.set_amt(__content.string(2393), goodwill_amort);
            cf.line_no += 1;
        }
    }
    cf.ceo(g, ui, rng, e);
    let mut etf_mgmt_fee = Ext::from_f64(0.0);
    if g.industry_num[e] == 71 {
        etf_mgmt_fee = g.total_assets * (s(0.0025f32) * cast::<Ext, _>(g.fund_fee_pct[e]));
        etf_mgmt_fee = cf.fv(etf_mgmt_fee);
        cf.set_amt(__content.string(2394), etf_mgmt_fee);
        cf.line_no += 1;
    }
    let mut storage_fees: Ext = crate::pb::numeric::double(calc_phys_commod_storage_cost(g, e) * s(0.25f32)); // DOUBLE local
    if !(s(0.005f32) >= storage_fees) {
        storage_fees = crate::pb::numeric::double(cf.fv(storage_fees));
        if storage_fees != Ext::from_f64(0.0) {
            cf.set_amt(__content.string(2395), storage_fees);
            cf.line_no += 1;
        }
    } else {
        storage_fees = crate::pb::numeric::double(Ext::from_f64(0.0));
    }
    cf.rule();
    cf.line_no += 1;
    cf.total_expenses = (((((cf.div_received_ded + interest_expense) + goodwill_amort) + cf.ceo_pay)
        + -Ext::from_f64(1.0) * cf.bond_premium_amort)
        + etf_mgmt_fee)
        + storage_fees;
    cf.total_expenses = cf.fv(cf.total_expenses);
    cf.set_amt(__content.string(2396), cf.total_expenses);
    cf.line_no += 1;
    cf.rule();
    cf.line_no += 1;
    cf.net_taxable_income = cf.total_income - cf.total_expenses;
    cf.net_taxable_income = cf.fv(cf.net_taxable_income);
    if g.industry_num[e] == 71 {
        cf.set_amt(__content.string(2397), cf.net_taxable_income);
        cf.line_no += 2;
        cf.set(cf.line_no, st(__content.string(2398)));
    } else {
        cf.taxes(g, ui, rng, e);
    }
    cf.after_tax();
    cf.adjust_subs();
    if capex != Ext::from_f64(0.0) {
        capex = cf.fv(capex);
    }
    if capex > Ext::from_f64(0.0) {
        cf.set_amt(__content.string(2399), capex * -Ext::from_f64(1.0));
        cf.line_no += 1;
    } else if capex < Ext::from_f64(0.0) {
        cf.set_amt(__content.string(2400), capex * -Ext::from_f64(1.0));
        cf.line_no += 1;
    }
    capex = -Ext::from_f64(1.0) * capex;
    if working_cap_change != Ext::from_f64(0.0) {
        working_cap_change = cf.fv(working_cap_change);
        cf.set_amt(__content.string(2401), working_cap_change);
        cf.line_no += 1;
    }
    if goodwill_amort > Ext::from_f64(0.0) {
        cf.set_amt(__content.string(2402), goodwill_amort);
        cf.line_no += 1;
    }
    if e < 131 && ins_reserve_change != Ext::from_f64(0.0) {
        cf.set_amt(__content.string(2403), ins_reserve_change);
        cf.line_no += 1;
    }
    cf.adjust_extraordinary();
    cf.adjust_drd_bonds();
    if g.dividend_per_share[e] > Ext::from_f64(0.0) || g.industry_num[e] == 71 {
        if g.industry_num[e] == 71 {
            if cf.net_taxable_income > Ext::from_f64(0.0) {
                cf.dividend_payout = cf.net_taxable_income * s(0.9f32);
                cf.dividend_payout = cf.fv(cf.dividend_payout);
                cf.set_amt(__content.string(2404), cf.dividend_payout * -Ext::from_f64(1.0));
            }
        } else {
            cf.dividend_payout = (g.shares_outstanding[e] * g.dividend_per_share[e]) * Ext::from_f64(25.0);
            cf.dividend_payout = cf.fv(cf.dividend_payout);
            cf.set_amt(__content.string(2405), cf.dividend_payout * -Ext::from_f64(1.0));
        }
        if cf.dividend_payout > Ext::from_f64(0.0) {
            cf.line_no += 1;
        }
    }
    if g.industry_num[e] == 71
        && g.extraordinary_pl[e] > 0
        && g.cur_quarter >= 3
        && !(g.cur_quarter == 3 && e > g.sim_co_count)
        && !(g.cur_quarter == 4 && e < g.sim_co_count)
    {
        cf.extraordinary_gain = cf.fv(cast::<Ext, _>(g.extraordinary_pl[e]));
        cf.set_amt(__content.string(2406), cf.extraordinary_gain * -Ext::from_f64(1.0));
        cf.line_no += 1;
    }
    cf.rule();
    cf.line_no += 1;
    cf.total_adjustments = ((((ins_reserve_change + capex) + cf.div_received_ded) + cf.maturing_bonds)
        + -Ext::from_f64(1.0) * cf.net_bond_amort)
        + goodwill_amort;
    cf.total_adjustments = (((-Ext::from_f64(1.0) * cf.dividend_payout + cf.total_adjustments) + -Ext::from_f64(1.0) * cf.subs_taxable_income)
        + -Ext::from_f64(1.0) * cf.extraordinary_gain)
        + working_cap_change;
    cf.set_amt(__content.string(2407), cf.total_adjustments);
    cf.line_no += 1;
    cf.rule();
    cf.line_no += 1;
    cf.proj_cash_flow = cf.after_tax_cash_flow + cf.total_adjustments;
    cf.proj_cash_flow = cf.fv(cf.proj_cash_flow);
    cf.set_amt(__content.string(2408), cf.proj_cash_flow);
    let mut loan_principal_due = calc_bank_loan_principal_due(g, ui, rng, e, -1);
    if loan_principal_due < Ext::from_f64(0.0) {
        loan_principal_due = Ext::from_f64(0.0);
    }
    cf.bonds_due(g, e);
    if cf.bonds_due > 0 || loan_principal_due > Ext::from_f64(0.0) {
        cf.set_amt(__content.string(2409), cf.proj_cash_flow);
        cf.line_no += 2;
        if loan_principal_due > Ext::from_f64(0.0) {
            loan_principal_due = cf.fv(loan_principal_due);
            cf.set_amt(__content.string(2410), loan_principal_due * -Ext::from_f64(1.0));
            cf.line_no += 1;
        }
        if cf.bonds_due > 0 {
            cf.set_amt(__content.string(2411), -Ext::from_f64(1.0) * cast::<Ext, _>(cf.bonds_due));
            cf.line_no += 1;
        }
        cf.rule();
        cf.line_no += 1;
        cf.net_cash_flow = cf.proj_cash_flow - (loan_principal_due + cast::<Ext, _>(cf.bonds_due));
        cf.set_amt(__content.string(2408), cf.net_cash_flow);
    } else {
        cf.bonds_due = 0;
        loan_principal_due = Ext::from_f64(0.0);
        cf.net_cash_flow = cf.proj_cash_flow;
    }
    cf.line_no += 1;
    cf.double();
    cf.line_no += 2;
    g.liquid_funds = g.t_bills[e] + g.cash[e];
    g.liquid_funds = cf.fv(g.liquid_funds);
    cf.cash_lines(g);
    if cf.cash_in_3_months < Ext::from_f64(0.0) {
        let l = st("     ") + &g.ent_symbol[e] + __content.string(2412);
        cf.set(cf.line_no, l);
        if g.unused_credit_line == Ext::from_f64(0.0) {
            let l = cf.get(cf.line_no) + __content.string(2413);
            cf.set(cf.line_no, l);
            cf.line_no += 1;
            if g.credit_rating_num[e] <= 1 {
                cf.set(cf.line_no, st(__content.string(2414)));
            } else {
                cf.set(cf.line_no, st(__content.string(2415)));
            }
            cf.cash_status = 8;
        } else {
            let l = cf.get(cf.line_no)
                + __content.string(2416)
                + &rtrim(ltrim(g.currency_symbol.as_bytes()).as_bytes())
                + &rtrim(ltrim(&format(g.unused_credit_line, b"* ###,")).as_bytes())
                + " "
                + &g.million_prefix_lc
                + "illion.";
            cf.set(cf.line_no, l);
            cf.cash_status = 6;
            if !((loan_principal_due + g.unused_credit_line) + cast::<Ext, _>(cf.bonds_due) >= cf.net_cash_flow.abs()) {
                cf.cash_status = 7;
            }
        }
        cf.line_no += 2;
        if g.unused_credit_line > Ext::from_f64(0.0) && cf.cash_status == 7 {
            g.unused_credit_line = loan_principal_due + g.unused_credit_line;
            if g.unused_credit_line > cf.cash_in_3_months.abs() {
                cf.cash_status = 6;
            }
        }
    } else {
        cf.deficit_status(g);
    }
    if mode == 2 && g.industry_num[e] != 71 {
        ai_growth_and_dividends(g, rng, &cf, e, industry, ext_arg);
    }
    g.tmp_ext1 = Ext::from_f64(0.0);
    if mode == 1 && cf.proj_cash_flow > Ext::from_f64(0.0) && cf.cash_status > 1 {
        g.tmp_ext1 = -Ext::from_f64(99.0);
    }
    if 0 < mode {
        return cast::<Ext, _>(cf.cash_status);
    }
    cf.set(cf.line_no, st(__content.string(2417)));
    cf.line_no += 1;
    cf.set(cf.line_no, st(__content.string(2418)));
    cf.line_no += 1;
    cf.set(cf.line_no, st(__content.string(2419)));
    cf.line_no += 1;
    cf.set(cf.line_no, st(__content.string(2420)));
    cf.line_no += 1;
    cf.set(cf.line_no, st(__content.string(2421)));
    cf.line_no += 1;
    cf.set(cf.line_no, st(__content.string(2422)));
    cf.subs_note(g, e);
    cf.show(g, ui, rng, e);
    Ext::from_f64(0.0)
}

/// Mode 2 of ShowCashFlowProjection (0x6a77c4..0x6a7f9d): a computer-run corporation adjusts its
/// growth rate and dividend to the projected cash status.
fn ai_growth_and_dividends(g: &mut Globals, rng: &mut Rng, cf: &Cf, e: i32, industry: i16, ext_arg: i32) {
    let status = cf.cash_status;
    if 130 < e && g.industry_num[e] < 70 && g.monetary_stance <= 0 {
        let mut throttled = false;
        if get_controller_type(g, e) == 1 && get_managing_player(g, e) == 0 {
            let ctrl = g.corp_controller[e];
            if ctrl < 6 && g.growth_rate[e] >= g.player_growth_throttle[ctrl] {
                throttled = true;
            }
        }
        if !throttled {
            // SELECT CASE cashStatus (compared as EXT)
            match status {
                7 | 8 => {
                    let r = rng.rnd_range(3, 1);
                    if cast::<i32, _>(g.growth_rate[e]) > r {
                        let x = -Ext::from_f64(10.0) + Ext::from_f64(8.0) * rng.rnd();
                        g.growth_rate[e] = cast::<i16, _>(to_quad(x));
                    }
                }
                6 => {
                    let x = Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(20.0);
                    if x > cast::<Ext, _>(ext_arg) && g.growth_rate[e] > 5 {
                        g.growth_rate[e] = cast::<i16, _>(to_quad(s(0.4f32) * cast::<Ext, _>(g.growth_rate[e])));
                    }
                }
                5 => {
                    if ext_arg < 23 && g.growth_rate[e] > 3 {
                        g.growth_rate[e] = cast::<i16, _>(to_quad(s(0.5f32) * cast::<Ext, _>(g.growth_rate[e])));
                    }
                }
                4 => {
                    if ext_arg < 20 && g.growth_rate[e] > 6 {
                        g.growth_rate[e] = cast::<i16, _>(to_quad(s(0.6f32) * cast::<Ext, _>(g.growth_rate[e])));
                    }
                }
                3 => {
                    if ext_arg < 17 && g.growth_rate[e] > 8 {
                        g.growth_rate[e] = cast::<i16, _>(to_quad(s(0.8f32) * cast::<Ext, _>(g.growth_rate[e])));
                    }
                }
                1 | 2 => {
                    if g.monetary_stance == 0 {
                        let x = Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(15.0);
                        if !(x >= cast::<Ext, _>(ext_arg)) {
                            g.growth_rate[e] = g.growth_rate[e].wrapping_add(3);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let div_yield: Ext = calc_dividend_yield(g, e);
    if status > 6 {
        g.dividend_per_share[e] = Ext::from_f64(0.0);
    } else if status > 4 {
        if !(s(1.5f32) >= div_yield) {
            g.dividend_per_share[e] = g.dividend_per_share[e] * s(0.5f32);
        }
    } else if status > 2 {
        if !(s(2.8f32) >= div_yield) {
            g.dividend_per_share[e] = g.dividend_per_share[e] * s(0.75f32);
        }
    } else if status < 3 && g.monetary_stance > 0 && get_controller_type(g, e) != 1 {
        if industry == 69 {
            let x = Ext::from_f64(4.0) * rng.rnd() + Ext::from_f64(4.0);
            if x > div_yield {
                if Ext::from_f64(3.0) > div_yield {
                    g.dividend_per_share[e] = (g.market_cap[e] / g.shares_outstanding[e]) * s(0.03f32);
                } else {
                    g.dividend_per_share[e] = g.dividend_per_share[e] * s(1.2f32);
                }
                return;
            }
        }
        let x = Ext::from_f64(3.0) * rng.rnd() + Ext::from_f64(3.0);
        if x > div_yield {
            if Ext::from_f64(3.0) > div_yield {
                g.dividend_per_share[e] = (g.market_cap[e] / g.shares_outstanding[e]) * s(0.03f32);
            } else {
                let y = Ext::from_f64(6.0) * rng.rnd() + Ext::from_f64(1.0);
                if y > div_yield {
                    g.dividend_per_share[e] = g.dividend_per_share[e] * s(1.15f32);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------
// BankCashFlowProjection
// ---------------------------------------------------------------------------------------------------------

/// Port of BankCashFlowProjection (0x6a99f7).
/// The bank (11..85) variant of the 3-month cash-flow projection: loan interest from every borrower
/// (whose financials are recomputed), bad-debt reserve changes, consumer/mortgage/subprime interest
/// and repayments, deposit cost and growth, expected corporate loan repayments
/// ([`calc_bank_loan_principal_due`] for borrowers without a reported figure or with frozen credit) and
/// the reinvestment of surplus cash under the bank's asset allocation. Returns the cash status in modes
/// 1 and 2 (see the module docs; mode 2 has no AI part here), 0 otherwise. Entities < 11 raise error 9
/// (runtime-error box in the original, not modelled) and return 0; a bank without misc data (no quarter
/// reported yet) gets a pop-up and 0.
/// Sets `UnusedCreditLine = 2000 - BankDebt`, `LiquidFunds`, `BankLargestLoan`, `TmpExt1`.
pub fn bank_cash_flow_projection(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, entity_ref: i32, mode: i32) -> i32 {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError, then exit.
    if entity_ref < 11 || 1600 < entity_ref {
        // MarketCap(entityRef) (11..1600) is read first: error 9 -> ShowRuntimeError, result 0. So the
        // MsgCorpOnlyFunction branch below is never reached (BUGS.md).
        return 0;
    }
    let mut cf = Cf::new();
    cf.formats(g, entity_ref);
    let bank_id = entity_ref;
    let b = bank_id;
    g.tmp_ext1 = Ext::from_f64(0.0);
    if g.entity_status[b] != 1 {
        return 0;
    }
    if b < 11 {
        if mode == 0 {
            msg_corp_only_function(g, ui);
        }
        return 0;
    }
    balance_cash_t_bills(g, b);
    if mode < 2 {
        calc_entity_financials(g, ui, rng, b);
    }
    if len(g.ent_misc_data[b].as_bytes()) <= 20 {
        let mut msg = st(__content.string(2423)) + &g.ent_name[b] + __content.string(2424);
        show_quick_message(g, ui, &mut msg);
        return 0;
    }
    cf.header(g, b);
    calc_entity_financials(g, ui, rng, b);
    let mut reserve_change = Ext::from_f64(0.0);
    if g.bad_debt_reserve[b] < Ext::from_f64(0.0) {
        reserve_change = -Ext::from_f64(1.0) * g.bad_debt_reserve[b];
    }
    let mut bad_loans: i64 = 0;
    let mut weak_loans: i64 = 0;
    let mut scratch_amt;
    let mut loan_interest_income = Ext::from_f64(0.0);
    let mut net_loan_repay = Ext::from_f64(0.0);
    let mut loan_interest;
    for idx in 1..=1600 {
        if g.entity_status[idx] == 1 && b != idx && cast::<i32, _>(g.lending_bank[idx]) == b && g.bank_debt[idx] > Ext::from_f64(0.0) {
            calc_entity_financials(g, ui, rng, idx);
            if g.credit_rating_num[idx] < 2 {
                bad_loans = to_quad(cast::<Ext, _>(bad_loans) + g.bank_debt[idx]);
            }
            if g.credit_rating_num[idx] == 2 {
                weak_loans = to_quad(cast::<Ext, _>(weak_loans) + g.bank_debt[idx]);
            }
            let mut rate: f32 = cast::<f32, _>(g.borrow_rate);
            if 10 < idx && idx < 86 {
                rate = g.entity_borrow_rate;
            }
            loan_interest = (g.bank_debt[idx] * cast::<Ext, _>(rate)) * s(0.0025f32);
            loan_interest_income = loan_interest_income + loan_interest;
            if idx < 86 {
                continue;
            }
            let mut borrower_repay = if len(g.ent_misc_data[idx].as_bytes()) > 20 {
                val(parse_any(g.ent_misc_data[idx].as_bytes(), b"|", 13).as_bytes())
            } else {
                Ext::from_f64(0.0)
            };
            if borrower_repay == Ext::from_f64(0.0) || g.credit_frozen[idx] == 1 {
                borrower_repay = calc_bank_loan_principal_due(g, ui, rng, idx, -1);
            }
            net_loan_repay = net_loan_repay + borrower_repay;
        }
    }
    if loan_interest_income > Ext::from_f64(0.0) {
        loan_interest_income = cf.fv(loan_interest_income);
        cf.set_amt(__content.string(2425), loan_interest_income);
        cf.line_no += 1;
    }
    calc_entity_financials(g, ui, rng, b);
    g.bank_largest_loan = g.bank_largest_loan - g.bank_loans_total * s(0.25f32);
    if g.bank_largest_loan < Ext::from_f64(0.0) {
        g.bank_largest_loan = Ext::from_f64(0.0);
    }
    let mut expected_losses = ((((cast::<Ext, _>(bad_loans) * s(0.2f32) + g.tmp_bank_total_loans * s(0.005f32))
        + cast::<Ext, _>(weak_loans) * s(0.03f32))
        + cast::<Ext, _>(g.bank_consumer_loans[b]) * s(0.06f32))
        + g.bank_largest_loan * s(0.1f32))
        + cast::<Ext, _>(g.subprime_secs[b]) * s(0.03f32);
    cf.sub_id = 1;
    if b <= g.sim_co_count {
        cf.sub_id = 2;
    }
    if b % 4 + cf.sub_id == g.cur_quarter {
        let gdp = cast::<Ext, _>(g.gdp_growth);
        scratch_amt = cast::<Ext, _>(g.bank_consumer_loans[b]) * (((Ext::from_f64(7.0) - gdp) + Ext::from_f64(5.0)) * s(0.01f32));
        loan_interest = cast::<Ext, _>(g.subprime_secs[b]) * (((Ext::from_f64(6.0) - gdp) * s(0.1f32)) * s(0.01f32)) + s(0.3f32);
        expected_losses = (expected_losses + scratch_amt) + loan_interest;
    }
    if g.subprime_crisis_state > 0 {
        expected_losses = expected_losses + cast::<Ext, _>(g.subprime_secs[b]) * s(0.05f32);
    }
    scratch_amt = expected_losses - g.bad_debt_reserve[b];
    reserve_change = reserve_change + scratch_amt * s(0.25f32);
    if g.rand_seed % 3 + 3 > g.cur_quarter && g.start_year == g.cur_year {
        reserve_change = reserve_change * s(0.25f32);
    }
    if g.bad_debt_reserve[b] > Ext::from_f64(0.0)
        && g.bad_debt_reserve[b] > expected_losses
        && !(g.bad_debt_reserve[b] >= expected_losses * s(1.5f32))
    {
        reserve_change = Ext::from_f64(0.0);
    }
    reserve_change = reserve_change + g.reserve_adj_pending[b];
    cf.subprime(g, rng, b);
    cf.tax_credits = g.tax_credits[b];
    let mut mortgage_interest = Ext::from_f64(0.0);
    if g.bank_mortgage_loans[b] > 0 {
        cf.work_amt = s(0.0025f32) * get_market_interest_rate(g, rng, 2);
        mortgage_interest = cast::<Ext, _>(g.bank_mortgage_loans[b]) * cf.work_amt + cf.work_amt;
        mortgage_interest = cf.fv(mortgage_interest);
        cf.set_amt(__content.string(2426), mortgage_interest);
        cf.line_no += 1;
    }
    let mut consumer_interest = Ext::from_f64(0.0);
    if g.bank_consumer_loans[b] > 0 {
        cf.work_amt = s(0.0025f32) * get_market_interest_rate(g, rng, 1);
        consumer_interest = cast::<Ext, _>(g.bank_consumer_loans[b]) * cf.work_amt;
        consumer_interest = cf.fv(consumer_interest);
        cf.set_amt(__content.string(2427), consumer_interest);
        cf.line_no += 1;
    }
    let mut reserve_reduction = Ext::from_f64(0.0);
    if reserve_change < Ext::from_f64(0.0) {
        reserve_reduction = cf.fv(reserve_change);
        cf.set_amt(__content.string(2428), reserve_reduction);
        cf.line_no += 1;
    }
    cf.tbills(g, b);
    cf.gov_bonds(g, b);
    cf.corp_bonds(g, b);
    cf.net_amort();
    cf.dividends(g, b);
    cf.swaps(g, ui, rng, b);
    cf.subs(g, b);
    cf.extraordinary(g, b);
    cf.rule();
    cf.line_no += 1;
    cf.total_income = (((((((cf.subs_taxable_income + cf.subprime_interest) + cf.tbill_interest)
        + cf.gov_bond_interest)
        + cf.corp_bond_interest)
        + cf.gross_dividends)
        + cast::<Ext, _>(cf.swap_pl))
        + cf.extraordinary_gain)
        + cf.bond_discount_amort;
    cf.total_income = (((loan_interest_income + cf.total_income) + mortgage_interest) + consumer_interest) + reserve_reduction;
    cf.total_income = cf.fv(cf.total_income);
    cf.set_amt(__content.string(2386), cf.total_income);
    if g.industry_num[b] == 71 {
        cf.set_amt(__content.string(2387), cf.total_income);
    }
    cf.line_no += 1;
    cf.rule();
    cf.line_no += 2;
    cf.set(cf.line_no, st(__content.string(2388)));
    cf.line_no += 1;
    let mut reserve_increase = Ext::from_f64(0.0);
    if reserve_change > Ext::from_f64(0.0) {
        reserve_increase = cf.fv(reserve_change);
        cf.set_amt(__content.string(2429), reserve_increase);
        cf.line_no += 1;
    }
    scratch_amt = calc_bank_deposit_cost(g, b);
    let mut overhead_exp = Ext::from_f64(0.0);
    if scratch_amt > Ext::from_f64(0.0) {
        overhead_exp = cf.fv(scratch_amt);
        cf.set_amt(__content.string(2430), overhead_exp);
        cf.line_no += 1;
    }
    scratch_amt = g.bank_c_ds[b] * (s(0.0025f32) * get_bank_cd_rate(g, b));
    let mut cd_interest = Ext::from_f64(0.0);
    if scratch_amt > Ext::from_f64(0.0) {
        cd_interest = cf.fv(scratch_amt);
        cf.set_amt(__content.string(2431), cd_interest);
        cf.line_no += 1;
    }
    let mut bond_interest_exp = Ext::from_f64(0.0);
    let mut bond_int_accrued = Ext::from_f64(0.0);
    if g.bonds_issued[b] > 0 {
        cf.coupon_rate = cast::<f32, _>(val(right(g.bond_issue_info[b].as_bytes(), 5).as_bytes()));
        cf.work_amt = ((cast::<Ext, _>(g.bonds_issued[b]) * s(0.01f32)) * cast::<Ext, _>(cf.coupon_rate)) * s(0.25f32);
        bond_interest_exp = cf.fv(cf.work_amt);
        cf.set_amt(__content.string(2390), bond_interest_exp);
        if g.credit_rating_num[b] == 0 {
            bond_interest_exp = Ext::from_f64(0.0);
        }
        if g.credit_rating_num[b] == 1 {
            if g.bank_debt[b] > Ext::from_f64(0.0) {
                bond_interest_exp = Ext::from_f64(0.0);
            }
            if bond_interest_exp > Ext::from_f64(0.0) && bond_interest_exp > g.t_bills[b] + g.cash[b] {
                bond_interest_exp = Ext::from_f64(0.0);
                bond_int_accrued = val(cf.fmt_text.as_bytes());
            }
        }
        if bond_interest_exp == Ext::from_f64(0.0) {
            cf.set_amt(__content.string(2432), bond_int_accrued);
        }
        cf.line_no += 1;
    }
    let mut interbank_interest = Ext::from_f64(0.0);
    if g.bank_debt[b] > Ext::from_f64(0.0) {
        interbank_interest = (g.bank_debt[b] * (s(0.01f32) * cast::<Ext, _>(g.borrow_rate))) * s(0.25f32);
        interbank_interest = cf.fv(interbank_interest);
        cf.set_amt(__content.string(2433), interbank_interest);
        cf.line_no += 1;
    }
    cf.advances(g, b);
    let mut interest_expense =
        (((bond_interest_exp + interbank_interest) + bond_int_accrued) + cf.advances_interest) + cd_interest;
    if interest_expense == Ext::from_f64(0.0) {
        interest_expense = cf.fv(interest_expense);
        cf.set_amt(__content.string(2392), interest_expense);
        cf.line_no += 1;
    }
    cf.premium_and_drd();
    cf.ceo(g, ui, rng, b);
    cf.rule();
    cf.line_no += 1;
    cf.total_expenses = ((((cf.div_received_ded + interest_expense) + cf.ceo_pay) + -Ext::from_f64(1.0) * cf.bond_premium_amort)
        + overhead_exp)
        + reserve_increase;
    cf.total_expenses = cf.fv(cf.total_expenses);
    cf.set_amt(__content.string(2396), cf.total_expenses);
    cf.line_no += 1;
    cf.rule();
    cf.line_no += 1;
    cf.net_taxable_income = cf.total_income - cf.total_expenses;
    cf.net_taxable_income = cf.fv(cf.net_taxable_income);
    cf.taxes(g, ui, rng, b);
    cf.after_tax();
    if reserve_change != Ext::from_f64(0.0) {
        reserve_change = cf.fv(reserve_change);
        if reserve_change > Ext::from_f64(0.0) {
            cf.set_amt(__content.string(2434), reserve_increase);
        } else {
            // the decrease line shows reserveIncrease (0) as well
            cf.set_amt(__content.string(2435), reserve_increase);
        }
        cf.line_no += 1;
    }
    if bond_int_accrued > Ext::from_f64(0.0) {
        cf.set(cf.line_no, st(__content.string(2436)));
        cf.line_no += 1;
    }
    cf.adjust_subs();
    cf.adjust_extraordinary();
    cf.adjust_drd_bonds();
    if g.dividend_per_share[b] > Ext::from_f64(0.0) {
        cf.dividend_payout = (g.shares_outstanding[b] * g.dividend_per_share[b]) * Ext::from_f64(25.0);
        cf.dividend_payout = cf.fv(cf.dividend_payout);
        cf.set_amt(__content.string(2405), cf.dividend_payout * -Ext::from_f64(1.0));
        if cf.dividend_payout > Ext::from_f64(0.0) {
            cf.line_no += 1;
        }
    }
    net_loan_repay = cf.fv(net_loan_repay);
    let star = net_loan_repay != Ext::from_f64(0.0) && g.credit_frozen[b] == 0 && g.bank_alloc_off[b] == 0;
    if net_loan_repay > Ext::from_f64(0.0) {
        cf.set_amt(__content.string(2437), net_loan_repay);
        if star {
            let l = cf.get(cf.line_no) + " *";
            cf.set(cf.line_no, l);
        }
        cf.line_no += 1;
    } else if net_loan_repay < Ext::from_f64(0.0) {
        cf.set_amt(__content.string(2438), net_loan_repay);
        if star {
            let l = cf.get(cf.line_no) + " *";
            cf.set(cf.line_no, l);
        }
        cf.line_no += 1;
    }
    let mut consumer_repay = cast::<Ext, _>(g.bank_consumer_loans[b]) * s(0.05f32);
    consumer_repay = cf.fv(consumer_repay);
    if consumer_repay > Ext::from_f64(0.0) {
        cf.set_amt(__content.string(2439), consumer_repay);
        cf.line_no += 1;
    }
    let mut mortgage_repay = cast::<Ext, _>(g.bank_mortgage_loans[b]) * s(0.02f32);
    mortgage_repay = cf.fv(mortgage_repay);
    if mortgage_repay > Ext::from_f64(0.0) {
        cf.set_amt(__content.string(2440), mortgage_repay);
        cf.line_no += 1;
    }
    let mut subprime_repay = cast::<Ext, _>(g.subprime_secs[b]) * s(0.01f32);
    subprime_repay = cf.fv(subprime_repay);
    if subprime_repay > Ext::from_f64(0.0) {
        cf.set_amt(__content.string(2441), subprime_repay);
        cf.line_no += 1;
    }
    let deposit_growth_pct = cast::<Ext, _>(g.growth_rate[b]) * s(0.0025f32);
    let mut deposit_change = (g.bank_demand_deposits[b] + g.bank_c_ds[b]) * deposit_growth_pct;
    deposit_change = cf.fv(deposit_change);
    if deposit_change != Ext::from_f64(0.0) {
        cf.set_amt(__content.string(2442), deposit_change);
        if deposit_change < Ext::from_f64(0.0) {
            cf.replace_line("Inc", "Dec");
        }
        cf.line_no += 1;
    }
    cf.bonds_due(g, b);
    cf.total_adjustments = ((((((cf.maturing_bonds + cf.div_received_ded) + -Ext::from_f64(1.0) * cf.net_bond_amort) + reserve_change)
        + net_loan_repay)
        + consumer_repay)
        + mortgage_repay)
        + subprime_repay;
    cf.total_adjustments = ((((-Ext::from_f64(1.0) * cf.dividend_payout + cf.total_adjustments) + -Ext::from_f64(1.0) * cf.subs_taxable_income)
        + -Ext::from_f64(1.0) * cf.extraordinary_gain)
        + deposit_change)
        + bond_int_accrued;
    g.liquid_funds = g.t_bills[b] + g.cash[b];
    g.liquid_funds = cf.fv(g.liquid_funds);
    if g.bank_alloc_off[b] == 0 {
        let mut pre_bond_cash_flow = to_quad(cf.after_tax_cash_flow + cf.total_adjustments);
        if cf.bonds_due > 0 {
            pre_bond_cash_flow = to_quad(cast::<Ext, _>(pre_bond_cash_flow) - cast::<Ext, _>(cf.bonds_due));
        }
        let pct = cast::<Ext, _>(g.bank_alloc_cash_pct[b]);
        let mut reinvest_amt = to_quad(cast::<Ext, _>(pre_bond_cash_flow) * (Ext::from_f64(1.0) - s(0.01f32) * pct));
        reinvest_amt = to_quad(cast::<Ext, _>(reinvest_amt) - cf.after_tax_cash_flow * s(0.5f32));
        scratch_amt = (((g.tmp_corp_bonds_value + g.tmp_gov_bonds_value) + cast::<Ext, _>(g.bank_consumer_loans[b]))
            + cast::<Ext, _>(g.bank_mortgage_loans[b]))
            + cast::<Ext, _>(g.subprime_secs[b])
            + g.liquid_funds;
        let target_cash = to_quad(scratch_amt * (s(0.01f32) * pct));
        cf.work_amt = g.liquid_funds - cast::<Ext, _>(target_cash);
        reinvest_amt = to_quad(cast::<Ext, _>(reinvest_amt) + cf.work_amt * s(0.3f32));
        cf.total_adjustments = cf.total_adjustments - cast::<Ext, _>(reinvest_amt);
        if reinvest_amt > 0 {
            cf.set_amt(__content.string(2443), -Ext::from_f64(1.0) * cast::<Ext, _>(reinvest_amt));
            cf.line_no += 1;
        } else if reinvest_amt < 0 {
            cf.set_amt(__content.string(2444), cast::<Ext, _>(reinvest_amt));
            cf.line_no += 1;
        }
    }
    cf.rule();
    cf.line_no += 1;
    cf.set_amt(__content.string(2407), cf.total_adjustments);
    cf.line_no += 1;
    cf.rule();
    cf.line_no += 1;
    cf.proj_cash_flow = cf.after_tax_cash_flow + cf.total_adjustments;
    cf.proj_cash_flow = cf.fv(cf.proj_cash_flow);
    cf.set_amt(__content.string(2408), cf.proj_cash_flow);
    if cf.bonds_due > 0 {
        cf.set_amt(__content.string(2409), cf.proj_cash_flow);
        cf.line_no += 2;
        cf.set_amt(__content.string(2411), -Ext::from_f64(1.0) * cast::<Ext, _>(cf.bonds_due));
        cf.line_no += 1;
        cf.rule();
        cf.line_no += 1;
        cf.net_cash_flow = cf.proj_cash_flow - cast::<Ext, _>(cf.bonds_due);
        cf.set_amt(__content.string(2408), cf.net_cash_flow);
    } else {
        cf.bonds_due = 0;
        cf.net_cash_flow = cf.proj_cash_flow;
    }
    cf.line_no += 1;
    cf.double();
    cf.line_no += 2;
    cf.cash_lines(g);
    g.unused_credit_line = Ext::from_f64(2000.0) - g.bank_debt[b];
    if cf.cash_in_3_months < Ext::from_f64(0.0) {
        let l = st("     ") + &g.ent_symbol[b] + __content.string(2412);
        cf.set(cf.line_no, l);
        if g.unused_credit_line == Ext::from_f64(0.0) {
            let l = cf.get(cf.line_no) + __content.string(2445);
            cf.set(cf.line_no, l);
            if b < 85 {
                let with = g.ent_name[b + 1].clone() + __content.string(2446);
                let l = replace(cf.get(cf.line_no).as_bytes(), __content.bytes(2447), with.as_bytes());
                cf.set(cf.line_no, l);
            }
            cf.line_no += 1;
            if g.credit_rating_num[b] <= 1 {
                cf.set(cf.line_no, st(__content.string(2414)));
            } else {
                cf.set(cf.line_no, st(__content.string(2415)));
            }
            cf.cash_status = 8;
        } else {
            let l = cf.get(cf.line_no)
                + __content.string(2448)
                + &rtrim(ltrim(g.currency_symbol.as_bytes()).as_bytes())
                + &rtrim(ltrim(&format(g.unused_credit_line, b"* ###,")).as_bytes())
                + " "
                + &g.million_prefix_lc
                + "illion.";
            cf.set(cf.line_no, l);
            cf.cash_status = 6;
            if !(g.unused_credit_line + cast::<Ext, _>(cf.bonds_due) >= cf.net_cash_flow.abs()) {
                cf.cash_status = 7;
            }
        }
        cf.line_no += 2;
    } else {
        cf.deficit_status(g);
    }
    // IF mode = 2 THEN (empty)
    g.tmp_ext1 = Ext::from_f64(0.0);
    if mode == 1 && cf.proj_cash_flow > Ext::from_f64(0.0) && cf.cash_status > 1 {
        g.tmp_ext1 = -Ext::from_f64(99.0);
    }
    if 0 < mode {
        return cf.cash_status;
    }
    cf.set(cf.line_no, st(__content.string(2449)));
    cf.line_no += 1;
    cf.set(cf.line_no, st(__content.string(2450)));
    cf.line_no += 1;
    cf.set(cf.line_no, st(__content.string(2451)));
    cf.line_no += 1;
    cf.set(cf.line_no, st(__content.string(2452)));
    cf.line_no += 1;
    cf.set(cf.line_no, st(__content.string(2453)));
    cf.subs_note(g, b);
    if net_loan_repay != Ext::from_f64(0.0) && g.credit_frozen[b] == 0 && g.bank_alloc_off[b] == 0 {
        cf.line_no += 2;
        cf.set(cf.line_no, st(__content.string(2454)));
        cf.line_no += 1;
        cf.set(cf.line_no, st(__content.string(2455)));
        cf.line_no += 1;
        cf.set(cf.line_no, st(__content.string(2456)));
    }
    cf.show(g, ui, rng, b);
    0
}

// ---------------------------------------------------------------------------------------------------------
// PlayerCashFlowProjection
// ---------------------------------------------------------------------------------------------------------

/// `RIGHT$(STRING$(12, " ") + RTRIM$(LTRIM$(FORMAT$(x, numFmt))), n)`.
fn amount_text(x: Ext, num_fmt: &PbStr, n: i32) -> PbStr {
    let t = string_fill(12, b" ") + &rtrim(ltrim(&format(x, num_fmt.as_bytes())).as_bytes());
    right(t.as_bytes(), n)
}

/// `VAL(REMOVE$(FORMAT$(x, numFmt), ANY ","))`.
fn fv_nc(x: Ext, num_fmt: &PbStr) -> Ext {
    let t = format(x, num_fmt.as_bytes());
    val(remove_any(&t, b",").as_bytes())
}

/// Port of PlayerCashFlowProjection (0x6afc4d).
/// Report of the current player's projected annual portfolio cash flow (T-bill, bond, dividend and
/// advance interest income, storage fees, interest expense, income and capital taxes, prepaid tax,
/// maturing bonds, living expenses). Always reports on `CurPlayer` (`entity_ref` and `mode` are not
/// used) and relies on the portfolio totals (`TmpGovBondsValue`, `TmpCorpBondsValue`,
/// `TmpSubsidStockValue`, `PhysCommodValue`, `NetWorth`, `TotalAssets`) of the last
/// CalcEntityFinancials. A liquidated player gets DescribeDefunctEntity. Returns 0. Side effects:
/// `TmpCapitalTax`, `CorpSharesTaxDue` (annualized), `TmpExt1 = 0`.
pub fn player_cash_flow_projection(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, _entity_ref: i32, _mode: i32) -> i32 {
    let __content = crate::content::current();
    let mut lines = vec![PbStr::new(); 1501];
    let rule_line = string_fill(70, b"-");
    let mut num_fmt = st("* #################,");
    if g.total_assets < Ext::from_f64(100000.0) {
        num_fmt = st("* ##############.00");
    }
    // ON ERROR GOTO -> ShowRuntimeError — error path not ported.
    // (UI) DIALOG END DlgWhoOwnsWhat, 0
    let p = g.cur_player;
    if g.entity_status[p] < 1 {
        return 0;
    }
    if g.entity_status[p] == 2 {
        describe_defunct_entity(g, ui, p);
        return 0;
    }
    let mut line_no: i32 = 0;
    let dash = st("                                ------------");
    macro_rules! set {
        ($i:expr, $x:expr) => {{
            let i: i32 = $i;
            lines[i as usize] = $x;
        }};
    }
    if g.tmp_corp_bonds_value > Ext::from_f64(0.0)
        || g.tmp_gov_bonds_value > Ext::from_f64(0.0)
        || g.bank_debt[p] > Ext::from_f64(10.0)
        || g.t_bills[p] + g.cash[p] > Ext::from_f64(10.0)
        || g.tmp_subsid_stock_value > Ext::from_f64(10.0)
        || g.phys_commod_value > Ext::from_f64(10.0)
    {
        let mut tbill_interest = Ext::from_f64(0.0);
        let mut salary_bonus = Ext::from_f64(0.0);
        let mut advances_interest = Ext::from_f64(0.0);
        let mut gov_bond_interest = Ext::from_f64(0.0);
        let mut corp_bond_interest = Ext::from_f64(0.0);
        let mut dividend_income = Ext::from_f64(0.0);
        let mut net_bond_amort = Ext::from_f64(0.0);
        let mut bond_amort;
        let mut bond_discount;
        let mut maturing_bonds = Ext::from_f64(0.0);
        let mut skip_amort = 0;
        let swap_pl: i64 = 0; // never set
        let mut maturity_year: i32;
        let mut quarters_left: i32;
        line_no += 2;
        set!(line_no, st(__content.string(2457)) + &g.million_prefix_uc + __content.string(2458));
        set!(line_no + 1, rule_line.clone());
        if g.t_bills[p] > Ext::from_f64(0.0) {
            let tbill_rate: Ext = cast::<Ext, _>(g.t_bill_rate);
            tbill_interest = g.t_bills[p] * (s(0.01f32) * tbill_rate);
            if !(g.t_bills[p] > Ext::from_f64(0.0)) {
                tbill_interest = Ext::from_f64(0.0);
            }
            tbill_interest = fv_nc(tbill_interest, &num_fmt);
            line_no += 1;
            set!(line_no + 1, st(__content.string(2459)) + &amount_text(tbill_interest, &num_fmt, 12));
        }
        if g.player_ceo_of[p] > 10 {
            salary_bonus = fv_nc(salary_bonus, &num_fmt);
            let at = string_fill(12, b" ") + &rtrim(ltrim(&format(salary_bonus, num_fmt.as_bytes())).as_bytes());
            if val(at.as_bytes()) > Ext::from_f64(0.0) {
                line_no += 1;
                let at = right(at.as_bytes(), 12);
                set!(line_no + 1, st(__content.string(2460)) + &at);
            }
        }
        if g.tmp_player_advances > Ext::from_f64(0.0) {
            advances_interest = g.tmp_player_advances * (s(0.01f32) * cast::<Ext, _>(g.prime_rate));
            advances_interest = fv_nc(advances_interest, &num_fmt);
            line_no += 1;
            set!(line_no + 1, st(__content.string(2461)) + &amount_text(advances_interest, &num_fmt, 12));
        }
        if g.tmp_gov_bonds_value > Ext::from_f64(0.0) {
            if g.long_gov_bond_face[p] > 0 {
                let coupon_rate: Ext = val(right(g.long_bond_desc.as_bytes(), 5).as_bytes());
                gov_bond_interest = (cast::<Ext, _>(g.long_gov_bond_face[p]) * s(0.01f32)) * coupon_rate;
                maturity_year = to_long(val(mid(g.long_bond_desc.as_bytes(), 1, 4).as_bytes()));
                quarters_left = maturity_year.wrapping_sub(g.cur_year).wrapping_mul(4).wrapping_sub(g.cur_quarter - 1);
                bond_discount = cast::<Ext, _>(g.long_gov_bond_face[p]) - g.lt_gov_bond_cost[p];
                if quarters_left < 1 {
                    quarters_left = 1;
                }
                bond_amort = bond_discount / cast::<Ext, _>(quarters_left);
                if quarters_left > 4 {
                    bond_amort = bond_amort * Ext::from_f64(4.0);
                }
                if maturity_year - g.cur_year < 2 {
                    bond_amort = bond_discount;
                }
                net_bond_amort = bond_amort;
            }
            if g.short_gov_bond_face[p] > 0 {
                let coupon_rate: Ext = val(right(g.short_bond_desc.as_bytes(), 5).as_bytes());
                gov_bond_interest = gov_bond_interest + (cast::<Ext, _>(g.short_gov_bond_face[p]) * s(0.01f32)) * coupon_rate;
                maturity_year = to_long(val(mid(g.short_bond_desc.as_bytes(), 1, 4).as_bytes()));
                quarters_left = maturity_year.wrapping_sub(g.cur_year).wrapping_mul(4).wrapping_sub(g.cur_quarter - 1);
                bond_discount = cast::<Ext, _>(g.short_gov_bond_face[p]) - g.st_gov_bond_cost[p];
                if quarters_left < 1 {
                    quarters_left = 1;
                }
                bond_amort = bond_discount / cast::<Ext, _>(quarters_left);
                if quarters_left > 4 {
                    bond_amort = bond_amort * Ext::from_f64(4.0);
                } else {
                    bond_amort = bond_discount;
                }
                if maturity_year - g.cur_year < 2 {
                    bond_amort = bond_discount;
                    if maturity_year - 1 == g.cur_year {
                        maturing_bonds = cast::<Ext, _>(g.short_gov_bond_face[p]);
                    }
                }
                net_bond_amort = net_bond_amort + bond_amort;
            }
            gov_bond_interest = fv_nc(gov_bond_interest, &num_fmt);
            line_no += 1;
            set!(line_no + 1, st(__content.string(2462)) + &amount_text(gov_bond_interest, &num_fmt, 12));
        }
        if g.tmp_corp_bonds_value > Ext::from_f64(0.0) {
            for slot in 1..=15 {
                let h = g.hold_bond_id[(slot, p)];
                if h == 0 || g.credit_rating_num[h] < 2 {
                    continue;
                }
                let info = g.bond_issue_info[h].clone();
                let coupon_rate: Ext = val(right(info.as_bytes(), 5).as_bytes());
                let mut bond_coupon = (cast::<Ext, _>(g.hold_bond_face[(slot, p)]) * s(0.01f32)) * coupon_rate;
                corp_bond_interest = corp_bond_interest + bond_coupon;
                maturity_year = to_long(val(mid(info.as_bytes(), 1, 4).as_bytes()));
                quarters_left = maturity_year.wrapping_sub(g.cur_year).wrapping_mul(4);
                if 0 < quarters_left {
                    quarters_left = quarters_left - (g.cur_quarter - 1);
                    if g.sim_co_count < h {
                        quarters_left += 1;
                    }
                }
                bond_discount = cast::<Ext, _>(g.hold_bond_face[(slot, p)]) - g.bond_hold_cost[(slot, p)];
                skip_amort = 0;
                if bond_discount < Ext::from_f64(0.0) && 0 < instr(1, info.as_bytes(), b"cv") {
                    skip_amort = 1;
                    // convPremiumFlag = 1 (not used)
                }
                let mut convert_called = 0;
                if g.bond_price[h].get() > Ext::from_f64(100.0) && 0 < instr(1, info.as_bytes(), b"cv") {
                    if maturity_year == g.cur_year {
                        convert_called = 1;
                    } else if maturity_year - 1 == g.cur_year
                        && g.bond_price[h].get() > cast::<Ext, _>(4 - g.cur_quarter) + Ext::from_f64(100.0)
                    {
                        let conv_threshold = val(parse_any(info.as_bytes(), b"|", 2).as_bytes());
                        if g.market_cap[h] > conv_threshold {
                            convert_called = 1;
                        }
                    }
                }
                if skip_amort == 0 {
                    if quarters_left < 1 {
                        quarters_left = 1;
                    }
                    bond_amort = bond_discount / cast::<Ext, _>(quarters_left);
                    if quarters_left > 4 {
                        bond_amort = bond_amort * Ext::from_f64(4.0);
                    } else {
                        bond_amort = bond_discount;
                    }
                    net_bond_amort = net_bond_amort + bond_amort;
                }
                if !(net_bond_amort.abs() >= s(0.5f32)) {
                    net_bond_amort = Ext::from_f64(0.0);
                } else {
                    net_bond_amort = fv_nc(net_bond_amort, &num_fmt);
                }
                if maturity_year > g.cur_year + 1 {
                    continue;
                }
                if g.cur_year + 1 == maturity_year {
                    if g.cur_quarter == 1 && g.sim_co_count < h {
                        continue;
                    }
                    if g.cur_quarter == 1 && convert_called == 0 {
                        maturing_bonds = maturing_bonds + cast::<Ext, _>(g.hold_bond_face[(slot, p)]);
                        continue;
                    }
                }
                if convert_called == 0 {
                    maturing_bonds = maturing_bonds + cast::<Ext, _>(g.hold_bond_face[(slot, p)]);
                }
                if g.cur_year == maturity_year {
                    corp_bond_interest = corp_bond_interest - bond_coupon * s(0.75f32);
                } else if g.cur_quarter != 1 {
                    if g.cur_quarter == 2 {
                        if g.sim_co_count < h {
                            continue;
                        }
                        bond_coupon = bond_coupon * s(0.25f32);
                    }
                    if g.cur_quarter == 3 {
                        if g.sim_co_count < h {
                            bond_coupon = bond_coupon * s(0.25f32);
                        } else {
                            bond_coupon = bond_coupon * s(0.5f32);
                        }
                    }
                    if g.cur_quarter == 4 {
                        if g.sim_co_count < h {
                            bond_coupon = bond_coupon * s(0.5f32);
                        } else {
                            bond_coupon = bond_coupon * s(0.75f32);
                        }
                    }
                    corp_bond_interest = corp_bond_interest - bond_coupon;
                }
            }
            corp_bond_interest = fv_nc(corp_bond_interest, &num_fmt);
            line_no += 1;
            set!(line_no + 1, st(__content.string(2463)) + &amount_text(corp_bond_interest, &num_fmt, 12));
        }
        if g.hold_stock_id[(0, p)] > 0 {
            for slot in 1..=15 {
                let h = cast::<i32, _>(g.hold_stock_id[(slot, p)]);
                if h == 0 {
                    continue;
                }
                let pct = cast::<Ext, _>(g.hold_stock_pct[(slot, p)]);
                if g.industry_num[h] == 71 {
                    let etf_dist_yield = val(parse_any(g.ent_misc_data[h].as_bytes(), b"|", 12).as_bytes());
                    dividend_income = dividend_income + pct * (etf_dist_yield * s(0.04f32));
                } else if g.dividend_per_share[h] != Ext::from_f64(0.0) {
                    dividend_income = dividend_income + g.shares_outstanding[h] * (g.dividend_per_share[h] * pct);
                }
            }
            dividend_income = fv_nc(dividend_income, &num_fmt);
            line_no += 1;
            let mut at = amount_text(dividend_income, &num_fmt, 12);
            if dividend_income < Ext::from_f64(0.0) {
                at = at + __content.string(2464);
            }
            set!(line_no + 1, st(__content.string(2465)) + &at);
        }
        let total_cash_income = ((((gov_bond_interest + tbill_interest) + corp_bond_interest) + dividend_income)
            + advances_interest)
            + salary_bonus;
        line_no += 1;
        set!(line_no + 1, dash.clone());
        set!(line_no + 2, st(__content.string(2466)) + &amount_text(total_cash_income, &num_fmt, 12));
        line_no += 1;
        let mut storage_fees: Ext = calc_phys_commod_storage_cost(g, p);
        if !(Ext::from_f64(0.0) >= storage_fees) {
            storage_fees = fv_nc(storage_fees, &num_fmt);
            if storage_fees != Ext::from_f64(0.0) {
                set!(
                    line_no + 2,
                    st(__content.string(2467)) + &amount_text(-1.0 * storage_fees, &num_fmt, 12)
                );
                line_no += 1;
            }
        } else {
            storage_fees = Ext::from_f64(0.0);
        }
        let mut interest_expense = (g.bank_debt[p] * cast::<Ext, _>(g.borrow_rate)) * s(0.01f32);
        if interest_expense < s(0.5f32) {
            interest_expense = Ext::from_f64(0.0);
        }
        interest_expense = fv_nc(interest_expense, &num_fmt);
        line_no += 1;
        set!(line_no + 1, st(__content.string(2468)) + &amount_text(-1.0 * interest_expense, &num_fmt, 12));
        set!(line_no + 2, dash.clone());
        let pre_tax_income = total_cash_income - (interest_expense + storage_fees);
        set!(line_no + 3, st(__content.string(2469)) + &amount_text(pre_tax_income, &num_fmt, 12));
        let taxable_income;
        if !(net_bond_amort.abs() >= s(0.5f32)) {
            net_bond_amort = Ext::from_f64(0.0);
            taxable_income = pre_tax_income;
        } else {
            line_no += 1;
            let at = amount_text(net_bond_amort, &num_fmt, 12);
            if net_bond_amort > Ext::from_f64(0.0) {
                set!(line_no + 3, st(__content.string(2470)) + &at);
            } else {
                set!(line_no + 3, st(__content.string(2471)) + &at);
            }
            line_no += 1;
            set!(line_no + 3, dash.clone());
            line_no += 1;
            taxable_income = pre_tax_income + net_bond_amort;
            set!(line_no + 3, st(__content.string(2472)) + &amount_text(taxable_income, &num_fmt, 12));
        }
        let mut income_tax = taxable_income * (cast::<Ext, _>(5 + g.corp_tax_rate) * s(0.01f32));
        if income_tax < Ext::from_f64(0.0) {
            income_tax = Ext::from_f64(0.0);
        }
        income_tax = fv_nc(income_tax, &num_fmt);
        set!(line_no + 4, st(__content.string(2473)) + &amount_text(-1.0 * income_tax, &num_fmt, 12));
        let mut capital_taxes = calc_capital_taxes(g, p);
        if g.tmp_capital_tax == Ext::from_f64(0.0) && g.wealth_tax_rate > 0 {
            g.tmp_capital_tax = (cast::<Ext, _>(g.wealth_tax_rate) * s(0.01f32))
                * ((g.net_worth + g.player_prepaid_tax[p]) - g.exchange_rate * Ext::from_f64(1000.0));
            if !(g.tmp_capital_tax > Ext::from_f64(1.0)) {
                g.tmp_capital_tax = Ext::from_f64(0.0);
            }
            g.corp_shares_tax_due = capital_taxes * Ext::from_f64(4.0);
        } else if g.corp_shares_tax_due > Ext::from_f64(0.0) {
            g.corp_shares_tax_due = g.corp_shares_tax_due * Ext::from_f64(4.0);
        }
        capital_taxes = g.tmp_capital_tax + g.corp_shares_tax_due;
        if capital_taxes > Ext::from_f64(0.0) {
            capital_taxes = fv_nc(capital_taxes, &num_fmt);
            line_no += 1;
            let at = amount_text(-Ext::from_f64(1.0) * capital_taxes, &num_fmt, 12);
            set!(line_no + 4, st(__content.string(2474)) + &at);
            if g.tmp_capital_tax == Ext::from_f64(0.0) {
                set!(line_no + 4, st(__content.string(2475)) + &at);
            }
            if g.corp_shares_tax_due == Ext::from_f64(0.0) {
                set!(line_no + 4, st(__content.string(2476)) + &at);
            }
        }
        set!(line_no + 5, dash.clone());
        let mut net_income = taxable_income - (income_tax + capital_taxes);
        let at = amount_text(net_income, &num_fmt, 12);
        set!(line_no + 6, st(__content.string(2477)) + &at);
        net_income = val(remove_any(at.as_bytes(), b",").as_bytes());
        line_no += 1;
        let tax = calc_player_income_tax(g, p);
        let mut prepaid_tax_diff = g.player_prepaid_tax[p] - tax;
        g.tmp_ext1 = Ext::from_f64(0.0);
        let at = amount_text(prepaid_tax_diff, &num_fmt, 12);
        prepaid_tax_diff = val(remove_any(at.as_bytes(), b",").as_bytes());
        if prepaid_tax_diff != Ext::from_f64(0.0) {
            if prepaid_tax_diff < Ext::from_f64(0.0) {
                set!(line_no + 6, st(__content.string(2478)) + &at);
            } else {
                set!(line_no + 6, st(__content.string(2479)) + &at);
            }
            line_no += 1;
        }
        if net_bond_amort != Ext::from_f64(0.0) {
            net_bond_amort = -Ext::from_f64(1.0) * net_bond_amort;
            let non_cash_text = amount_text(net_bond_amort, &num_fmt, 12);
            if net_bond_amort < Ext::from_f64(0.0) {
                set!(line_no + 6, st(__content.string(2480)) + &non_cash_text);
            } else {
                set!(line_no + 6, st(__content.string(2481)) + &non_cash_text);
            }
            line_no += 1;
        }
        if maturing_bonds > Ext::from_f64(0.0) {
            maturing_bonds = fv_nc(maturing_bonds, &num_fmt);
            let maturing_text = amount_text(maturing_bonds, &num_fmt, 12);
            set!(line_no + 6, st(__content.string(2482)) + &maturing_text);
            line_no += 1;
        }
        capital_taxes = fv_nc(-Ext::from_f64(1.0) * g.exchange_rate, &num_fmt);
        let living_exp_line = st(__content.string(2483)) + &amount_text(capital_taxes, &num_fmt, 10);
        set!(line_no + 6, living_exp_line);
        line_no += 1;
        set!(line_no + 6, dash.clone());
        let mut cash_adjustments = (maturing_bonds + net_bond_amort) + prepaid_tax_diff;
        cash_adjustments = cash_adjustments - g.exchange_rate * Ext::from_f64(1.0);
        net_income = net_income + cash_adjustments;
        let at = amount_text(net_income, &num_fmt, 12);
        line_no += 1;
        set!(line_no + 6, st(__content.string(2484)) + &at);
        set!(line_no + 7, st("                                ============"));
        if swap_pl > 0 {
            line_no += 1;
            set!(line_no + 7, st(__content.string(2485)));
        }
        if net_bond_amort != Ext::from_f64(0.0) {
            line_no += 1;
            set!(line_no + 7, st(__content.string(2486)));
            line_no += 1;
            set!(line_no + 7, st(__content.string(2487)));
        } else if skip_amort == 1 {
            line_no += 1;
            set!(line_no + 7, st(__content.string(2488)));
        }
        line_no += 1;
        set!(line_no + 7, st(__content.string(2489)));
        set!(line_no + 8, rule_line.clone());
        line_no += 8;
    } else {
        line_no += 2;
        set!(line_no, st(__content.string(2490)));
    }
    line_no += 2;
    if !lines[cast::<usize, _>(line_no + 2)].is_empty() {
        line_no += 2;
    }
    set!(line_no + 2, st("@END"));
    line_no += 2;
    let loop_limit = line_no + 3;
    for slot in 1..=loop_limit {
        g.text_lines[slot] = lines[cast::<usize, _>(slot)].clone();
        if lines[cast::<usize, _>(slot)] == "@END" {
            break;
        }
    }
    g.text_box_title = st("  === ") + &g.ent_name[p] + __content.string(2382);
    g.text_has_links = 1;
    show_text_report(g, ui, rng);
    0
}

// ---------------------------------------------------------------------------------------------------------
// CashFlowAlert
// ---------------------------------------------------------------------------------------------------------

/// Port of CashFlowAlert (0x6a9490).
/// Checks one corporation per call (`SimCoCount + 1000`, wrapped below 1601) that the current player
/// holds, controls, has active or (last) selected: if its projected cash status (mode 1) is 7 or 8 and
/// it was not among the last three alerts, a "FINANCIAL ALERT" box offers its cash-flow report;
/// `CashAlertCorp1..3` are rotated and a big corp of someone else becomes `DistressedCorp`. Offers to
/// make a controlled corp the active entity after the report (`SelectTargetCorp`; `time_secs` =
/// `VAL(RIGHT$(TIME$, 2))` for its `CalcStockPrice`).
pub fn cash_flow_alert(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    let __content = crate::content::current();
    let mut corp = g.sim_co_count + 1000;
    if corp > 1600 {
        corp -= 1600;
    }
    if corp < 86 || g.entity_status[corp] != 1 {
        return;
    }
    let mut flag = 0;
    if find_stock_holding(g, g.cur_player, corp) != 0 {
        flag = 1;
    }
    if g.active_entity == corp || g.cur_player == g.corp_controller[corp] {
        flag = 1;
    }
    if g.cur_suppress_cash_warn == 1 {
        flag = 0;
    }
    if flag == 0 && g.sel_entity != corp && g.last_sel_entity != corp {
        return;
    }
    if show_cash_flow_projection(g, ui, rng, corp, 1) > Ext::from_f64(6.0) {
        g.tmp_ext1 = Ext::from_f64(0.0);
        if !(g.cash_alert_corp3 == corp || g.cash_alert_corp2 == corp || g.cash_alert_corp1 == corp) {
            let mut msg_text = st(__content.string(2491))
                + &g.ent_name[corp]
                + "!"
                + "\r"
                + "\r"
                + &g.ent_name[corp]
                + __content.string(2492)
                + __content.string(2493);
            if g.t_bills[corp] + g.cash[corp] < Ext::from_f64(0.0) {
                calc_entity_financials(g, ui, rng, corp);
                if g.unused_credit_line == Ext::from_f64(0.0) {
                    msg_text = msg_text + __content.string(2494);
                }
            }
            msg_text = msg_text
                + "\r"
                + "\r"
                + __content.string(2495)
                + &g.ent_symbol[corp]
                + " NOW?";
            let answer = ui.msgbox(&msg_text, 8211, &st(__content.string(2496)));
            g.cash_alert_corp3 = g.cash_alert_corp2;
            g.cash_alert_corp2 = g.cash_alert_corp1;
            g.cash_alert_corp1 = corp;
            if 130 < corp && g.business_assets[corp] > Ext::from_f64(200.0) && g.cur_player != g.corp_controller[corp] {
                g.distressed_corp = corp;
            }
            if answer != 6 {
                return;
            }
            show_cash_flow_projection(g, ui, rng, corp, 0);
            if g.cur_player == g.corp_controller[corp] && g.sel_entity != corp {
                let msg_text = st("Make ") + &g.ent_name[corp] + __content.string(2497);
                let title = st(__content.string(2498)) + &g.ent_symbol[corp] + "?";
                let answer = ui.msgbox(&msg_text, 8228, &title);
                if answer != 7 {
                    g.target_corp = corp;
                    crate::logic::market::select_target_corp(g, ui, rng, time_secs);
                    // (UI) CONTROL SET TEXT hDlgMain, 202, EntSymbol(SelEntity)
                }
            }
        }
    }
    g.tmp_ext1 = Ext::from_f64(0.0);
}
