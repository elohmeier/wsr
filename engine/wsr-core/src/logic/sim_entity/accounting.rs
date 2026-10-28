//! GOSUB blocks of SimProcessNextEntity (see [`super`]): bank-loan repayment, quarter-end accounting
//! (income tax, dividends, insurer operations) and closing the books (working capital, cash shortfalls,
//! distressed asset sales, bankruptcy test).
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::Frame;
#[allow(unused_imports)]
use super::prelude::*;

use super::bank_ops::bank_operations;
use super::dividends::{
    adjust_options_for_corp_action, adjust_price_for_distribution, ai_corp_growth_and_dividends,
    ai_set_dividend, calc_consolidated_tax_payment, charge_shorts_for_distribution,
    etf_add_to_holdings,
};
use super::operating::{disc_spending_effects, random_margin_drift, withdraw_from_industry};
use crate::logic::cashflow::{
    calc_bank_loan_principal_due, get_etf_adviser_human_status, show_cash_flow_projection,
};
use crate::logic::control::find_option_positions;
use crate::logic::earnings::{balance_cash_t_bills, goodwill_write_down};
use crate::logic::finance::{calc_entity_financials, get_controller_type, get_managing_player};
use crate::logic::futures::recalc_commod_margin_balances;
use crate::logic::news::is_in_recent_headlines;
use crate::logic::options::close_option_positions;
use crate::logic::valuation::{calc_stock_price, calc_working_capital_ratio};

/// GOSUB SimEntity_RepayBankDebt (0x4cd5cf): corps (> 85) pay the quarter's principal due on their bank
/// loan to the lender (`CalcBankLoanPrincipalDue`; the amount is `covenantPay`).
pub(super) fn repay_bank_debt(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    f.line_marker = PbStr::from("25900");
    let e = f.ent_id;
    if g.entity_status[e] == 0 {
        return;
    }
    f.covenant_pay = Ext::from_f64(0.0);
    if g.bank_debt[e] == Ext::from_f64(0.0) || e < 86 {
        return;
    }
    f.covenant_pay = calc_bank_loan_principal_due(g, ui, rng, e, f.credit_rating);
    let l = g.lender_id;
    g.cash[l] = f.covenant_pay + g.cash[l];
    g.cash[e] = g.cash[e] - f.covenant_pay;
    g.bank_debt[e] = g.bank_debt[e] - f.covenant_pay;
    if g.bank_debt[e] < Ext::from_f64(0.0) {
        g.bank_debt[e] = Ext::from_f64(0.0);
    }
}

/// GOSUB SimEntity_QuarterEndAccounting (0x4cd774): banks go to their own block (SimEntity_BankOperations,
/// after the liquidity check); others write down goodwill, run the insurer block, compute taxable income
/// and income tax (GOSUB SimEntity_CalcIncomeTax), book the reserve adjustment, update `EarningsYTD`, and
/// set the dividend (AI growth plan and dividend policy, or the managing player's), with an occasional
/// special dividend of half the liquid funds for cash-rich corps.
pub(super) fn quarter_end_accounting(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    f: &mut Frame,
) {
    let __content = crate::content::current();
    f.line_marker = PbStr::from("26000");
    let e = f.ent_id;
    if g.entity_status[e] != 1 {
        return;
    }
    if g.industry_num[e] == 1 {
        if g.cash[e] < Ext::from_f64(0.0) || (g.cash[e] > Ext::from_f64(0.0) && g.bank_debt[e] > Ext::from_f64(0.0)) {
            bank_system_liquidity(g, ui, rng, f, e);
        }
        f.ctrl_type = get_controller_type(g, e);
        f.managing_player = get_managing_player(g, e);
        if !f.skips(0x4dcd27) {
            bank_operations(g, ui, rng, f);
        }
        return;
    }
    if 130 < e && g.goodwill[e] != Ext::from_f64(0.0) {
        f.goodwill_charge = goodwill_write_down(g, ui, rng, e, 0);
        g.accrued_pl[e] = g.accrued_pl[e] - f.goodwill_charge;
        g.goodwill[e] = g.goodwill[e] - f.goodwill_charge;
    }
    if g.industry_num[e] == 2 && !f.skips(0x4d1ac5) {
        insurer_operations(g, ui, rng, f);
    }
    let e = f.ent_id;
    f.taxable_income = (g.div_recvd1[e] * sf(0.35f32) + g.accrued_pl[e]) + g.div_recvd2[e] * sf(0.5f32);
    f.taxable_income = (cast::<Ext, _>(g.extraordinary_pl[e]) + f.taxable_income) + g.consol_tax_inc_ytd[e];
    calc_income_tax(g, ui, rng, f);
    f.net_earnings = f.net_earnings - g.reserve_adj_pending[e];
    g.contingency_reserve[e] = g.reserve_adj_pending[e] + g.contingency_reserve[e];
    f.reserve_adj = g.reserve_adj_pending[e];
    g.reserve_adj_pending[e] = Ext::from_f64(0.0);
    update_earnings_ytd(g, f);
    calc_entity_financials(g, ui, rng, e);
    if f.managing_player != 0 {
        f.div_change = cast::<f32, _>(set_dividend(g, rng, f));
        return;
    }
    if e <= 130 || 70 <= f.industry {
        if g.industry_num[e] == 2 {
            f.div_change = cast::<f32, _>(set_dividend(g, rng, f));
        }
    } else {
        f.other_id = to_long(Ext::from_f64(100.0) * cast::<Ext, _>(f.net_margin_frac));
        if Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(15.0) > cast::<Ext, _>(f.other_id) {
            g.tmp_ext1 = cast::<Ext, _>(f.other_id);
            f.cash_flow_score = to_long(show_cash_flow_projection(g, ui, rng, e, 2));
        }
        if Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(10.0) < cast::<Ext, _>(f.other_id) {
            let r = rng.rnd_range(10, 0);
            if (cast::<i32, _>(f.market_share)) < r + 5
                && f.cash_flow_score < 7
                && 0 <= g.monetary_stance
                && g.industry_num[e] != 70
            {
                f.tmp_pct = cast::<f32, _>(g.growth_rate[e]);
                if g.growth_rate[e] < 10 && f.cash_flow_score < 5 {
                    g.growth_rate[e] = cast::<i16, _>(to_quad(Ext::from_f64(8.0) * rng.rnd() + cast::<Ext, _>(g.growth_rate[e])));
                }
                if f.market_share < 10 && g.growth_rate[e] < 12 {
                    g.growth_rate[e] = cast::<i16, _>(to_quad(Ext::from_f64(9.0) * rng.rnd() + cast::<Ext, _>(g.growth_rate[e])));
                }
                if f.market_share < 5 && g.growth_rate[e] < 17 {
                    g.growth_rate[e] = cast::<i16, _>(to_quad(Ext::from_f64(8.0) * rng.rnd() + cast::<Ext, _>(g.growth_rate[e])));
                }
                f.tmp_pct = cast::<f32, _>(cast::<Ext, _>(g.growth_rate[e]) - cast::<Ext, _>(f.tmp_pct));
                if Ext::from_f64(5.0) < cast::<Ext, _>(f.tmp_pct) && g.monetary_stance == 0 {
                    g.growth_rate[e] =
                        cast::<i16, _>(to_quad(cast::<Ext, _>(g.growth_rate[e]) - cast::<Ext, _>(f.tmp_pct) * sf(0.7f32)));
                }
                f.tmp_short = cast::<i16, _>(8 - f.cash_flow_score);
                if f.market_share < 7 {
                    g.growth_rate[e] = f.tmp_short.wrapping_add(g.growth_rate[e]);
                }
            }
        }
        if !f.skips(0x6b2a08) {
            ai_corp_growth_and_dividends(g, ui, rng, e);
        }
        f.div_change = cast::<f32, _>(set_dividend(g, rng, f));
        if g.dividend_per_share[e] < f.start_div_per_share {
            f.div_change = -1.0f32;
        } else if g.dividend_per_share[e] > f.start_div_per_share {
            f.div_change = 1.0f32;
        } else {
            f.div_change = 0.0f32;
        }
    }
    let e = f.ent_id;
    if 15 < g.growth_rate[e] || f.div_change != Ext::from_f32(0.0f32) || !(g.earnings_y1[e] > Ext::from_f64(0.0)) {
        return;
    }
    if 15 < g.ind_growth_rate[cast::<i32, _>(g.industry_num[e])] {
        return;
    }
    if f.div_change != Ext::from_f32(0.0f32) {
        return;
    }
    let ind = g.industry_num[e];
    if 2 < ind && ind < 70 {
        let liquid = g.t_bills[e] + g.cash[e];
        if g.business_assets[e] * sf(0.6f32) < liquid && g.exchange_rate * Ext::from_f64(12000.0) < liquid {
            if g.net_worth > g.exchange_rate * Ext::from_f64(15000.0)
                && 6 < g.credit_rating_num[e]
                && g.rand_seed % 7 == 1
            {
                f.special_div_flag = 1;
            }
        } else if g.t_bills[e] + g.cash[e] > g.exchange_rate * Ext::from_f64(75000.0)
            && 7 < g.credit_rating_num[e]
            && g.rand_seed % 7 == g.cur_quarter
        {
            f.special_div_flag = 1;
        }
    }
    if f.special_div_flag != 1 {
        return;
    }
    f.special_div_mode = -1;
    f.div_payout = (g.t_bills[e] + g.cash[e]) * sf(0.5f32);
    g.cash[e] = g.cash[e] - f.div_payout;
    if !f.skips(0x4d630d) {
        pay_dividends(g, ui, rng, f);
    }
    let e = f.ent_id;
    if f.special_div_mode == 1 || f.div_payout > g.exchange_rate * Ext::from_f64(25000.0) {
        g.market_cap[e] = sf(0.95f32) * g.market_cap[e];
        if !g.news_msg.is_empty() {
            post_news(g, ui, 0, 0);
        }
        g.news_msg = f.ent_name.clone() + " (" + &g.ent_symbol[e] + __content.string(10461);
        post_news(g, ui, e, 0);
    }
    f.special_div_mode = 0;
}

