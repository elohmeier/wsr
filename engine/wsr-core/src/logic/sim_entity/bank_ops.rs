//! GOSUB SimEntity_BankOperations (0x4dcd27) of SimProcessNextEntity (see [`super`]): a bank's quarter.
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::Frame;
#[allow(unused_imports)]
use super::prelude::*;

use super::accounting::{
    bank_system_liquidity, calc_income_tax, pay_dividends, update_earnings_ytd,
};
use super::bonds::bond_buyback;
use super::dividends::ai_set_dividend;
use super::operating::{disc_spending_effects, random_margin_drift};
use crate::logic::finance::{calc_entity_financials, get_controller_type, get_managing_player};
use crate::logic::news::is_in_recent_headlines;
use crate::logic::research::bank_regulator_lending_check;
use crate::logic::valuation::{calc_dividend_yield, calc_stock_price};

/// GOSUB SimEntity_BankOperations (0x4dcd27): spending effects; the AI sells (or supports) one stock
/// holding; start-up interest income; marketing budget; bond interest; loan-loss reserves (problem
/// loans of borrowers rated D/C, consumer loans, largest loan, subprime securities) and consumer-loan
/// write-offs; income tax, EarningsYTD, dividends, revaluation; capital ratio (dividend raise, cut or
/// deposit run-off), regulator orders, loan-portfolio purchases from other banks or the liquidity step,
/// lending freezes, repayment of player advances, the problem-bank list or bankruptcy, bond buybacks;
/// the quarter's income accumulators are cleared.
pub(super) fn bank_operations(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    f.line_marker = PbStr::from("33000");
    disc_spending_effects(g, ui, rng, f);
    let e = f.ent_id;
    f.amt_long = cast::<i64, _>(g.hold_stock_id[(0, e)]);
    f.idx = 0;
    if f.amt_long > 2 {
        f.idx = rng.rnd_range(cast::<i32, _>(to_quad(cast::<Ext, _>(f.amt_long))), 2);
    } else {
        f.idx = 2;
    }
    if f.idx == 0 {
        f.amt_long = 0;
    }
    if 2 <= f.idx {
        let i = f.idx;
        f.amt_long = cast::<i64, _>(g.hold_stock_id[(i, e)]);
        if 0 < g.hold_stock_id[(0, e)]
            && f.ctrl_type != 1
            && !(f.ctrl_type == 2 && cast::<Ext, _>(f.amt_long) == cast::<Ext, _>(g.ai_target_corp))
            && g.hold_stock_pct[(i, e)] < 100
        {
            g.tmp_ext1 = cast::<Ext, _>(e);
            g.tmp_ext2 = cast::<Ext, _>(g.hold_stock_pct[(i, e)]);
            if g.tmp_ext2 > Ext::from_f64(30.0) && g.hold_stock_id[(0, e)] < 5 {
                g.tmp_ext2 = sf(0.6f32) * g.tmp_ext2;
            }
            g.tmp_ext3 = Ext::from_f64(2.0);
            f.other_id = cast::<i32, _>(f.amt_long);
            if !f.skips(0x4ed93b) {
                super::transactions::sell_stock_holding(g, ui, rng, f.other_id);
            }
            f.amt_long = 0;
        }
    }
    if cast::<Ext, _>(f.amt_long) == (cast::<Ext, _>(g.asbestos_corp)).abs()
        || cast::<Ext, _>(f.amt_long) == cast::<Ext, _>(g.superfund_corp)
    {
        f.amt_long = 0;
    }
    if f.amt_long > 0 && f.managing_player == 0 {
        support_or_sell_holding(g, ui, rng, f);
    }
    let e = f.ent_id;
    if g.cur_year == g.start_year && g.cur_quarter == 1 {
        f.amount = sf(0.0025f32) * ((Ext::from_f64(1.0) + cast::<Ext, _>(g.sofr_rate)) + sf(1.5f32) * rng.rnd());
        f.tmp_long = to_quad(g.tmp_bank_total_loans * f.amount);
        g.accrued_pl[e] = cast::<Ext, _>(f.tmp_long) + g.accrued_pl[e];
        g.cash[e] = cast::<Ext, _>(g.corp_tax_rate) * sf(0.01f32) * cast::<Ext, _>(f.tmp_long) + g.cash[e];
    }
    if f.managing_player == 0 {
        if f.credit_rating < 4 {
            if 1 < g.disc_spend_pct[e] {
                f.msg_part1 = PbStr::new();
                if g.disc_spend_pct[e] < 3 {
                    g.disc_spend_pct[e] = 0;
                    f.msg_part1 = PbStr::from(__content.string(10694));
                }
                g.disc_spend_pct[e] = cast::<i16, _>(to_quad(cast::<Ext, _>(g.disc_spend_pct[e]) * sf(0.3f32)));
                if g.rand_seed % 14 == 2 || g.cur_player == g.corp_controller[e] {
                    g.news_msg = g.ent_symbol[e].clone()
                        + __content.string(10695)
                        + &f.msg_part1;
                }
            }
        } else {
            if g.profit_margin[e] < -1 && g.disc_spend_pct[e] < 4 && g.earnings_ytd[e] > Ext::from_f64(400.0) {
                g.disc_spend_pct[e] = (cast::<i16, _>(f.credit_rating)).wrapping_add(2);
            }
            if 8 < g.profit_margin[e] && 8 < g.disc_spend_pct[e] {
                g.disc_spend_pct[e] = g.disc_spend_pct[e].wrapping_sub(5);
            }
        }
    }
    if !f.skips(0x4d5ae2) {
        super::earnings_news::pay_bond_interest(g, ui, rng, f);
    }
    let e = f.ent_id;
    f.reserve_provision = Ext::from_f64(0.0);
    if g.bad_debt_reserve[e] < Ext::from_f64(0.0) {
        f.reserve_provision = -g.bad_debt_reserve[e];
    }
    f.tmp_long = 0;
    f.amt_long = 0;
    f.idx = 1;
    while f.idx <= 1600 {
        let b = f.idx;
        if g.entity_status[b] == 1 && cast::<i32, _>(g.lending_bank[b]) == e && g.bank_debt[b] > Ext::from_f64(0.0) {
            calc_entity_financials(g, ui, rng, b);
            if g.credit_rating_str == "D" {
                f.tmp_long = to_quad(g.bank_debt[b] + cast::<Ext, _>(f.tmp_long));
            }
            if g.credit_rating_str == "C" {
                f.amt_long = to_quad(g.bank_debt[b] + cast::<Ext, _>(f.amt_long));
            }
        }
        f.idx += 1;
    }
    calc_entity_financials(g, ui, rng, e);
    g.bank_largest_loan = g.bank_largest_loan - g.bank_loans_total * sf(0.25f32);
    if g.bank_largest_loan < Ext::from_f64(0.0) {
        g.bank_largest_loan = Ext::from_f64(0.0);
    }
    f.rating_str = g.credit_rating_str.clone();
    f.credit_rating = g.credit_rating_num[e];
    f.required_reserve = (((cast::<Ext, _>(f.tmp_long) * sf(0.2f32) + g.tmp_bank_total_loans * sf(0.005f32))
        + cast::<Ext, _>(f.amt_long) * sf(0.03f32))
        + cast::<Ext, _>(g.bank_consumer_loans[e]) * sf(0.06f32))
        + g.bank_largest_loan * sf(0.1f32)
        + cast::<Ext, _>(g.subprime_secs[e]) * sf(0.03f32);
    if 0 < g.subprime_crisis_state {
        f.required_reserve = cast::<Ext, _>(g.subprime_secs[e]) * sf(0.1f32) + f.required_reserve;
    }
    f.cmp_tmp = 1;
    f.write_down_qtr = (g.cur_year + e) % 4 + f.cmp_tmp;
    let consumer_crisis = g.cur_quarter == f.write_down_qtr
        && Ext::from_f64(3.0) - Ext::from_f64(2.0) * rng.rnd() > cast::<Ext, _>(g.gdp_growth)
        && !(0 < g.subprime_crisis_state);
    if consumer_crisis {
        f.consumer_write_off = Ext::from_f64(6.0) - cast::<Ext, _>(g.gdp_growth);
        if sf(3.5f32) > f.consumer_write_off {
            f.consumer_write_off = Ext::from_f64(4.0);
        }
        if f.consumer_write_off > Ext::from_f64(10.0) {
            f.consumer_write_off = Ext::from_f64(10.0);
        }
        f.amount = f.consumer_write_off * (sf(0.6f32) * rng.rnd()) + f.consumer_write_off * sf(0.7f32);
        f.consumer_write_off = f.amount;
        if 5 < g.profit_margin[e] {
            f.consumer_write_off = sf(0.7f32) * f.consumer_write_off;
        }
        if g.profit_margin[e] < -5 {
            f.consumer_write_off = sf(1.2f32) * f.consumer_write_off;
        }
        if Ext::from_f64(2.0) > f.consumer_write_off {
            f.consumer_write_off = Ext::from_f64(2.0);
        } else if f.consumer_write_off > Ext::from_f64(12.0) {
            f.consumer_write_off = Ext::from_f64(2.0) * cast::<Ext, _>(g.difficulty_level) + Ext::from_f64(8.0);
        }
        f.amount = f.consumer_write_off * sf(0.01f32) * cast::<Ext, _>(g.bank_consumer_loans[e]);
        g.bad_debt_reserve[e] = g.bad_debt_reserve[e] - f.amount;
        g.bank_consumer_loans[e] = to_quad(cast::<Ext, _>(g.bank_consumer_loans[e]) - f.amount);
        if f.amount > Ext::from_f64(8.0) && g.last_ticker_news_kind != 1 && g.rand_seed % 3 == 1 {
            if !g.news_msg.is_empty() {
                post_news(g, ui, 0, 0);
            }
            g.news_msg = g.ent_symbol[e].clone()
                + " takes "
                + &g.currency_symbol
                + &fmt_trim(f.amount, b"* ###,.00")
                + " "
                + &g.million_prefix_lc
                + __content.string(10696);
            post_news(g, ui, e, 0);
            g.last_ticker_news_kind = 1;
        }
    } else {
        f.amount = cast::<Ext, _>(g.bank_consumer_loans[e]) * (sf(0.01f32) * (rng.rnd() * Ext::from_f64(1.0)));
        if (cast::<Ext, _>(g.gdp_growth)) < Ext::from_f64(0.0) {
            f.amount = Ext::from_f64(2.0) * f.amount;
        }
        g.bad_debt_reserve[e] = g.bad_debt_reserve[e] - f.amount;
        g.bank_consumer_loans[e] = to_quad(cast::<Ext, _>(g.bank_consumer_loans[e]) - f.amount);
    }
    f.consumer_write_off = f.amount;
    f.subprime_write_down = Ext::from_f64(0.0);
    if !f.skips(0x508952) {
        f.subprime_write_down = super::fin_inst::subprime_secs_write_down(g, rng, e);
    }
    f.amount = f.required_reserve - g.bad_debt_reserve[e];
    f.reserve_provision = f.amount * sf(0.25f32) + f.reserve_provision;
    if g.rand_seed % 3 + 3 > g.cur_quarter && g.start_year == g.cur_year {
        f.reserve_provision = sf(0.25f32) * f.reserve_provision;
    }
    if g.bad_debt_reserve[e] > Ext::from_f64(0.0)
        && g.bad_debt_reserve[e] > f.required_reserve
        && g.bad_debt_reserve[e] < f.required_reserve * sf(1.5f32)
    {
        f.reserve_provision = Ext::from_f64(0.0);
    }
    f.reserve_provision = g.reserve_adj_pending[e] + f.reserve_provision;
    g.reserve_adj_pending[e] = Ext::from_f64(0.0);
    g.accrued_pl[e] = g.accrued_pl[e] - f.reserve_provision;
    g.bad_debt_reserve[e] = f.reserve_provision + g.bad_debt_reserve[e];
    random_margin_drift(g, ui, rng, f);
    let e = f.ent_id;
    f.taxable_income = (g.div_recvd1[e] * sf(0.35f32) + g.accrued_pl[e]) + g.div_recvd2[e] * sf(0.5f32);
    f.taxable_income = (cast::<Ext, _>(g.extraordinary_pl[e]) + f.taxable_income) + g.consol_tax_inc_ytd[e];
    calc_income_tax(g, ui, rng, f);
    update_earnings_ytd(g, f);
    f.earnings_ytd_save = g.earnings_ytd[e];
    f.div_change = 0.0f32;
    if !f.skips(0x49b683) {
        let nw = g.net_worth;
        f.div_change = cast::<f32, _>(ai_set_dividend(g, rng, e, f.net_earnings, nw, f.managing_player));
    }
    if !f.skips(0x4d630d) {
        pay_dividends(g, ui, rng, f);
    }
    f.line_marker = PbStr::from("33185");
    g.valuation_mode = 3;
    calc_stock_price(g, ui, rng, e, f.time_secs);
    capital_ratio(g, ui, rng, f);
    let e = f.ent_id;
    if 0 < f.managing_player && g.dividend_per_share[e] < f.start_div_per_share {
        if g.dividend_per_share[e] == Ext::from_f64(0.0) {
            g.news_msg = PbStr::from(__content.string(10697))
                + &g.ent_symbol[e]
                + __content.string(10698);
            if is_in_recent_headlines(g, __content.bytes(10699)) == 1
                && g.cur_player != g.corp_controller[e]
            {
                g.news_msg = PbStr::new();
            }
        } else {
            g.news_msg =
                g.ent_symbol[e].clone() + __content.string(10700);
            if is_in_recent_headlines(g, __content.bytes(10701)) == 1
                && g.cur_player != g.corp_controller[e]
            {
                g.news_msg = PbStr::new();
            }
        }
    }
    post_news(g, ui, e, 0);
    f.slot = e - 1;
    f.amt_long = 0;
    if g.bank_debt[e] == Ext::from_f64(0.0) && g.cash[e] > Ext::from_f64(0.0) {
        if g.exchange_rate * Ext::from_f64(6500.0) < g.t_bills[e] + g.cash[e]
            && f.managing_player == 0
            && g.bank_alloc_off[e] == 0
        {
            loan_portfolio_purchase(g, rng, f);
        }
    } else {
        bank_system_liquidity(g, ui, rng, f, e);
        f.ctrl_type = get_controller_type(g, e);
        f.managing_player = get_managing_player(g, e);
    }
    calc_entity_financials(g, ui, rng, e);
    if f.managing_player == 0 {
        if !g.news_msg.is_empty() {
            post_news(g, ui, 0, 0);
        }
        if bank_regulator_lending_check(g, ui, e, 1) == 1 {
            if g.credit_frozen[e] == 0 && is_in_recent_headlines(g, __content.bytes(10702)) == 0 {
                g.news_msg = g.ent_symbol[e].clone() + __content.string(10703);
                if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) && is_in_recent_headlines(g, __content.bytes(10704)) == 0 {
                    g.news_msg = replace(g.news_msg.as_bytes(), b"lending", __content.bytes(10705));
                }
            }
            g.credit_frozen[e] = 1;
        } else if g.credit_frozen[e] == 1 {
            g.credit_frozen[e] = 0;
        }
        if !g.news_msg.is_empty() {
            post_news(g, ui, e, 0);
        }
    }
    if 5 < g.credit_rating_num[e] {
        f.idx = 1;
        while f.idx <= 5 {
            let p = f.idx;
            if g.player_advances[(p, e)] > Ext::from_f64(0.0) && p != g.corp_controller[e] {
                g.cash[p] = g.player_advances[(p, e)] + g.cash[p];
                g.cash[e] = g.cash[e] - g.player_advances[(p, e)];
                g.player_advances[(p, e)] = Ext::from_f64(0.0);
                g.news_msg =
                    g.ent_symbol[e].clone() + __content.string(10706) + &g.ent_symbol[p];
                post_news(g, ui, p, e);
            }
            f.idx += 1;
        }
    }
    if g.credit_rating_num[e] < 2 {
        if g.dividend_per_share[e] > Ext::from_f64(0.0) {
            g.dividend_per_share[e] = Ext::from_f64(0.0);
            f.div_payout = Ext::from_f64(0.0);
        }
        f.asset_amt = g.bond_unrealized_loss;
        f.asset_amt = -f.asset_amt;
        f.liabilities = g.total_assets - g.net_worth;
        let mut bankrupt = false;
        if g.credit_rating_str == "D"
            && (g.bad_debt_reserve[e] + g.total_assets) + f.asset_amt < f.liabilities * sf(0.8f32)
        {
            f.bankrupt_code = 11;
            g.tmp_ext1 = cast::<Ext, _>(f.bankrupt_code);
            g.tmp_ext2 = Ext::from_f64(0.0);
            if !f.skips(0x4e7250) {
                super::liquidation::bankruptcy_liquidation(g, ui, rng, f.ent_id, f.time_secs);
            }
            bankrupt = true;
        }
        if !bankrupt {
            f.msg_part1 = PbStr::from("FDIC's");
            if 0 < g.country_idx[e] {
                f.msg_part1 = PbStr::from(__content.string(10707));
            }
            f.msg_part2 = PbStr::from(__content.string(10708));
            match g.rand_seed % 5 {
                1 => f.msg_part2 = PbStr::from(__content.string(10709)),
                2 => f.msg_part2 = PbStr::from(__content.string(10710)),
                3 => f.msg_part2 = PbStr::from(__content.string(10711)),
                4 => f.msg_part2 = PbStr::from(__content.string(10712)),
                _ => {}
            }
            g.news_msg =
                g.ent_symbol[e].clone() + " on " + &f.msg_part1 + __content.string(10713) + &f.msg_part2;
            if is_in_recent_headlines(g, f.msg_part2.as_bytes()) == 0
                && g.cur_player == g.corp_controller[e]
                && g.suppress_popups != 1
            {
                post_news(g, ui, e, 0);
            }
        }
    } else if g.net_worth > Ext::from_f64(0.0)
        && g.t_bills[e] + g.cash[e] > Ext::from_f64(3000.0)
        && g.bonds_issued[e] > 0
        && f.managing_player == 0
    {
        g.tmp_ext1 = Ext::from_f64(0.0);
        g.tmp_ext2 = Ext::from_f64(2.0);
        g.tmp_ext3 = Ext::from_f64(0.0);
        if !f.skips(0x4e4128) {
            bond_buyback(g, ui, rng, e);
        }
        f.bond_op_code2 = to_quad(g.tmp_ext2);
        f.bond_op_code3 = to_quad(g.tmp_ext3);
        g.tmp_ext1 = Ext::from_f64(0.0);
        g.tmp_ext2 = Ext::from_f64(0.0);
        g.tmp_ext3 = Ext::from_f64(0.0);
    }
    g.div_recvd1[e] = Ext::from_f64(0.0);
    g.div_recvd2[e] = Ext::from_f64(0.0);
    g.equity_income_ytd[e] = Ext::from_f64(0.0);
    g.extra_items_ytd[e] = Ext::from_f64(0.0);
    g.consol_tax_inc_ytd[e] = Ext::from_f64(0.0);
    g.accrued_pl[e] = Ext::from_f64(0.0);
    g.extraordinary_pl[e] = 0;
    if g.bad_debt_reserve[e] < Ext::from_f64(0.0) {
        g.accrued_pl[e] = g.bad_debt_reserve[e] - g.exchange_rate * Ext::from_f64(10.0);
        g.bad_debt_reserve[e] = g.exchange_rate * Ext::from_f64(10.0);
    }
}