/// `AiSetDividend(entId, netEarnings, NetWorth, managingPlayer)` (0x49b683).
fn set_dividend(g: &mut Globals, rng: &mut Rng, f: &mut Frame) -> i32 {
    if f.skips(0x49b683) {
        return 0;
    }
    let nw = g.net_worth;
    ai_set_dividend(g, rng, f.ent_id, f.net_earnings, nw, f.managing_player)
}

/// `EarningsYTD` = this quarter's net earnings in Q1, else accumulated; in Q2 an empty YTD of a corp with
/// prior-year earnings is first restored from the stored Q1 earnings record (field 1 of `EntMiscData`).
pub(super) fn update_earnings_ytd(g: &mut Globals, f: &mut Frame) {
    let e = f.ent_id;
    if g.cur_quarter == 1 {
        g.earnings_ytd[e] = f.net_earnings;
    } else {
        if g.cur_quarter == 2
            && g.earnings_ytd[e] == Ext::from_f64(0.0)
            && g.earnings_y1[e] != Ext::from_f64(0.0)
            && val(parse_any(g.ent_misc_data[e].as_bytes(), b"|", 1).as_bytes()) != Ext::from_f64(0.0)
        {
            g.earnings_ytd[e] = val(parse_any(g.ent_misc_data[e].as_bytes(), b"|", 1).as_bytes());
        }
        g.earnings_ytd[e] = f.net_earnings + g.earnings_ytd[e];
    }
}

/// `BankSystemLiquidity(id)` (0x4e635d).
pub(super) fn bank_system_liquidity(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    f: &mut Frame,
    id: i32,
) {
    if !f.skips(0x4e635d) {
        super::fin_inst::bank_system_liquidity(g, ui, rng, id);
    }
}

/// GOSUB SimEntity_CalcIncomeTax (0x4d7809): consolidated tax (CalcConsolidatedTaxPayment), the income
/// tax on accrued P&L and half the minority dividends, tax credits used (or passed to an 80% parent),
/// net earnings after tax and capital taxes, the [runtime resource wording] , investor sentiment.
pub(super) fn calc_income_tax(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let e = f.ent_id;
    g.tmp_ext1 = f.taxable_income;
    g.parent80_owner = 0;
    if !f.skips(0x4fa767) {
        calc_consolidated_tax_payment(g, ui, rng, e);
    }
    f.tax_parent_id = g.parent80_owner;
    f.consol_tax1 = g.tmp_ext1;
    f.consol_tax_cap = g.tmp_ext2;
    f.consol_tax3 = g.tmp_ext3;
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_ext2 = Ext::from_f64(0.0);
    g.tmp_ext3 = Ext::from_f64(0.0);
    if f.consol_tax3 > Ext::from_f64(0.0) {
        f.tax_flags = 1;
    }
    if f.consol_tax1 > Ext::from_f64(0.0) {
        f.tax_flags += 2;
    }
    f.pretax_income = g.div_recvd2[e] * sf(0.5f32) + g.accrued_pl[e];
    f.tax_amt = sf(0.01f32) * f.pretax_income * cast::<Ext, _>(g.corp_tax_rate);
    f.income_tax = f.tax_amt;
    let tp = cast::<i32, _>(f.tax_parent_id);
    if g.tax_credits[e] > Ext::from_f64(0.0) && f.tax_amt > Ext::from_f64(0.0) {
        if tp > 10 {
            f.tax_credit_used = g.tax_credits[e];
            g.cash[tp] = g.cash[tp] - f.tax_credit_used;
            g.tax_credits[tp] = f.tax_credit_used + g.tax_credits[tp];
            g.equity_income_ytd[tp] = g.equity_income_ytd[tp] - f.tax_credit_used;
        } else if g.tax_credits[e] > f.consol_tax_cap {
            f.tax_credit_used = f.consol_tax_cap;
        } else {
            f.tax_credit_used = g.tax_credits[e];
        }
        g.cash[e] = f.tax_credit_used + g.cash[e];
        g.tax_credits[e] = g.tax_credits[e] - f.tax_credit_used;
        if f.tax_credit_used > Ext::from_f64(0.0) && f.tax_amt > Ext::from_f64(0.0) {
            f.tax_amt = f.tax_amt - f.tax_credit_used;
        }
        if f.tax_amt > Ext::from_f64(0.0) && g.tax_credits[e] > Ext::from_f64(0.0) {
            if g.tax_credits[e] > f.tax_amt {
                f.tax_credit_used = f.tax_amt + f.tax_credit_used;
                f.tax_amt = Ext::from_f64(0.0);
            } else {
                f.tax_credit_used = g.tax_credits[e] + f.tax_credit_used;
                f.tax_amt = f.tax_amt - g.tax_credits[e];
            }
        }
    } else if g.tax_credits[e] > Ext::from_f64(0.0) && tp > 10 {
        f.tax_credit_used = g.tax_credits[e];
        g.cash[tp] = g.cash[tp] - f.tax_credit_used;
        g.tax_credits[tp] = f.tax_credit_used + g.tax_credits[tp];
        g.equity_income_ytd[tp] = g.equity_income_ytd[tp] - f.tax_credit_used;
        g.cash[e] = f.tax_credit_used + g.cash[e];
        g.tax_credits[e] = Ext::from_f64(0.0);
        f.tax_amt = f.tax_amt - f.tax_credit_used;
    } else if g.tax_credits[e] > Ext::from_f64(0.0) && !(f.tax_amt > Ext::from_f64(0.0)) && f.consol_tax_cap > Ext::from_f64(0.0) {
        f.tax_credit_used = g.tax_credits[e];
        if f.consol_tax_cap < f.tax_credit_used {
            f.tax_credit_used = f.consol_tax_cap;
        }
        g.cash[e] = f.tax_credit_used + g.cash[e];
        g.tax_credits[e] = g.tax_credits[e] - f.tax_credit_used;
    }
    f.income_tax = f.tax_amt;
    f.extraord_tax = sf(0.01f32) * cast::<Ext, _>(g.extraordinary_pl[e]) * cast::<Ext, _>(g.corp_tax_rate);
    f.net_earnings = ((g.div_recvd2[e] + g.accrued_pl[e]) + g.equity_income_ytd[e]) - f.tax_amt;
    f.net_earnings = f.net_earnings - f.capital_taxes;
    if g.industry_num[e] == 71 {
        f.net_earnings = g.div_recvd2[e] + g.accrued_pl[e];
        g.contingency_reserve[e] = Ext::from_f64(0.0);
    } else {
        f.cap_gain_dist = (g.extra_items_ytd[e] + cast::<Ext, _>(g.extraordinary_pl[e])) - f.extraord_tax;
        if f.cap_gain_dist.abs() < f.net_earnings.abs() * sf(0.05f32) {
            f.net_earnings = f.cap_gain_dist + f.net_earnings;
            f.cap_gain_dist = Ext::from_f64(0.0);
        }
        if f.net_earnings > g.earnings_y1[e] * sf(0.25f32) {
            g.investor_sentiment[e] =
                cast::<i32, _>(to_quad((Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(5.0)) + cast::<Ext, _>(g.investor_sentiment[e])));
        }
        if f.net_earnings < g.earnings_y1[e] * sf(0.25f32) {
            g.investor_sentiment[e] =
                cast::<i32, _>(to_quad(cast::<Ext, _>(g.investor_sentiment[e]) - (Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(5.0))));
        }
    }
}

/// GOSUB SimEntity_PayDividends (0x4d630d): the quarter's dividend (`DividendPerShare` x shares x 25; a
/// fund distributes 90% of its earnings, capital gains and possibly excess cash as return of capital) is
/// paid out of cash to every stockholder (cash; dividend income DivRecvd1/2 or basis for 80% holders;
/// fund capital gains; short sellers are charged); equity income of 20% holders. Also forced dividend
/// cuts of weak corps, and the special dividend (specialDivMode -1 -> 1 when paid to someone).
pub(super) fn pay_dividends(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    f.line_marker = PbStr::from("29850");
    let e = f.ent_id;
    calc_entity_financials(g, ui, rng, e);
    if g.industry_num[e] == 71 {
        f.special_div_mode = 0;
    }
    if f.special_div_mode != -1 {
        if g.industry_num[e] == 71 {
            if fund_distribution(g, ui, rng, f) {
                return;
            }
        } else if g.dividend_per_share[e] == Ext::from_f64(0.0) {
            f.div_payout = Ext::from_f64(0.0);
        } else {
            f.div_payout = g.shares_outstanding[e] * g.dividend_per_share[e] * Ext::from_f64(25.0);
            if f.div_payout > g.net_worth && g.net_worth > Ext::from_f64(0.0) {
                f.div_payout = Ext::from_f64(0.0);
                if 1 < g.industry_num[e] {
                    g.dividend_per_share[e] = sf(0.5f32) * g.dividend_per_share[e];
                }
            }
            if !(f.div_payout >= Ext::from_f64(1.0)) {
                f.div_payout = Ext::from_f64(0.0);
            } else {
                g.cash[e] = g.cash[e] - f.div_payout;
            }
        }
    }
    let e = f.ent_id;
    if g.dividend_per_share[e] < f.start_div_per_share
        && g.credit_rating_num[e] < 5
        && 0 < g.corp_controller[e]
        && g.corp_controller[e] <= g.num_players
        && f.special_div_mode == 0
        && get_controller_type(g, e) != 1
    {
        g.news_msg =
            g.ent_symbol[e].clone() + __content.string(10462);
        post_news(g, ui, 0, 0);
    }
    f.slot = 0;
    if f.div_payout > Ext::from_f64(0.0) && !f.skips(0x5e2f14) {
        charge_shorts_for_distribution(g, ui, rng, f.div_payout, e);
    }
    if g.industry_num[e] == 71 && f.cap_gain_dist > Ext::from_f64(0.0) && !f.skips(0x5e2f14) {
        charge_shorts_for_distribution(g, ui, rng, f.cap_gain_dist, e);
    }
    f.holder_id = 1;
    while f.holder_id <= 1600 {
        if g.entity_status[f.holder_id] == 1 {
            f.idx = 1;
            while f.idx <= 15 {
                if cast::<i32, _>(g.hold_stock_id[(f.idx, f.holder_id)]) == e {
                    pay_holder(g, ui, f);
                    f.idx = 15;
                }
                f.idx += 1;
            }
        }
        f.holder_id += 1;
    }
}