/// The holding block of SimEntity_BankOperations: a small 100%-owned subsidiary with tax losses gets
/// capital (to use its losses), else the AI may sell part of the holding.
fn support_or_sell_holding(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    let e = f.ent_id;
    let i = f.idx;
    let h = cast::<i32, _>(f.amt_long);
    f.slot = to_long(g.exchange_rate * Ext::from_f64(500.0));
    if 2 < g.industry_num[h]
        && g.market_cap[h] < g.exchange_rate * Ext::from_f64(30.0)
        && g.hold_stock_pct[(i, e)] == 100
    {
        if g.t_bills[e] + g.cash[e] > cast::<Ext, _>(f.slot)
            && g.tax_loss_carry[h] < -sf(0.2f32) * cast::<Ext, _>(f.slot)
            && g.tax_loss_carry[e] > -Ext::from_f64(100.0)
        {
            g.cash[e] = g.cash[e] - cast::<Ext, _>(f.slot);
            g.cash[h] = cast::<Ext, _>(f.slot) + g.cash[h];
            g.market_cap[h] = cast::<Ext, _>(f.slot) * sf(0.005f32) + g.market_cap[h];
            g.hold_stock_basis[(i, e)] = cast::<Ext, _>(f.slot) + g.hold_stock_basis[(i, e)];
            f.msg_part1 = PbStr::from(__content.string(10714));
            if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
                f.msg_part1 = PbStr::from(__content.string(10715));
            }
            g.news_msg = g.ent_symbol[e].clone()
                + &f.msg_part1
                + &g.ent_symbol[h]
                + __content.string(10716);
            if is_in_recent_headlines(g, __content.bytes(10717)) == 0 {
                post_news(g, ui, 0, 0);
            } else {
                g.news_msg = PbStr::new();
            }
        }
        return;
    }
    if 100 <= g.hold_stock_pct[(i, e)] && g.market_cap[h] <= g.exchange_rate * Ext::from_f64(350.0) {
        return;
    }
    if 20 <= g.hold_stock_pct[(1, e)]
        && g.exchange_rate * Ext::from_f64(500.0) < g.market_cap[h] * cast::<Ext, _>(g.hold_stock_pct[(i, e)])
        && f.ctrl_type == 2
        && g.hold_stock_id[(0, e)] < 6
    {
        return;
    }
    if Ext::from_f64(8.0) * rng.rnd() > Ext::from_f64(7.0)
        && f.ctrl_type != 1
        && f.ctrl_type != 2
        && cast::<Ext, _>(f.amt_long) != cast::<Ext, _>(g.ai_target_corp)
    {
        g.tmp_ext1 = cast::<Ext, _>(e);
        g.tmp_ext2 = cast::<Ext, _>(g.hold_stock_pct[(i, e)]);
        if 2 < g.rand_seed % 7 {
            g.tmp_ext2 = g.tmp_ext2 * (sf(0.7f32) * rng.rnd() + sf(0.1f32));
        }
        if Ext::from_f64(1.0) > g.tmp_ext2 {
            g.tmp_ext2 = Ext::from_f64(1.0);
        }
        g.tmp_ext3 = Ext::from_f64(2.0);
        f.other_id = h;
        if !f.skips(0x4ed93b) {
            super::transactions::sell_stock_holding(g, ui, rng, f.other_id);
        }
    }
}