/// The fund (industry 71) part of SimEntity_PayDividends. Returns true when the fund was disqualified
/// (the GOSUB returns at once).
fn fund_distribution(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) -> bool {
    let __content = crate::content::current();
    let e = f.ent_id;
    if f.net_earnings > Ext::from_f64(0.0) {
        f.div_payout = f.net_earnings * sf(0.9f32);
    }
    if g.net_worth < f.div_payout + f.cap_gain_dist {
        g.news_msg =
            g.ent_symbol[e].clone() + __content.string(10463);
        if f.cap_gain_dist > Ext::from_f64(0.0) && g.net_worth < f.cap_gain_dist {
            g.news_msg = replace(g.news_msg.as_bytes(), b"gains", __content.bytes(10464));
        }
        let cp = g.cur_player;
        post_news(g, ui, e, cp);
        g.industry_num[e] = 70;
        g.dividend_per_share[e] = Ext::from_f64(0.0);
        f.div_payout = Ext::from_f64(0.0);
        f.cap_gain_dist = Ext::from_f64(0.0);
        return true;
    }
    if f.div_payout > Ext::from_f64(0.0) && !f.skips(0x619ed2) {
        adjust_price_for_distribution(g, ui, rng, e, f.div_payout);
    }
    if e != 1600
        && g.exchange_rate * Ext::from_f64(1000.0) < g.t_bills[e] + g.cash[e]
        && g.total_assets * sf(0.3f32) < g.t_bills[e] + g.cash[e]
        && !f.skips(0x64f921)
    {
        f.etf_add_result = etf_add_to_holdings(g, ui, rng, e, f.div_payout, f.time_secs);
    }
    if e != 1600
        && g.exchange_rate * Ext::from_f64(2000.0) < g.t_bills[e] + g.cash[e]
        && g.total_assets * sf(0.3f32) < g.t_bills[e] + g.cash[e]
        && f.etf_add_result == 0
        && 8 < g.credit_rating_num[e]
        && e != 1584
        && e != 1585
    {
        f.amt_long = to_quad(g.cash[e] + g.t_bills[e]);
        if g.extraordinary_pl[e] > 0 {
            f.amt_long = to_quad(cast::<Ext, _>(f.amt_long) - cast::<Ext, _>(g.extraordinary_pl[e]));
        }
        if !((cast::<Ext, _>(f.amt_long)) < g.exchange_rate * Ext::from_f64(4000.0))
            && (cast::<Ext, _>(f.amt_long)) > g.total_assets * sf(0.3f32)
            && !(f.cap_gain_dist > Ext::from_f64(100.0))
            && 13 < g.hold_stock_id[(0, e)]
            && g.start_year + 5 < g.cur_year
            && g.net_worth > g.exchange_rate * Ext::from_f64(10000.0)
            && g.cash[e] > g.exchange_rate * Ext::from_f64(500.0)
        {
            let skip = get_etf_adviser_human_status(g, ui, rng, e) > 0
                && g.auto_pilot[g.fund_adviser[e]] == 0;
            if !skip {
                f.div_payout = g.cash[e] * sf(0.2f32) + f.div_payout;
                f.return_of_capital = g.cash[e] * sf(0.2f32);
                g.news_msg = g.ent_symbol[e].clone()
                    + __content.string(10465);
                post_news(g, ui, e, 0);
                if !f.skips(0x619ed2) {
                    adjust_price_for_distribution(g, ui, rng, e, f.return_of_capital);
                }
                f.opt_adj = f.return_of_capital * -sf(0.01f32);
                if !f.skips(0x5f7680) {
                    adjust_options_for_corp_action(g, ui, e, f.opt_adj, 0, 2);
                }
            }
        }
    }
    g.cash[e] = g.cash[e] - f.div_payout;
    if f.cap_gain_dist > Ext::from_f64(0.0) {
        g.cash[e] = g.cash[e] - f.cap_gain_dist;
        if !f.skips(0x619ed2) {
            adjust_price_for_distribution(g, ui, rng, e, f.cap_gain_dist);
        }
        f.opt_adj = f.cap_gain_dist * -sf(0.01f32);
        if !f.skips(0x5f7680) {
            adjust_options_for_corp_action(g, ui, e, f.opt_adj, 0, 1);
        }
        g.extraordinary_pl[e] = 0;
    }
    f.start_div_per_share = Ext::from_f64(0.0);
    g.dividend_per_share[e] = Ext::from_f64(0.0);
    false
}

/// Pays holder `holderId` (holding slot `idx`) of `entId` its share of the distribution (SimEntity_PayDividends).
fn pay_holder(g: &mut Globals, ui: &mut dyn Ui, f: &mut Frame) {
    let __content = crate::content::current();
    let e = f.ent_id;
    let idx = f.idx;
    f.slot = f.holder_id;
    let s = f.slot;
    f.tmp_pct = cast::<f32, _>(g.hold_stock_pct[(idx, f.holder_id)]);
    let pct = cast::<Ext, _>(f.tmp_pct);
    if f.div_payout > Ext::from_f64(0.0) || f.cap_gain_dist > Ext::from_f64(0.0) || f.return_of_capital > Ext::from_f64(0.0) {
        if f.special_div_mode == -1 {
            f.special_div_mode = 1;
        }
        f.amount = sf(0.01f32) * (f.div_payout - f.return_of_capital) * pct;
        f.cap_gain_share = Ext::from_f64(0.0);
        f.tax_payment = Ext::from_f64(0.0);
        if g.industry_num[e] == 71 {
            f.tax_payment = pct * sf(0.01f32) * f.return_of_capital;
            if f.cap_gain_dist > Ext::from_f64(0.0) {
                f.cap_gain_share = pct * sf(0.01f32) * f.cap_gain_dist;
            }
        }
        g.cash[s] = ((f.amount + g.cash[s]) + f.cap_gain_share) + f.tax_payment;
        if g.cur_player == s && is_in_recent_headlines(g, __content.bytes(10466)) == 0 {
            f.div_news_amt = to_quad(f.tax_payment + f.amount);
            if 1 < f.div_news_amt {
                g.news_msg = g.tmp_symbol.clone()
                    + __content.string(10467)
                    + &g.ent_name[s]
                    + " of "
                    + &trim(g.currency_symbol.as_bytes())
                    + &fmt_trim(cast::<Ext, _>(f.div_news_amt), b"* ###,.00")
                    + " "
                    + &g.million_prefix_lc
                    + "illion";
            }
            if -Ext::from_f64(1.0) > f.cap_gain_share + cast::<Ext, _>(f.div_news_amt) {
                g.news_msg = g.ent_name[s].clone()
                    + __content.string(10468)
                    + &g.tmp_symbol
                    + __content.string(4361);
            }
            if !g.news_msg.is_empty() {
                post_news(g, ui, 0, 0);
            }
        }
        if Ext::from_f64(20.0) > pct {
            g.div_recvd2[s] = f.amount + g.div_recvd2[s];
            if g.industry_num[e] == 71 && f.cap_gain_dist > Ext::from_f64(0.0) {
                if s < 11 {
                    g.player_cap_gains[s] = f.cap_gain_share + g.player_cap_gains[s];
                    f.div_news_amt = to_quad(f.cap_gain_share);
                    if 1 < f.div_news_amt {
                        g.news_msg = g.tmp_symbol.clone()
                            + __content.string(10469)
                            + &g.ent_name[s]
                            + " of "
                            + &trim(g.currency_symbol.as_bytes())
                            + &fmt_trim(cast::<Ext, _>(f.div_news_amt), b"* ###,.00")
                            + " "
                            + &g.million_prefix_lc
                            + "il.";
                        post_news(g, ui, 0, 0);
                    }
                } else {
                    g.extraordinary_pl[s] =
                        to_quad(f.cap_gain_share + cast::<Ext, _>(g.extraordinary_pl[s]));
                }
            } else if g.industry_num[e] == 71 && f.return_of_capital > Ext::from_f64(0.0) {
                g.hold_stock_basis[(idx, s)] = g.hold_stock_basis[(idx, s)] - f.tax_payment;
                if g.hold_stock_basis[(idx, s)] < Ext::from_f64(0.0) {
                    f.asset_amt = g.hold_stock_basis[(idx, s)].abs();
                    if s < 11 {
                        g.player_cap_gains[s] = f.asset_amt + g.player_cap_gains[s];
                    } else {
                        g.extraordinary_pl[s] = to_quad(f.asset_amt + cast::<Ext, _>(g.extraordinary_pl[s]));
                    }
                    g.hold_stock_basis[(idx, s)] = Ext::from_f64(0.0);
                }
            }
        } else if Ext::from_f64(80.0) > pct || s < 11 {
            g.div_recvd1[s] = f.amount + g.div_recvd1[s];
        } else if s > 10 {
            g.hold_stock_basis[(idx, f.holder_id)] =
                g.hold_stock_basis[(idx, f.holder_id)] - f.amount;
        }
    }
    if s > 10 && f.special_div_mode == 0 && g.industry_num[s] != 71 && Ext::from_f64(19.0) < pct {
        g.equity_income_ytd[s] = sf(0.01f32) * f.net_earnings * pct + g.equity_income_ytd[s];
        g.extra_items_ytd[s] = sf(0.01f32) * f.cap_gain_dist * pct + g.extra_items_ytd[s];
    }
}

/// GOSUB SimEntity_InsurerOperations (0x4d1ac5): spending effects, AI sales of stock holdings (or
/// support of wholly owned subsidiaries), margin drift, underwriting profit, growth and spending policy
/// of the AI, government bond trading and other bond trading, subprime write-downs.
pub(super) fn insurer_operations(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    f.line_marker = PbStr::from("27000");
    disc_spending_effects(g, ui, rng, f);
    let e = f.ent_id;
    if f.ctrl_type != 1 && 1 < g.hold_stock_id[(0, e)] {
        f.idx = 1;
        loop {
            let i = f.idx;
            if 0 < g.hold_stock_id[(i, e)] {
                f.tmp_long = cast::<i64, _>(g.hold_stock_id[(i, e)]);
                let t = cast::<i32, _>(f.tmp_long);
                let skip = f.ctrl_type == 2
                    && (19 < g.hold_stock_pct[(i, e)]
                        || cast::<Ext, _>(f.tmp_long) == cast::<Ext, _>(g.ai_target_corp));
                if !skip {
                    let big = g.exchange_rate * Ext::from_f64(350.0)
                        <= g.market_cap[t] * cast::<Ext, _>(g.hold_stock_pct[(i, e)]);
                    if !big || g.hold_stock_pct[(i, e)] < 16 {
                        if g.hold_stock_pct[(i, e)] < 100 {
                            g.tmp_ext1 = cast::<Ext, _>(e);
                            g.tmp_ext2 = int(cast::<Ext, _>(g.hold_stock_pct[(i, e)]) * sf(0.5f32));
                            if g.hold_stock_pct[(i, e)] < 6 {
                                g.tmp_ext2 = cast::<Ext, _>(g.hold_stock_pct[(i, e)]);
                            }
                            g.tmp_ext3 = Ext::from_f64(2.0);
                            f.other_id = t;
                            if !f.skips(0x4ed93b) {
                                super::transactions::sell_stock_holding(g, ui, rng, f.other_id);
                            }
                            f.idx = 15;
                        } else if g.cash[e] > g.exchange_rate * Ext::from_f64(1000.0) {
                            if !(g.tax_loss_carry[t] < g.exchange_rate * -Ext::from_f64(500.0)) {
                                f.other_id = to_long(g.exchange_rate * Ext::from_f64(100.0));
                            } else {
                                f.other_id = to_long(g.exchange_rate * Ext::from_f64(500.0));
                            }
                            g.cash[e] = g.cash[e] - cast::<Ext, _>(f.other_id);
                            g.cash[t] = cast::<Ext, _>(f.other_id) + g.cash[t];
                            g.hold_stock_basis[(i, e)] =
                                cast::<Ext, _>(f.other_id) + g.hold_stock_basis[(i, e)];
                        }
                    }
                }
            }
            f.idx += 1;
            if !(f.idx < 16) {
                break;
            }
        }
    }
    f.tmp_long = to_quad(int(rng.rnd() * Ext::from_f64(9.0)) + Ext::from_f64(1.0));
    g.rand_seed = to_long(cast::<Ext, _>(f.tmp_long) + cast::<Ext, _>(g.rand_seed));
    random_margin_drift(g, ui, rng, f);
    f.uw_profit_ytd = g.insur_uw_profit_ytd[e];
    g.insur_uw_profit_ytd[e] = Ext::from_f64(0.0);
    f.start_growth = g.growth_rate[e];
    f.growth_adj = crate::pb::numeric::double(cast::<Ext, _>(g.growth_rate[e]));
    if Ext::from_f64(4.0) < f.growth_adj {
        f.growth_adj = crate::pb::numeric::double((cast::<Ext, _>(g.growth_rate[e]) - Ext::from_f64(4.0)) * sf(0.15f32) + sf(4.1f32));
    }
    if -Ext::from_f64(5.0) > f.growth_adj {
        f.growth_adj = crate::pb::numeric::double(-Ext::from_f64(5.0));
    }
    f.ins_margin = to_integer(cast::<Ext, _>(g.profit_margin[e]) + sf(0.7f32) * (Ext::from_f64(3.0) - f.growth_adj));
    if g.growth_rate[e] < -5 {
        g.profit_margin[e] = g.profit_margin[e].wrapping_add(1);
    }
    if 5 < g.growth_rate[e] {
        g.profit_margin[e] = g.profit_margin[e].wrapping_sub(1);
    }
    if 15 < g.growth_rate[e] {
        g.profit_margin[e] = g.profit_margin[e].wrapping_sub(1);
    }
    if 25 < g.growth_rate[e] {
        g.profit_margin[e] = g.profit_margin[e].wrapping_sub(1);
    }
    f.ins_margin = f.ins_margin.wrapping_sub(g.disc_spend_pct[e]);
    if f.managing_player == 0 {
        if g.profit_margin[e] < 3 {
            g.growth_rate[e] = g.growth_rate[e].wrapping_sub(2);
            g.disc_spend_pct[e] = g.disc_spend_pct[e].wrapping_add(2);
        }
        if 5 < g.profit_margin[e] {
            if g.exchange_rate * Ext::from_f64(25000.0) > g.t_bills[e] + g.cash[e] {
                g.growth_rate[e] = g.growth_rate[e].wrapping_add(3);
            }
            g.disc_spend_pct[e] = cast::<i16, _>(to_quad(sf(0.67f32) * cast::<Ext, _>(g.disc_spend_pct[e])));
        }
        if f.ins_margin < -7 && 8 < g.disc_spend_pct[e] {
            g.disc_spend_pct[e] = cast::<i16, _>(to_quad(sf(0.5f32) * cast::<Ext, _>(g.disc_spend_pct[e])));
        }
        if 2 < g.growth_rate[e] && g.net_worth < Ext::from_f64(0.0) {
            g.growth_rate[e] = -5;
        }
        if g.net_worth > g.ins_reserves[e] * sf(1.5f32)
            && !(g.net_worth * sf(0.2f32) <= g.t_bills[e] + g.cash[e])
        {
            if g.growth_rate[e] < 5 {
                g.growth_rate[e] = cast::<i16, _>(to_quad(Ext::from_f64(7.0) * rng.rnd() + cast::<Ext, _>(g.growth_rate[e])));
            }
        } else if 20 < g.growth_rate[e] {
            g.growth_rate[e] = cast::<i16, _>(to_quad(Ext::from_f64(5.0) * rng.rnd()));
        }
        if g.exchange_rate * Ext::from_f64(6000.0) < g.t_bills[e] + g.cash[e] {
            f.tmp_long = to_quad(cast::<Ext, _>(g.difficulty_level) + Ext::from_f64(6.0));
            if (cast::<Ext, _>(f.tmp_long)) < cast::<Ext, _>(g.growth_rate[e]) {
                g.growth_rate[e] = cast::<i16, _>(to_quad((cast::<Ext, _>(f.tmp_long) + Ext::from_f64(1.0)) * rng.rnd()));
            }
        }
        if get_controller_type(g, e) == 2
            && g.ins_reserves[e] < f.total_assets_save * sf(0.11f32)
            && 6 < f.credit_rating
        {
            if g.growth_rate[e] < 24 {
                g.growth_rate[e] = cast::<i16, _>(to_quad(Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(20.0)));
            }
            if g.growth_rate[e] < 25 {
                g.growth_rate[e] = cast::<i16, _>(to_quad(Ext::from_f64(10.0) * rng.rnd() + cast::<Ext, _>(g.growth_rate[e])));
            }
        }
    }
    if !f.skips(0x4e5b4b) {
        super::fin_inst::insurer_gov_bond_trading(g, ui, rng, e);
    }
    if g.t_bills[e] + g.cash[e] > Ext::from_f64(5000.0) && !f.skips(0x4dfde0) {
        super::economy_tick::fin_inst_bond_trading(g, ui, rng, f);
    }
    if f.managing_player == 0 {
        f.sel_case = cast::<Ext, _>(g.growth_rate[e]);
        if f.sel_case < -Ext::from_f64(15.0) {
            g.growth_rate[e] = -5;
            if Ext::from_f64(10.0) < cast::<Ext, _>(g.base_interest_rate) {
                g.growth_rate[e] = 7;
            }
        } else if f.sel_case > Ext::from_f64(22.0) && g.ai_target_corp != e {
            g.growth_rate[e] = g.growth_rate[e].wrapping_sub(7);
        }
    }
    if g.growth_rate[e] < -10 {
        g.growth_rate[e] = -10;
    }
    f.subprime_write_down = Ext::from_f64(0.0);
    if !f.skips(0x508952) {
        f.subprime_write_down = super::fin_inst::subprime_secs_write_down(g, rng, e);
    }
}