/// The capital-ratio block of SimEntity_BankOperations: (net worth + reserve) / deposits decides a
/// dividend raise (> 12%), a dividend stop (< 3%) or a deposit run-off (3..5%).
fn capital_ratio(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    let e = f.ent_id;
    f.cust_deposits = g.bank_cust_deposits[e];
    if f.cust_deposits < Ext::from_f64(0.0) {
        f.cust_deposits = Ext::from_f64(0.0);
    }
    f.amount = (g.bad_debt_reserve[e] + g.net_worth)
        / ((g.bank_c_ds[e] + g.bank_demand_deposits[e]) + f.cust_deposits);
    if f.amount > sf(0.12f32) && get_controller_type(g, e) != 1 {
        let r = Ext::from_f64(2.0) * rng.rnd() + Ext::from_f64(5.0);
        if calc_dividend_yield(g, e) < r {
            g.dividend_per_share[e] = sf(1.25f32) * g.dividend_per_share[e];
        }
        return;
    }
    if sf(0.03f32) > f.amount {
        if g.dividend_per_share[e] > Ext::from_f64(0.0) {
            g.dividend_per_share[e] = Ext::from_f64(0.0);
            f.div_payout = Ext::from_f64(0.0);
        }
        return;
    }
    if f.amount > sf(0.03f32) && f.amount < sf(0.05f32) && Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(2.0) {
        f.cap_gain_share = (g.bank_c_ds[e] + g.bank_demand_deposits[e]) * sf(0.02f32);
        f.amount = g.total_assets
            - ((g.tmp_subsid_stock_value + g.bank_loans_total) + g.options_net_value);
        if f.amount > f.cap_gain_share * Ext::from_f64(5.0) {
            g.cash[e] = g.cash[e] - f.cap_gain_share;
            g.bank_demand_deposits[e] = sf(0.98f32) * g.bank_demand_deposits[e];
            g.bank_c_ds[e] = sf(0.98f32) * g.bank_c_ds[e];
            g.news_msg = g.ent_symbol[e].clone()
                + __content.string(10718);
            if is_in_recent_headlines(g, __content.bytes(10719)) == 1
                && g.cur_player != g.corp_controller[e]
            {
                g.news_msg = PbStr::new();
            }
        }
    }
    let _ = ui;
}