/// GOSUB SimEntity_CloseQuarterBooks (0x4ce6a0): clears the quarter's income accumulators, pays the
/// dividends, revalues the stock (ValuationMode 2), funds working capital (SimEntity_WorkingCapitalCash)
/// and tests for bankruptcy (liabilities beyond 125% of assets; insurers: negative adjusted net worth
/// over 20% of assets).
pub(super) fn close_quarter_books(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let e = f.ent_id;
    if g.entity_status[e] != 1 || e < 86 {
        return;
    }
    f.line_marker = PbStr::from("26100");
    g.div_recvd1[e] = Ext::from_f64(0.0);
    g.div_recvd2[e] = Ext::from_f64(0.0);
    g.equity_income_ytd[e] = Ext::from_f64(0.0);
    g.extra_items_ytd[e] = Ext::from_f64(0.0);
    g.accrued_pl[e] = Ext::from_f64(0.0);
    g.consol_tax_inc_ytd[e] = Ext::from_f64(0.0);
    if g.industry_num[e] != 71 {
        g.extraordinary_pl[e] = 0;
    }
    f.earnings_ytd_save = g.earnings_ytd[e];
    if !f.skips(0x4d630d) {
        pay_dividends(g, ui, rng, f);
    }
    g.valuation_mode = 2;
    calc_stock_price(g, ui, rng, f.ent_id, f.time_secs);
    if !f.skips(0x4d2976) {
        working_capital_cash(g, ui, rng, f);
    }
    let e = f.ent_id;
    if f.bankrupt_code == 0 {
        calc_entity_financials(g, ui, rng, e);
        f.sel_case = cast::<Ext, _>(g.industry_num[e]);
        if f.sel_case == Ext::from_f64(2.0) {
            f.asset_amt = g.bond_unrealized_loss;
            if f.asset_amt > Ext::from_f64(0.0) {
                f.asset_amt = Ext::from_f64(0.0);
            } else {
                f.asset_amt = f.asset_amt.abs();
            }
            if f.asset_amt + g.net_worth < Ext::from_f64(0.0)
                && (f.asset_amt + g.net_worth).abs() > g.total_assets * sf(0.2f32)
            {
                f.bankrupt_code = 11;
            }
        } else if g.total_assets < (g.tmp_total_debt - g.contingency_reserve[e]) * sf(0.8f32) {
            f.bankrupt_code = 11;
        }
    }
    if 0 < f.bankrupt_code && f.bankrupt_code != 99 {
        g.tmp_ext1 = cast::<Ext, _>(f.bankrupt_code);
        g.tmp_ext2 = Ext::from_f64(0.0);
        if !f.skips(0x4e7250) {
            super::liquidation::bankruptcy_liquidation(g, ui, rng, f.ent_id, f.time_secs);
        }
    }
}

/// GOSUB SimEntity_WorkingCapitalCash (0x4d2976): moves part of the working-capital gap of an operating
/// corp to/from cash, then covers a cash deficit step by step: T-bills, the credit line, closing option
/// positions, margin cover, a bond issue (AI), futures liquidation, forced bond sales, stock sales,
/// distressed asset sales; still negative with no assets left -> bankruptCode 7 (11 for insurers).
pub(super) fn working_capital_cash(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    f.line_marker = PbStr::from("27400");
    let e = f.ent_id;
    if 130 < e && g.industry_num[e] < 70 {
        f.cap_gain_share = g.business_assets[e] * calc_working_capital_ratio(g, e);
        f.amount = f.cap_gain_share - g.working_capital[e];
        if g.credit_rating_num[e] < 8 {
            f.amount = sf(0.5f32) * f.amount;
        } else {
            f.amount = sf(0.95f32) * f.amount;
        }
        g.working_capital[e] = f.amount + g.working_capital[e];
        g.cash[e] = g.cash[e] - f.amount;
    }
    recalc_commod_margin_balances(g, ui, rng, e);
    if !(g.cash[e] < Ext::from_f64(0.0)) {
        return;
    }
    balance_cash_t_bills(g, e);
    if g.cash[e] < Ext::from_f64(0.0) {
        draw_credit_line(g, ui, rng, f);
    }
    if g.cash[e] < Ext::from_f64(0.0) {
        f.other_id = find_option_positions(g, ui, e, 0, 0, 0);
        if f.other_id == 41 || f.other_id == 42 {
            f.tries = 0;
            while f.other_id > 40 && g.cash[e] < Ext::from_f64(0.0) {
                f.other_id = find_option_positions(g, ui, e, 0, 0, 0);
                g.tmp_ext1 = Ext::from_f64(0.0);
                g.tmp_ext2 = Ext::from_f64(0.0);
                close_option_positions(g, ui, rng, e, 0, 0, 0);
                f.tries += 1;
                if 1000 < f.tries {
                    break;
                }
            }
        }
    }
    if g.cash[e] < Ext::from_f64(0.0) && 130 < e && !f.skips(0x6625d7) {
        // GOSUB SimEntity_CoverMarginDeficit (0x4d41e2)
        super::transactions::cover_cash_deficit(g, ui, rng, e, &f.now.clone());
    }
    if g.cash[e] < g.exchange_rate * -Ext::from_f64(100.0) && 5 < f.credit_rating {
        let n = if f.skips(0x687f52) {
            0
        } else {
            super::bonds::count_bond_issuers(g)
        };
        if n < 700 && g.bonds_issued[e] == 0 && Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) && f.managing_player == 0 {
            g.tmp_ext1 = Ext::from_f64(0.0);
            if !f.skips(0x5231bc) {
                super::bonds::issue_bonds(g, ui, rng, e, Ext::from_f64(0.0), 0);
            }
        }
    }
    if g.cash[e] < Ext::from_f64(0.0) && 85 < e {
        f.other_id = 0;
        if !f.skips(0x62e730) {
            f.other_id = super::transactions::liquidate_futures_positions(g, ui, rng, e, 1, 1);
        }
        if 0 < f.other_id {
            g.news_msg = PbStr::from(__content.string(10470))
                + &g.ent_symbol[e]
                + __content.string(10471);
            if 1 < f.other_id {
                g.news_msg = g.news_msg.clone() + "s";
            }
            post_news(g, ui, 0, 0);
        }
    }
    if g.cash[e] < Ext::from_f64(0.0) && !f.skips(0x4d30ba) {
        forced_bond_sales(g, ui, rng, f);
    }
    if g.cash[e] < Ext::from_f64(0.0) && sf(0.3f32) < cast::<Ext, _>(f.margin_frac) && 130 < e {
        sell_holding_for_cash(g, ui, rng, f);
    }
    if g.cash[e] < Ext::from_f64(0.0) && !f.skips(0x4d31c1) {
        distressed_asset_sale(g, ui, rng, f);
    }
    if g.cash[e] < Ext::from_f64(0.0) {
        sell_holding_for_cash(g, ui, rng, f);
    }
    if g.cash[e] < Ext::from_f64(0.0) {
        calc_entity_financials(g, ui, rng, e);
        if g.total_assets > Ext::from_f64(0.0) {
            return;
        }
        f.bankrupt_code = 7;
        if 85 < e && g.industry_num[e] == 2 {
            f.bankrupt_code = 11;
        }
    }
}

/// GOSUB SimEntity_DrawCreditLine (0x4d3093): `DrawCreditLineForCash(entId)`, added to `bondOpsTotal`.
pub(super) fn draw_credit_line(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    if f.skips(0x4d3093) {
        return;
    }
    f.amount = Ext::from_f64(0.0);
    if !f.skips(0x506842) {
        f.amount = super::corp_ops::draw_credit_line_for_cash(g, ui, rng, f.ent_id);
    }
    f.bond_ops_total = f.amount + f.bond_ops_total;
}

/// GOSUB SimEntity_ForcedBondSales (0x4d30ba): sells government bonds, then corporate bonds
/// (`ForcedSellBonds` modes 1 and 2) to cover a cash deficit.
pub(super) fn forced_bond_sales(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    f.line_marker = PbStr::from("28000");
    let e = f.ent_id;
    balance_cash_t_bills(g, e);
    if !(g.cash[e] < Ext::from_f64(0.0)) {
        return;
    }
    f.amount = f.bond_int_status;
    f.cap_gain_share = f.unused_amt;
    f.bond_int_status = Ext::from_f64(0.0);
    if !f.skips(0x500723) {
        f.bond_int_status =
            super::transactions::forced_sell_bonds(g, ui, rng, e, 1, f.amount, f.cap_gain_share);
    }
    if !(g.cash[e] < Ext::from_f64(0.0)) {
        return;
    }
    f.amount = f.bond_int_status;
    f.unused_amt = Ext::from_f64(0.0);
    if !f.skips(0x500723) {
        f.unused_amt =
            super::transactions::forced_sell_bonds(g, ui, rng, e, 2, f.amount, f.cap_gain_share);
    }
}

/// GOSUB SimEntity_SellHoldingForCash (0x4d41ef): `soldList = ForcedSellStocks(entId, managingPlayer)`.
pub(super) fn sell_holding_for_cash(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    f: &mut Frame,
) {
    if f.skips(0x4d41ef) {
        return;
    }
    f.line_marker = PbStr::from("28300");
    f.sold_list = PbStr::new();
    if !f.skips(0x4ff864) {
        f.sold_list =
            super::transactions::forced_sell_stocks(g, ui, rng, f.ent_id, f.managing_player);
    }
}