/// A cash-rich AI bank buys 1000 of consumer loans (or 500 of mortgages) from a random indebted bank.
fn loan_portfolio_purchase(g: &mut Globals, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    let e = f.ent_id;
    f.tmp_long = cast::<i64, _>(rng.rnd_range(85, 11));
    let b = cast::<i32, _>(f.tmp_long);
    if g.entity_status[b] != 1
        || 0 < instr(1, g.news_headline[59].as_bytes(), b"loan")
        || g.corp_controller[b] < 11
        || b == e
    {
        return;
    }
    if g.bank_consumer_loans[b] > 3500 && g.bank_debt[b] > Ext::from_f64(0.0) && 0 < g.bank_alloc_consumer_pct[e] {
        g.cash[e] = g.cash[e] - Ext::from_f64(1000.0);
        g.bank_consumer_loans[e] = to_quad(Ext::from_f64(1000.0) + cast::<Ext, _>(g.bank_consumer_loans[e]));
        g.cash[b] = Ext::from_f64(1000.0) + g.cash[b];
        g.bank_consumer_loans[b] = to_quad(cast::<Ext, _>(g.bank_consumer_loans[b]) - Ext::from_f64(1000.0));
        if g.rand_seed % 4 == 1 {
            g.news_msg = g.ent_symbol[e].clone()
                + __content.string(10720)
                + &g.ent_symbol[b];
        }
        return;
    }
    if g.bank_mortgage_loans[b] > 2500 && g.bank_debt[b] > Ext::from_f64(0.0) && 0 < g.bank_alloc_prime_mort_pct[e]
    {
        g.cash[e] = g.cash[e] - Ext::from_f64(500.0);
        g.bank_mortgage_loans[e] = to_quad(Ext::from_f64(500.0) + cast::<Ext, _>(g.bank_mortgage_loans[e]));
        g.cash[b] = Ext::from_f64(500.0) + g.cash[b];
        g.bank_mortgage_loans[b] = to_quad(cast::<Ext, _>(g.bank_mortgage_loans[b]) - Ext::from_f64(500.0));
        if g.rand_seed % 3 == 1 {
            g.news_msg =
                g.ent_symbol[e].clone() + __content.string(10721) + &g.ent_symbol[b];
        }
    }
}