/// GOSUB SimEntity_DistressedAssetSale (0x4d31c1): an operating corp short of cash sells business assets
/// in blocks of 100 (at a price depending on its margin, spending and the difficulty level), cutting
/// growth; withdraws from the industry when nothing is left to sell; books the gain/loss (`extraPlAdj`)
/// and builds the asset-sale headline.
pub(super) fn distressed_asset_sale(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    f: &mut Frame,
) {
    let __content = crate::content::current();
    f.line_marker = PbStr::from("28100");
    let e = f.ent_id;
    if e < 131 || 69 < g.industry_num[e] {
        return;
    }
    f.assets_sold = crate::pb::numeric::double(Ext::from_f64(0.0));
    if get_managing_player(g, e) == 0 {
        g.distressed_corp = e;
    }
    f.ind_outlook2 = crate::pb::numeric::double(cast::<Ext, _>(g.disc_spend_pct[e]) + Ext::from_f64(100.0) * cast::<Ext, _>(f.margin_frac));
    f.ind_outlook2 = crate::pb::numeric::double(f.ind_outlook2 - (cast::<Ext, _>(g.difficulty_level) + Ext::from_f64(9.0)));
    f.ind_outlook2 = crate::pb::numeric::double(sf(0.01f32) * f.ind_outlook2);
    while !(g.business_assets[e] < Ext::from_f64(100.0)) && g.cash[e] < Ext::from_f64(0.0) {
        if 4 < g.disc_spend_pct[e] {
            g.disc_spend_pct[e] = g.disc_spend_pct[e].wrapping_sub(1);
        }
        g.business_assets[e] = g.business_assets[e] - Ext::from_f64(100.0);
        f.wc_released = calc_working_capital_ratio(g, e) * Ext::from_f64(100.0);
        g.working_capital[e] = g.working_capital[e] - f.wc_released;
        f.amount = sf(0.1f32);
        if g.difficulty_level == 2 {
            f.amount = sf(0.15f32);
        }
        if g.difficulty_level == 1 {
            f.amount = sf(0.2f32);
        }
        if f.amount > f.ind_outlook2 {
            f.ind_outlook2 = crate::pb::numeric::double(f.amount);
        }
        if sf(0.32f32) < f.ind_outlook2 {
            f.ind_outlook2 = crate::pb::numeric::double(sf(0.32f32));
        }
        g.cash[e] = ((Ext::from_f64(50.0) + g.cash[e]) + Ext::from_f64(200.0) * f.ind_outlook2) + f.wc_released;
        f.assets_sold = crate::pb::numeric::double(Ext::from_f64(1.0) + f.assets_sold);
        if Ext::from_f64(5.0) > f.assets_sold && -9 < g.growth_rate[e] {
            g.growth_rate[e] = cast::<i16, _>(to_quad(cast::<Ext, _>(g.growth_rate[e]) - (Ext::from_f64(10.0) - f.assets_sold)));
        }
        if g.growth_rate[e] < -10 {
            g.growth_rate[e] = -10;
        }
    }
    if g.cash[e] < Ext::from_f64(0.0) && (g.business_assets[e] < g.exchange_rate * Ext::from_f64(25.0) || f.assets_sold == Ext::from_f64(0.0)) {
        withdraw_from_industry(g, ui, rng, f);
        if Ext::from_f64(0.0) < f.assets_sold {
            f.extra_pl_adj = (Ext::from_f64(2.0) * f.ind_outlook2 + sf(0.5f32)) * (Ext::from_f64(100.0) * f.assets_sold);
            f.extra_pl_adj = f.extra_pl_adj - Ext::from_f64(100.0) * f.assets_sold;
        }
        return;
    }
    f.extra_pl_adj = (Ext::from_f64(2.0) * f.ind_outlook2 + sf(0.5f32)) * (Ext::from_f64(100.0) * f.assets_sold);
    f.extra_pl_adj = f.extra_pl_adj - Ext::from_f64(100.0) * f.assets_sold;
    if g.rand_seed % 7 != 5 && !(f.extra_pl_adj.abs() > Ext::from_f64(300.0)) && Ext::from_f64(15.0) >= f.assets_sold {
        return;
    }
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    g.asset_sale_desc = PbStr::new();
    if !f.skips(0x4e1095) {
        super::asset_sales::gen_asset_sale_description(g, rng, e);
    }
    let sym = g.ent_symbol[e].clone();
    f.msg_part1 = sym.clone() + __content.string(10472);
    f.msg_part2 = PbStr::new();
    f.msg_part3 = PbStr::from(" sells ");
    if g.rand_seed % 4 == 1 {
        f.msg_part3 = PbStr::from(__content.string(10473));
    }
    if g.rand_seed % 4 == 2 {
        f.msg_part3 = PbStr::from(__content.string(10474));
    }
    if Ext::from_f64(6.0) * rng.rnd() > Ext::from_f64(2.0) {
        f.msg_part1 = sym.clone() + __content.string(10475) + &f.msg_part3;
    }
    if Ext::from_f64(6.0) * rng.rnd() > Ext::from_f64(3.0) {
        f.msg_part1 = PbStr::from(__content.string(10476)) + &sym + &f.msg_part3;
    }
    if Ext::from_f64(6.0) * rng.rnd() > Ext::from_f64(4.0) {
        f.msg_part1 = sym.clone() + __content.string(10477);
    }
    if Ext::from_f64(6.0) * rng.rnd() > Ext::from_f64(5.0) {
        f.msg_part1 = PbStr::from(__content.string(10478)) + &sym + " sells ";
    }
    if len(g.asset_sale_desc.as_bytes()) + len(f.msg_part1.as_bytes()) < 50 {
        if f.extra_pl_adj < -Ext::from_f64(100.0) {
            f.msg_part2 = PbStr::from(__content.string(10479));
        }
        if f.extra_pl_adj > Ext::from_f64(100.0) {
            f.msg_part2 = PbStr::from(__content.string(10480));
        }
    }
    g.news_msg = f.msg_part1.clone() + &g.asset_sale_desc + &f.msg_part2 + ".";
    f.curr_sym = g.currency_symbol.clone();
    if f.extra_pl_adj.abs() > Ext::from_f64(300.0) {
        f.msg_part2 = fmt_trim(f.extra_pl_adj.abs(), b"* ###,")
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix;
        if f.extra_pl_adj < Ext::from_f64(0.0) {
            f.msg_part1 = PbStr::from(" loss.");
        } else {
            f.msg_part1 = PbStr::from(" gain.");
        }
        g.news_msg = sym.clone()
            + " sells "
            + &g.asset_sale_desc
            + " at "
            + &f.curr_sym
            + &f.msg_part2
            + &f.msg_part1;
    } else if g.rand_seed % 4 == 3 {
        f.msg_part3 = fmt_trim(Ext::from_f64(100.0) * f.assets_sold, b"* ###,");
        g.news_msg = sym.clone()
            + " sells "
            + &g.asset_sale_desc
            + " ("
            + &f.curr_sym
            + &f.msg_part3
            + " "
            + &g.million_prefix_lc
            + "illion"
            + " cost)";
        if val(f.msg_part3.as_bytes()) == Ext::from_f64(0.0) {
            g.news_msg = PbStr::new();
        }
    }
    f.asset_sale_text = g.asset_sale_desc.clone();
    if cast::<i32, _>(g.industry_num[e]) == g.last_ticker_news_kind || g.asset_sale_desc.is_empty() {
        g.news_msg = PbStr::new();
    }
    g.last_ticker_news_kind = cast::<i32, _>(g.industry_num[e]);
}
