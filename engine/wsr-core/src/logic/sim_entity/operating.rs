//! GOSUB blocks of SimProcessNextEntity (see [`super`]): the operating results of an entity
//! (`SimEntity_OperatingResults` 0x4c9e42) and its sub-blocks.
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::Frame;
#[allow(unused_imports)]
use super::prelude::*;

use super::earnings_news::pay_bond_interest;
use crate::logic::cashflow::get_etf_adviser_human_status;
use crate::logic::control::{find_option_positions, update_corp_control};
use crate::logic::finance::{
    calc_capital_taxes, calc_entity_financials, get_controller_type, get_industry_category,
    get_managing_player,
};
use crate::logic::futures::{count_futures_positions, recalc_commod_margin_balances};
use crate::logic::holdings::{find_bond_holding_slot, find_holding_slot, find_stock_holding};
use crate::logic::news::is_in_recent_headlines;
use crate::logic::options::close_option_positions;
use crate::logic::reports_corp::assign_etf_adviser;
use crate::logic::swaps::cancel_entity_swaps;
use crate::logic::valuation::calc_industry_outlook;
use crate::pb::string::{CaseMode, mcase};

/// GOSUB SimEntity_OperatingResults (0x4c9e42): the quarter's operations of entity `SimCoCount`:
/// futures/option clean-up for weak credit, asset sales and purchases of the AI, fund management fees
/// and ETF trading (industry 71), capital taxes, control update, forced futures liquidation, the funding
/// need and financing (GOSUB SimEntity_AiFinancing), capital infusions and stock issues of wholly owned
/// subsidiaries, operations moved downstream, strikes, bond interest, and for operating corps the
/// discretionary spending, margins and growth (or withdrawal from the industry).
pub(super) fn operating_results(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    if g.sim_co_count == 200 {
        trim_mgmt_change_lists(g, ui, rng, f);
    }
    if 300 < g.sim_co_count && g.sim_co_count % 300 == 1 {
        f.cmp_tmp = 25;
        if f.cmp_tmp < len(g.new_mgmt_list.as_bytes()) {
            trim_mgmt_change_lists(g, ui, rng, f);
        }
    }
    f.cap_change = Ext::from_f64(0.0);
    f.bond_ops_total = Ext::from_f64(0.0);
    f.extra_pl_adj = Ext::from_f64(0.0);
    f.start_growth = 0;
    f.div_change = 0.0f32;
    f.bond_op_code2 = 0;
    f.bond_op_code3 = 0;
    f.start_div_per_share = g.dividend_per_share[g.sim_co_count];
    f.line_marker = PbStr::from("25510");
    f.ent_id = g.sim_co_count;
    if 85 < g.sim_co_count {
        if get_controller_type(g, f.ent_id) != 1 && !f.skips(0x62e730) {
            super::transactions::liquidate_futures_positions(g, ui, rng, f.ent_id, 3, 0);
        }
        spawn_shortage_corp(g, ui, rng, f);
    }
    let e = f.ent_id;
    if g.entity_status[e] != 1 {
        return;
    }
    f.industry = g.industry_num[e];
    if 10 < e && e < 86 && g.cur_quarter == 1 && g.start_year == g.cur_year {
        f.idx = 1;
        while f.idx <= 15 {
            f.tmp_long = cast::<i64, _>(g.hold_stock_id[(f.idx, e)]);
            if f.tmp_long > cast::<i64, _>(e) {
                f.tmp_pct = cast::<f32, _>(g.hold_stock_pct[(f.idx, e)]);
                let h = cast::<i32, _>(f.tmp_long);
                g.equity_income_ytd[e] = g.earnings_y1[h] * sf(0.25f32) * sf(0.01f32) * cast::<Ext, _>(f.tmp_pct)
                    + g.equity_income_ytd[e];
            }
            f.idx += 1;
        }
    }
    recalc_commod_margin_balances(g, ui, rng, e);
    calc_entity_financials(g, ui, rng, e);
    if find_option_positions(g, ui, e, 0, 0, 0) != 0
        && g.credit_rating_num[e] < 6
        && get_controller_type(g, e) != 1
    {
        f.other_id = find_option_positions(g, ui, e, 0, 0, 0);
        if f.other_id != 41 {
            f.tries = 0;
            while f.other_id != 0 {
                f.other_id = find_option_positions(g, ui, e, 0, 0, 0);
                g.tmp_ext1 = Ext::from_f64(0.0);
                g.tmp_ext2 = Ext::from_f64(0.0);
                close_option_positions(g, ui, rng, e, 0, 0, 0);
                f.tries += 1;
                if 10 < f.tries {
                    break;
                }
            }
            calc_entity_financials(g, ui, rng, e);
        }
    }
    if get_managing_player(g, e) == 0 {
        if g.credit_rating_num[e] < 7 || g.unused_credit_line == Ext::from_f64(0.0) {
            if g.total_assets * sf(0.05f32) > g.t_bills[e] + g.cash[e] || g.industry_num[e] == 1 {
                if !f.skips(0x66b218) {
                    super::asset_sales::ai_offer_assets_for_sale(g, ui, rng, f.time_secs);
                }
            }
        } else if 130 < e && e != 1600 {
            if g.business_assets[e] * sf(0.3f32) < g.t_bills[e] + g.cash[e]
                && g.exchange_rate * Ext::from_f64(500.0) < g.t_bills[e] + g.cash[e]
                && g.liquid_funds > g.total_assets * sf(0.2f32)
            {
                if 69 < g.industry_num[e] {
                    buy_offered_stock(g, ui, rng, f);
                } else {
                    let mut r = 0;
                    if !f.skips(0x66c796) {
                        r = super::asset_sales::ai_buy_offered_business_assets(g, ui, rng);
                    }
                    if r == 0 {
                        buy_offered_stock(g, ui, rng, f);
                    }
                }
            }
        } else if 85 < e && e < 131 {
            if g.exchange_rate * Ext::from_f64(500.0) < g.t_bills[e] + g.cash[e]
                && g.total_assets * sf(0.2f32) < g.t_bills[e] + g.cash[e]
            {
                buy_offered_stock(g, ui, rng, f);
            }
        }
    }
    if g.industry_num[e] == 71 {
        fund_management(g, ui, rng, f);
    } else if g.hold_stock_id[(0, e)] == 15 && !f.skips(0x64c9d1) {
        f.holding_pick = super::etf::get_etf_smallest_holding(g, ui, e);
        if 10 < f.holding_pick && !f.skips(0x4c64ef) {
            super::acquisitions::ai_sell_or_merge_holding(g, ui, rng, e, f.holding_pick);
        }
    }
    f.capital_taxes = calc_capital_taxes(g, e);
    f.carbon_tax = g.carbon_tax_qtr;
    f.oil_windfall_tax = g.oil_windfall_tax_qtr;
    f.holding_co_tax = g.holding_co_tax_due;
    f.corp_shares_tax = g.corp_shares_tax_due;
    f.health_care_tax = g.health_care_tax_qtr;
    g.cash[e] = g.cash[e] - f.capital_taxes;
    calc_entity_financials(g, ui, rng, e);
    update_corp_control(g, ui, rng, e);
    f.public_pct = g.public_pct;
    f.ctrl_type = get_controller_type(g, e);
    f.rating_str = g.credit_rating_str.clone();
    f.credit_rating = g.credit_rating_num[e];
    f.tmp_short = 6;
    if f.ctrl_type == 1 {
        f.tmp_short = 5;
    }
    if (cast::<i32, _>(f.tmp_short)) > f.credit_rating {
        let mut n = 0;
        if !f.skips(0x62e730) {
            n = super::transactions::liquidate_futures_positions(g, ui, rng, e, 2, 1);
        }
        if n > 0 {
            f.msg_part1 = f.rating_str.clone();
            if f.rating_str == "B" {
                f.msg_part1 = PbStr::from("poor");
            }
            g.news_msg = PbStr::from("Due to ")
                + &f.msg_part1
                + __content.string(11040)
                + &g.ent_symbol[e]
                + __content.string(11041);
            post_news(g, ui, e, 0);
        }
    }
    f.total_assets_save = g.total_assets;
    f.managing_player = get_managing_player(g, e);
    f.need_funding = 0.0f32;
    if f.ctrl_type != 1 {
        let c = g.sim_co_count;
        if g.industry_num[c] < 3 {
            if g.net_worth < Ext::from_f64(0.0) {
                f.need_funding = 1.0f32;
            }
            if g.industry_num[c] == 2 && 7 < g.credit_rating_num[c] && g.bonds_issued[c] == 0 {
                if g.bank_debt[c] > g.exchange_rate * Ext::from_f64(10000.0)
                    || g.exchange_rate * Ext::from_f64(10000.0) > g.t_bills[c] + g.cash[c]
                {
                    f.need_funding = 1.0f32;
                }
            }
        } else if g.total_assets > g.exchange_rate * Ext::from_f64(1000000.0)
            && g.exchange_rate * Ext::from_f64(1000.0) > g.t_bills[c] + g.cash[c]
        {
            f.need_funding = 1.0f32;
        }
        if g.industry_num[c] == 70 && f.total_assets_save < g.exchange_rate * Ext::from_f64(5.0) {
            if count_futures_positions(g, c, 0) == 0
                && cancel_entity_swaps(g, ui, rng, c, 2, 2) == 0
                && !f.skips(0x51b258)
            {
                super::liquidation::liquidate_corporation(g, ui, rng, g.sim_co_count, 2, &f.now.clone());
            }
        }
    }
    if f.lbo_done == 0 && !f.skips(0x4d4451) {
        ai_financing(g, ui, rng, f);
    }
    let e = f.ent_id;
    f.idx = cast::<i32, _>(g.wholly_owned_by);
    if g.public_pct == 0 && 10 < g.wholly_owned_by {
        if Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(2.0) {
            if Ext::from_f64(10.0) * rng.rnd() > Ext::from_f64(9.0) || g.tax_loss_carry[e] < -Ext::from_f64(1000.0) * g.exchange_rate {
                if !f.skips(0x4e51b1) {
                    super::ai_corp::capital_infusion(g, ui, rng, f.idx, e);
                }
            }
        }
    }
    if 10 < f.idx && f.idx < 86 {
        if Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(2.0) && f.managing_player == 0 {
            let mut recent = 0;
            if !f.skips(0x498a15) {
                recent = super::ai_corp::is_recent_offering(g, e);
            }
            if recent != 1 && g.credit_rating_num[e] < 8 {
                f.tmp_short = cast::<i16, _>(rng.rnd_range(25, 20));
                let mut go = true;
                if get_controller_type(g, e) == 1 {
                    let mut ok = 0;
                    if !f.skips(0x4be9a8) {
                        ok = super::asset_sales::ask_autopilot_permission(
                            g,
                            ui,
                            rng,
                            e,
                            0,
                            f.tmp_short,
                            0,
                            3,
                            f.time_secs,
                        );
                    }
                    if ok == 0 {
                        go = false;
                    }
                }
                if go {
                    if g.recent_offerings[10] == 0 {
                        g.recent_offerings[10] = e;
                    }
                    g.tmp_ext1 = cast::<Ext, _>(f.tmp_short);
                    if !f.skips(0x5223a3) {
                        super::bonds::issue_stock(g, ui, rng, e, 1);
                    }
                    f.ctrl_type = get_controller_type(g, e);
                    f.managing_player = get_managing_player(g, e);
                    g.tmp_total_debt = f.total_assets_save;
                    g.tmp_bank_loan = g.bank_debt[e];
                }
            }
        }
    }
    if 10 < f.idx {
        let p = f.idx;
        if 2 < g.industry_num[p]
            && g.industry_num[p] == g.industry_num[e]
            && f.managing_player == 0
            && g.industry_num[e] < 70
            && g.tax_loss_carry[e] < -Ext::from_f64(500.0) * g.exchange_rate
            && !(g.tax_loss_carry[p] < Ext::from_f64(0.0))
            && (g.cur_quarter + p) % 4 == 3
        {
            f.asset_amt = sf(0.65f32) * g.business_assets[p];
            if f.asset_amt > g.exchange_rate * Ext::from_f64(300.0)
                && g.business_assets[e] > Ext::from_f64(50.0)
                && g.business_assets[p] > g.business_assets[e] * sf(0.4f32)
            {
                f.child_assets = g.business_assets[e];
                f.combined_assets = f.asset_amt + f.child_assets;
                f.cap_gain_share =
                    Ext::from_f64(100.0) * (f.child_assets / f.combined_assets) * cast::<Ext, _>(g.profit_margin[e]);
                f.tax_payment =
                    Ext::from_f64(100.0) * (f.asset_amt / f.combined_assets) * cast::<Ext, _>(g.profit_margin[p]);
                g.profit_margin[e] = cast::<i16, _>(to_quad((f.cap_gain_share + f.tax_payment) / Ext::from_f64(100.0)));
                f.other_id = find_holding_slot(g, p, e);
                g.hold_stock_basis[(f.other_id, p)] =
                    f.asset_amt + g.hold_stock_basis[(f.other_id, p)];
                g.market_cap[e] = f.asset_amt * sf(0.004f32) + g.market_cap[e];
                g.business_assets[e] = f.asset_amt + g.business_assets[e];
                g.business_assets[p] = g.business_assets[p] - f.asset_amt;
                g.news_msg = g.ent_symbol[p].clone()
                    + __content.string(11042)
                    + &g.ent_symbol[e];
                post_news(g, ui, 0, 0);
            }
        }
    }
    if (cast::<i32, _>(g.strike_corp_code)) % 10000 == g.sim_co_count && g.entity_status[e] == 1 {
        if !g.news_msg.is_empty() {
            post_news(g, ui, 0, 0);
        }
        f.msg_part1 = PbStr::new();
        f.msg_part2 = PbStr::new();
        if g.strike_corp_code < 10000 {
            g.strike_corp_code = 0;
            if g.industry_num[g.sim_co_count] < 70 {
                g.profit_margin[e] = g.profit_margin[e].wrapping_add(10);
                if Ext::from_f64(3.0) * rng.rnd() < Ext::from_f64(1.0) {
                    f.msg_part1 = PbStr::from(" costly");
                }
                if g.rand_seed % 3 == 1 {
                    f.msg_part1 = PbStr::from(__content.string(11043));
                }
                if f.msg_part1.is_empty() {
                    f.msg_part2 = PbStr::from(__content.string(11044));
                }
                g.news_msg = g.ent_symbol[e].clone()
                    + " and"
                    + &f.msg_part2
                    + __content.string(11045)
                    + &f.msg_part1
                    + __content.string(11046);
            }
        } else if g.industry_num[e] < 70 {
            g.profit_margin[e] = g.profit_margin[e].wrapping_sub(3);
            if g.rand_seed % 3 == 1 && len(f.name_and_symbol.as_bytes()) < 22 {
                f.msg_part1 = PbStr::from(__content.string(11047));
            }
            g.news_msg = PbStr::from("Long")
                + &f.msg_part1
                + __content.string(11048)
                + &f.name_and_symbol
                + __content.string(11049);
            if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
                g.news_msg =
                    PbStr::from(__content.string(11050)) + &f.name_and_symbol + __content.string(11046);
            }
        } else if g.industry_num[e] == 70 {
            g.strike_corp_code = 0;
        }
        post_news(g, ui, e, 0);
    }
    if 10000 < g.strike_corp_code && g.sim_co_count == e {
        g.strike_corp_code = g.strike_corp_code.wrapping_sub(10000);
    }
    if 1 < g.industry_num[e] && !f.skips(0x4d5ae2) {
        pay_bond_interest(g, ui, rng, f);
    }
    let e = f.ent_id;
    if 3 <= g.industry_num[e] && g.industry_num[e] <= 69 {
        if g.business_assets[e] < g.exchange_rate * Ext::from_f64(25.0) {
            f.assets_sold = crate::pb::numeric::double(Ext::from_f64(0.0));
            f.margin_frac = 0.3f32;
            withdraw_from_industry(g, ui, rng, f);
            f.margin_frac = 0.0f32;
            return;
        }
        if !f.skips(0x4cbd9e) {
            adjust_disc_spending(g, ui, rng, f);
        }
        if !f.skips(0x4cc112) {
            disc_spending_effects(g, ui, rng, f);
        }
        if !f.skips(0x4cc8d1) {
            growth_and_spending_news(g, ui, rng, f);
        }
        return;
    }
    f.margin_frac = 0.0f32;
}

/// `AiBuyOfferedStock()` followed by `CalcEntityFinancials(entId)`.
fn buy_offered_stock(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    if !f.skips(0x66d13a) {
        super::asset_sales::ai_buy_offered_stock(g, ui, rng, f.time_secs);
    }
    calc_entity_financials(g, ui, rng, f.ent_id);
}

/// The fund block of SimEntity_OperatingResults (industry 71): the advisory fee (0.25% of the fee
/// percentage of net worth per quarter) paid to the adviser or a new adviser assigned, then the AI's
/// put writing, holding sales and covered calls for funds without a human adviser.
fn fund_management(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    let e = f.ent_id;
    f.amount = g.total_assets * sf(0.001f32);
    if f.amount < Ext::from_f64(0.0) {
        f.amount = Ext::from_f64(0.0);
    }
    f.other_id = g.fund_adviser[e];
    let a = f.other_id;
    if a < 86
        || g.entity_status[a] != 1
        || sf(0.19f32) > cast::<Ext, _>(g.fund_fee_pct[e])
        || sf(1.01f32) < cast::<Ext, _>(g.fund_fee_pct[e])
    {
        assign_etf_adviser(g, ui, rng, e, 0, 0);
    } else if g.industry_num[a] == 37 || g.industry_num[a] == 2 {
        f.amount = g.net_worth * (sf(0.0025f32) * cast::<Ext, _>(g.fund_fee_pct[e]));
        g.cash[a] = f.amount + g.cash[a];
        g.accrued_pl[a] = f.amount + g.accrued_pl[a];
        if e == g.sel_entity && f.amount > g.exchange_rate * Ext::from_f64(10.0) {
            g.news_msg = g.ent_symbol[e].clone()
                + __content.string(11051)
                + &g.ent_symbol[a]
                + " is "
                + &trim(g.currency_symbol.as_bytes())
                + &fmt_trim(f.amount, b"* ###,.00")
                + " "
                + &g.million_prefix_uc;
            post_news(g, ui, e, a);
        }
    } else {
        assign_etf_adviser(g, ui, rng, e, 0, 0);
    }
    g.cash[e] = g.cash[e] - f.amount;
    g.accrued_pl[e] = g.accrued_pl[e] - f.amount;
    calc_entity_financials(g, ui, rng, e);
    if get_etf_adviser_human_status(g, ui, rng, e) > 0 || e < 1586 {
        return;
    }
    if g.hold_stock_id[(0, e)] < 15 {
        f.holding_pick = 0;
        if !f.skips(0x649382) {
            f.holding_pick =
                crate::logic::newgame_helpers::pick_etf_holding_stock(g, ui, rng, e, 0);
        }
        if 10 < f.holding_pick && find_option_positions(g, ui, e, 0, 0, 0) == 0 {
            let p = f.holding_pick;
            if g.market_cap[p] * Ext::from_f64(5.0) > (g.t_bills[e] + g.cash[e]) * sf(0.25f32)
                && g.market_cap[p] * Ext::from_f64(5.0) < g.t_bills[e] + g.cash[e]
            {
                if !f.skips(0x5fe20b) {
                    super::etf::ai_corp_sell_naked_puts(g, ui, rng, e, f.holding_pick);
                }
            }
        }
    } else if g.covered_call_fund_id != e {
        f.holding_pick = 0;
        if !f.skips(0x64c9d1) {
            f.holding_pick = super::etf::get_etf_smallest_holding(g, ui, e);
        }
        if 10 < f.holding_pick && !f.skips(0x4c64ef) {
            super::acquisitions::ai_sell_or_merge_holding(g, ui, rng, e, f.holding_pick);
        }
    }
    if g.covered_call_fund_id == e && !f.skips(0x603825) {
        super::etf::etf_write_covered_calls(g, ui, rng);
    }
}

/// GOSUB SimEntity_TrimMgmtChangeLists (0x4dfc3c): drops the oldest entry of `RecentMgmtChangeList`
/// (Q2, or when the list is long) or runs `CheckMgmtChange` on pending changes.
pub(super) fn trim_mgmt_change_lists(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    f: &mut Frame,
) {
    let __content = crate::content::current();
    if f.skips(0x4dfc3c) {
        return;
    }
    f.line_marker = PbStr::from("35000");
    f.tmp_long = cast::<i64, _>(len(g.recent_mgmt_change_list.as_bytes()));
    if !(Ext::from_f64(6.0) > cast::<Ext, _>(f.tmp_long)) {
        f.msg_part1 = right(
            g.recent_mgmt_change_list.as_bytes(),
            cast::<i32, _>(to_quad(cast::<Ext, _>(f.tmp_long) - Ext::from_f64(5.0))),
        );
    } else {
        f.msg_part1 = PbStr::from(__content.string(11052));
    }
    if g.cur_quarter == 2 {
        g.recent_mgmt_change_list = f.msg_part1.clone();
        return;
    }
    f.cmp_tmp = 5;
    if f.cmp_tmp <= len(g.new_mgmt_list.as_bytes()) {
        if !f.skips(0x4ef5eb) {
            super::mgmt::check_mgmt_change(g, ui, rng);
        }
        return;
    }
    g.new_mgmt_list = PbStr::new();
    f.cmp_tmp = 25;
    if f.cmp_tmp < len(g.recent_mgmt_change_list.as_bytes()) {
        g.recent_mgmt_change_list = f.msg_part1.clone();
    }
}

/// GOSUB SimEntity_SpawnShortageCorp (0x4d4232): when entity `entId` has left its industry (70), starts
/// a new corp (or expansion) in the shortage industry — less often the more corps are active.
pub(super) fn spawn_shortage_corp(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    if f.skips(0x4d4232) {
        return;
    }
    f.line_marker = PbStr::from("28400");
    let e = f.ent_id;
    if g.industry_num[e] == 71 {
        return;
    }
    f.industry = g.industry_num[e];
    if f.industry != 70 {
        return;
    }
    if g.entity_status[e] == 1 && get_controller_type(g, e) == 1 {
        return;
    }
    let k = (e + g.rand_seed) % 16;
    if 1000 < g.active_corp_count {
        if 1300 < g.active_corp_count && 1 < k {
            return;
        }
        if 1200 < g.active_corp_count && 2 < k {
            return;
        }
        if 1100 < g.active_corp_count && 3 < k {
            return;
        }
        if 1000 < g.active_corp_count && 5 < k {
            return;
        }
    }
    f.industry = g.shortage_industry;
    if g.shortage_industry < 3 || 69 < g.shortage_industry {
        return;
    }
    g.tmp_ext1 = Ext::from_f64(0.0);
    f.slot = 0;
    if !f.skips(0x4f21e7) {
        f.slot =
            super::asset_sales::start_new_corp_or_expand(g, ui, rng, f.industry, 0, f.time_secs);
    }
    if 130 < f.slot {
        g.shortage_industry = 0;
    }
}

/// GOSUB SimEntity_AdjustDiscSpending (0x4cbd9e): the AI's discretionary (R&D/marketing) spending
/// reacts to low margins; extreme margins are pulled toward 0.
pub(super) fn adjust_disc_spending(
    g: &mut Globals,
    _ui: &mut dyn Ui,
    rng: &mut Rng,
    f: &mut Frame,
) {
    f.line_marker = PbStr::from("25600");
    let e = f.ent_id;
    if g.profit_margin[e] < -9 {
        g.profit_margin[e] = g.profit_margin[e].wrapping_add(2);
    }
    if f.managing_player == 0 && g.profit_margin[e] < 5 {
        let mut done = false;
        if g.business_assets[e] * sf(0.25f32) < g.t_bills[e] + g.cash[e]
            && (cast::<i32, _>(g.disc_spend_pct[e])) < g.rand_seed % 25
        {
            g.disc_spend_pct[e] = (cast::<i16, _>(g.rand_seed % 6)).wrapping_add(g.disc_spend_pct[e]);
            done = true;
        }
        if !done
            && (cast::<i32, _>(g.disc_spend_pct[e])) < g.rand_seed % 10
            && 0 < instr_any(1, f.rating_str.as_bytes(), b"AB")
        {
            g.disc_spend_pct[e] = cast::<i16, _>(to_quad(Ext::from_f64(5.0) * rng.rnd() + cast::<Ext, _>(g.disc_spend_pct[e])));
        }
    }
    if Ext::from_f64(20.0) < (cast::<Ext, _>(g.profit_margin[e])).abs() {
        g.profit_margin[e] = cast::<i16, _>(to_quad(sf(0.9f32) * cast::<Ext, _>(g.profit_margin[e])));
        if f.managing_player == 0 && 3 < g.disc_spend_pct[e] && 15 < g.profit_margin[e] {
            g.disc_spend_pct[e] = g.disc_spend_pct[e].wrapping_sub(3);
        }
    }
}

/// GOSUB SimEntity_DiscSpendingEffects (0x4cc112): the margin effect of the discretionary spending
/// (`DiscSpendPct`); a big hit (amount > 15) lifts the market cap 35% with a breakthrough or marketing
/// headline. Also used by the bank and insurer blocks.
pub(super) fn disc_spending_effects(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    f: &mut Frame,
) {
    let __content = crate::content::current();
    let e = f.ent_id;
    f.amount = Ext::from_f64(0.0);
    f.amt_long = cast::<i64, _>(g.disc_spend_pct[e]);
    f.sel_case = cast::<Ext, _>(f.amt_long);
    match f.amt_long {
        0 | 1 => g.profit_margin[e] = g.profit_margin[e].wrapping_sub(1),
        2 | 3 => {
            if g.rand_seed % 2 == 1 && cast::<Ext, _>(f.amt_long) == cast::<Ext, _>(g.cur_quarter) {
                g.profit_margin[e] = g.profit_margin[e].wrapping_add(1);
            } else {
                g.profit_margin[e] = g.profit_margin[e].wrapping_sub(1);
            }
        }
        4 | 5 => {
            if g.cur_year % 4 + g.cur_quarter == 3 {
                g.profit_margin[e] = g.profit_margin[e].wrapping_add(1);
            }
        }
        x if x > 5 => {
            if cast::<Ext, _>(f.amt_long) > Ext::from_f64(15.0) * rng.rnd() + Ext::from_f64(10.0) {
                g.profit_margin[e] = g.profit_margin[e].wrapping_add(3);
            } else {
                g.profit_margin[e] = g.profit_margin[e].wrapping_add(1);
            }
            f.cmp_tmp = g.cur_quarter;
            if rng.rnd_range(6, 1) == f.cmp_tmp {
                f.amount = cast::<Ext, _>(f.amt_long) / Ext::from_f64(2.0);
                g.rand_seed += 1;
                if g.rand_seed % 6 == 1 {
                    f.amt_long = 3;
                }
                f.amount = f.amount * (sf(2.5f32) * rng.rnd());
                f.amount = sf(0.7f32) + f.amount;
                if f.amt_long < 5 && f.amount > Ext::from_f64(2.0) {
                    f.amount = Ext::from_f64(2.0);
                }
                if f.amt_long < 3 {
                    f.amount = f.amount - Ext::from_f64(1.0);
                }
                if f.amount < Ext::from_f64(0.0) {
                    f.amount = Ext::from_f64(0.0);
                }
                if g.industry_num[e] == 2 && f.amount > Ext::from_f64(5.0) {
                    f.amount = sf(0.5f32) * f.amount;
                }
                g.profit_margin[e] = cast::<i16, _>(to_quad(f.amount + cast::<Ext, _>(g.profit_margin[e])));
            }
        }
        _ => {}
    }
    if !(f.amount > Ext::from_f64(15.0)) {
        return;
    }
    g.market_cap[e] = sf(1.35f32) * g.market_cap[e];
    f.idx = get_industry_category(g.industry_num[e]);
    if f.idx == 1 {
        f.msg_part1 = PbStr::from(__content.string(11053));
        if 2 < g.rand_seed % 5 {
            f.msg_part1 = PbStr::from(__content.string(11054));
        }
        f.msg_part2 = PbStr::from(__content.string(11055));
        if 4 < g.rand_seed % 7 {
            f.msg_part2 = PbStr::from(__content.string(11056));
        }
        g.news_msg = g.ent_symbol[e].clone() + &f.msg_part2 + "es " + &f.msg_part1;
        f.msg_part3 = PbStr::from(__content.string(11057));
        if 1 < g.rand_seed % 4 {
            f.msg_part3 = PbStr::from(__content.string(11058));
        }
        let ind = g.industry_num[e];
        if ind == 7 || ind == 9 || ind == 50 {
            f.msg_part3 = PbStr::from(__content.string(11059));
        }
        g.news_msg = g.news_msg.clone() + &f.msg_part3;
    } else if f.idx == 2 {
        f.msg_part1 = PbStr::from(__content.string(11060));
        if g.industry_num[e] == 1 {
            f.msg_part1 = PbStr::from(__content.string(11061));
        }
        if g.industry_num[e] == 2 {
            f.msg_part1 = PbStr::from(__content.string(11062));
        }
        g.news_msg = g.ent_symbol[e].clone() + " " + &f.msg_part1;
        if 0 < is_in_recent_headlines(g, __content.bytes(11063))
            && find_stock_holding(g, g.cur_player, e) == 0
        {
            g.news_msg = PbStr::new();
        }
    }
    post_news(g, ui, e, 0);
}

/// GOSUB SimEntity_GrowthAndSpendingNews (0x4cc8d1): industry outlook and market share, the AI's growth
/// and spending decisions, antitrust and takeover candidates, the quarter's margin (`marginFrac`,
/// `marginText`, `netMarginFrac`) and the miscellaneous operating income booked to cash.
pub(super) fn growth_and_spending_news(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    f: &mut Frame,
) {
    let __content = crate::content::current();
    f.line_marker = PbStr::from("25800");
    let e = f.ent_id;
    f.start_growth = g.growth_rate[e];
    f.industry = g.industry_num[e];
    calc_industry_outlook(g, rng, f.industry);
    f.ind_outlook = cast::<f32, _>(g.tmp_ext1);
    f.ind_outlook2 = crate::pb::numeric::double(g.tmp_ext2);
    if Ext::from_f64(20.0) < cast::<Ext, _>(f.ind_outlook) {
        if Ext::from_f64(5.0) * rng.rnd() < Ext::from_f64(1.0) {
            let ind = cast::<i32, _>(g.industry_num[e]);
            if g.ind_growth_forecast[ind] > g.ind_growth_rate[ind]
                && cast::<i32, _>(f.industry) == g.crowded_industry
            {
                g.ind_growth_forecast[ind] = g.ind_growth_forecast[ind].wrapping_sub(1);
            }
        }
        f.need_funding = 1.0f32;
    }
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_ext2 = Ext::from_f64(0.0);
    g.tmp_ext3 = Ext::from_f64(0.0);
    f.market_share = to_integer(Ext::from_f64(100.0) * (g.business_assets[e] / g.ind_total_assets));
    if f.managing_player == 0
        && Ext::from_f64(15.0) * rng.rnd() + Ext::from_f64(40.0) < cast::<Ext, _>(f.market_share)
        && -5 < g.growth_rate[e]
    {
        g.growth_rate[e] = g.growth_rate[e].wrapping_sub(10);
        if Ext::from_f64(5.0) > cast::<Ext, _>(f.ind_outlook) {
            g.growth_rate[e] = g.growth_rate[e].wrapping_sub(10);
        }
        if 48 < f.market_share {
            g.growth_rate[e] = -10;
        }
    }
    f.tmp_pct = cast::<f32, _>(cast::<Ext, _>(f.market_share) * sf(0.2f32));
    if g.rand_seed % 10 + 50 < cast::<i32, _>(f.market_share) && g.antitrust_defendant == 0 {
        g.antitrust_defendant = e;
    }
    f.tmp_pct = cast::<f32, _>(cast::<Ext, _>(f.tmp_pct) - Ext::from_f64(2.0));
    if Ext::from_f64(8.0) < cast::<Ext, _>(f.tmp_pct) {
        f.tmp_pct = 5.0f32;
    }
    g.rand_seed += 5;
    random_margin_drift(g, ui, rng, f);
    f.base_margin =
        to_long((cast::<Ext, _>(g.profit_margin[e]) + cast::<Ext, _>(f.tmp_pct)) - cast::<Ext, _>(g.disc_spend_pct[e]));
    f.disc_spend = g.disc_spend_pct[e];
    f.margin_frac = cast::<f32, _>(cast::<Ext, _>(f.base_margin) + cast::<Ext, _>(f.ind_outlook));
    if Ext::from_f64(65.0) < cast::<Ext, _>(f.margin_frac) {
        f.margin_frac = cast::<f32, _>(Ext::from_f64(65.0) - Ext::from_f64(5.0) * rng.rnd());
    }
    if -Ext::from_f64(35.0) > cast::<Ext, _>(f.margin_frac) {
        f.margin_frac = cast::<f32, _>(-Ext::from_f64(35.0) + Ext::from_f64(5.0) * rng.rnd());
    }
    if f.margin_text.is_empty() {
        f.margin_text = fmt_trim(cast::<Ext, _>(f.margin_frac), b"####.0");
    }
    f.margin_frac = cast::<f32, _>(sf(0.01f32) * cast::<Ext, _>(f.margin_frac));
    if sf(0.4f32) < cast::<Ext, _>(f.margin_frac) && g.market_cap[e] < Ext::from_f64(95.0) && g.growth_rate[e] < 10 {
        g.takeover_candidate_id = e;
    }
    f.net_margin_frac = cast::<f32, _>(cast::<Ext, _>(f.base_margin) + f.ind_outlook2);
    f.net_margin_frac = cast::<f32, _>(sf(0.01f32) * cast::<Ext, _>(f.net_margin_frac));
    f.misc_income =
        cast::<f32, _>(val(f.margin_text.as_bytes()) * g.business_assets[e] * sf(0.01f32) * sf(0.05f32));
    if f.managing_player == 0 && Ext::from_f64(25.0) * rng.rnd() + Ext::from_f64(30.0) > cast::<Ext, _>(f.market_share) {
        if g.disc_spend_pct[e] < 20 && sf(0.3f32) < cast::<Ext, _>(f.margin_frac) && 5 < g.credit_rating_num[e]
        {
            g.disc_spend_pct[e] = g.disc_spend_pct[e].wrapping_add(6);
            if g.rand_seed % 11 == 3 || g.cur_player == g.corp_controller[e] {
                f.idx = get_industry_category(g.industry_num[e]);
                f.msg_part1 = PbStr::from("R&D");
                if f.idx == 2 {
                    f.msg_part1 = PbStr::from(__content.string(11064));
                }
                f.msg_part2 = PbStr::from(__content.string(11065));
                if g.rand_seed % 3 == 1 {
                    f.msg_part2 = PbStr::from(__content.string(2841));
                }
                if g.rand_seed % 3 == 2 {
                    f.msg_part2 = PbStr::from(__content.string(11066));
                }
                g.news_msg =
                    g.ent_symbol[e].clone() + &f.msg_part2 + __content.string(11067) + &f.msg_part1;
                if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
                    g.news_msg = replace(g.news_msg.as_bytes(), b"outlays", __content.bytes(7437));
                }
            }
        } else if 5 < g.disc_spend_pct[e]
            && sf(0.12f32) > cast::<Ext, _>(f.margin_frac)
            && 6 > g.credit_rating_num[e]
            && 10 < g.profit_margin[e]
        {
            g.disc_spend_pct[e] = cast::<i16, _>(to_quad(cast::<Ext, _>(g.disc_spend_pct[e]) - Ext::from_f64(5.0) * rng.rnd()));
        }
    }
    if g.growth_rate[e] < -10 {
        g.growth_rate[e] = -10;
    }
    if 60 < g.growth_rate[e] {
        g.growth_rate[e] = 55;
    }
    if f.managing_player == 0 && !(g.monetary_stance < 0) {
        if Ext::from_f64(6.0) * rng.rnd() + -Ext::from_f64(7.0) > cast::<Ext, _>(g.profit_margin[e])
            && sf(0.12f32) > cast::<Ext, _>(f.margin_frac)
            && 2 < g.credit_rating_num[e]
            && g.rand_seed % 3 == 1
            && g.disc_spend_pct[e] < 10
        {
            g.disc_spend_pct[e] = cast::<i16, _>(to_quad(Ext::from_f64(17.0) * rng.rnd() + cast::<Ext, _>(g.disc_spend_pct[e])));
        }
    }
    g.growth_rate[e] = g.growth_rate[e].clamp(-10, 60);
    g.accrued_pl[e] = cast::<Ext, _>(f.misc_income) + g.accrued_pl[e];
    g.cash[e] = cast::<Ext, _>(f.misc_income) + g.cash[e];
    if f.ctrl_type == 1 && get_managing_player(g, e) == 0 {
        let c = g.corp_controller[e];
        if 6 > c && g.growth_rate[e] > g.player_growth_throttle[c] {
            g.growth_rate[e] = g.player_growth_throttle[c];
        }
    }
}

/// GOSUB SimEntity_RandomMarginDrift (0x4d81d8): profit margin moves by -4..+4, damped at the extremes;
/// the hot stock gets +6 (and the AI stops its discretionary spending).
pub(super) fn random_margin_drift(g: &mut Globals, _ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    if f.skips(0x4d81d8) {
        return;
    }
    f.line_marker = PbStr::from("29950");
    let e = f.ent_id;
    f.tmp_long = to_quad(int(Ext::from_f64(9.0) * rng.rnd()) - Ext::from_f64(4.0));
    g.profit_margin[e] = cast::<i16, _>(to_quad(cast::<Ext, _>(f.tmp_long) + cast::<Ext, _>(g.profit_margin[e])));
    if 25 < g.profit_margin[e] {
        g.profit_margin[e] = g.profit_margin[e].wrapping_sub(7);
    }
    if g.profit_margin[e] < -12 {
        g.profit_margin[e] = g.profit_margin[e].wrapping_add(2);
    }
    if cast::<i32, _>(g.hot_stock_corp) == e && g.profit_margin[e] < 22 {
        g.profit_margin[e] = g.profit_margin[e].wrapping_add(6);
        if f.managing_player == 0 {
            g.disc_spend_pct[e] = 0;
        }
    }
}

/// GOSUB SimEntity_WithdrawFromIndustry (0x4d3e0b): business assets cover a cash deficit; if cash stays
/// negative or the assets drop below 25 (× exchange rate) the corp leaves its industry (becomes 70).
pub(super) fn withdraw_from_industry(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    f: &mut Frame,
) {
    let __content = crate::content::current();
    if f.skips(0x4d3e0b) {
        return;
    }
    f.line_marker = PbStr::from("28220");
    let e = f.ent_id;
    if g.cash[e].abs() >= g.business_assets[e] {
        g.cash[e] = g.business_assets[e] + g.cash[e];
        g.business_assets[e] = Ext::from_f64(0.0);
    } else {
        g.business_assets[e] = g.cash[e] + g.business_assets[e];
        g.cash[e] = Ext::from_f64(0.0);
    }
    if !(g.cash[e] < Ext::from_f64(0.0)) && !(g.business_assets[e] < g.exchange_rate * Ext::from_f64(25.0)) {
        return;
    }
    if g.business_assets[e] > Ext::from_f64(0.0) {
        g.cash[e] = g.business_assets[e] + g.cash[e];
    }
    g.cash[e] = g.working_capital[e] + g.cash[e];
    g.working_capital[e] = Ext::from_f64(0.0);
    let ind = cast::<i32, _>(g.industry_num[e]);
    g.news_msg = g.ent_symbol[e].clone()
        + __content.string(11068)
        + &mcase(g.ind_name[ind].as_bytes(), CaseMode::Ansi)
        + __content.string(11069);
    g.disc_spend_pct[e] = 0;
    g.business_assets[e] = Ext::from_f64(0.0);
    if g.industry_num[e] == 37 && !f.skips(0x64e6ba) {
        super::transactions::etf_drop_one_adviser_contract(g, ui, rng, e, 0);
    }
    g.industry_num[e] = 70;
    g.profit_margin[e] = 0;
}

/// GOSUB SimEntity_AiFinancing (0x4d4451): the AI decides whether the entity needs financing
/// (`financingMode` 1 = stock, 2 = bonds) and issues stock or bonds; after a bond issue, AI-controlled
/// banks and insurers may buy part of it.
pub(super) fn ai_financing(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    f.line_marker = PbStr::from("29700");
    let e = f.ent_id;
    if g.entity_status[e] != 1 || 0 < f.managing_player || g.industry_num[e] == 71 {
        return;
    }
    f.financing_mode = 0;
    if g.credit_rating_num[e] < 2 {
        return;
    }
    g.rand_seed += 1;
    if g.rand_seed % 7 < 3 && f.need_funding == Ext::from_f32(0.0f32) {
        return;
    }
    let ind = g.industry_num[e];
    if 2 < ind && ind < 70 && !(g.cash[e] > g.exchange_rate * Ext::from_f64(50.0)) && 20 < g.growth_rate[e] {
        if 2 < g.credit_rating_num[e] {
            f.financing_mode = 1;
        }
        if g.earnings_y1[e] < Ext::from_f64(0.0) || g.earnings_ytd[e] < Ext::from_f64(0.0) || g.market_cap[e] > Ext::from_f64(175.0) {
            f.financing_mode = 0;
        }
        if Ext::from_f64(3.0) * rng.rnd() < Ext::from_f64(2.0) {
            f.financing_mode = 0;
        }
    }
    if f.need_funding == Ext::from_f32(1.0f32) {
        f.financing_mode = 1;
        if g.industry_num[e] < 3 && g.bonds_issued[e] > 0 {
            f.financing_mode = 2;
        }
    }
    if g.industry_num[e] == 70
        && 2 < g.credit_rating_num[e]
        && g.credit_rating_num[e] < 8
        && g.cash[e] < g.exchange_rate * Ext::from_f64(200.0)
    {
        f.financing_mode = 1;
    }
    if f.financing_mode == 0 {
        'decide: {
            if g.cur_quarter == 2
                && g.rand_seed % 51 == 7
                && sf(6.5f32) > cast::<Ext, _>(g.base_interest_rate)
                && 0 < instr(1, f.rating_str.as_bytes(), b"A")
            {
                f.financing_mode = 1;
                break 'decide;
            }
            if g.industry_num[e] == 1
                && g.bank_debt[e] > Ext::from_f64(1000.0)
                && g.tmp_bank_total_loans > g.total_assets * sf(0.9f32)
            {
                f.financing_mode = 1;
                if Ext::from_f64(2.0) * rng.rnd() + sf(9.5f32) < cast::<Ext, _>(g.base_interest_rate) {
                    f.financing_mode = 2;
                }
                if Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(1.0) || g.credit_rating_num[e] < 3 {
                    f.financing_mode = 0;
                }
                break 'decide;
            }
            if g.industry_num[e] == 2 {
                if g.net_worth > Ext::from_f64(0.0) && !(g.cash[e] > Ext::from_f64(0.0)) && g.bonds_issued[e] == 0 {
                    f.financing_mode = 1;
                }
                if !(g.net_worth >= g.total_assets * sf(0.07f32)) {
                    f.financing_mode = 2;
                }
                break 'decide;
            }
            if g.industry_num[e] < 3 || 69 < g.industry_num[e] {
                return;
            }
            if instr(1, f.rating_str.as_bytes(), b"B") == 0 && f.need_funding == Ext::from_f32(0.0f32) {
                return;
            }
            if g.rand_seed % 37 != 3 {
                return;
            }
            let liquid = g.t_bills[e] + g.cash[e];
            if !(g.business_assets[e] * sf(0.15f32) >= liquid) {
                return;
            }
            if !(g.tmp_total_debt * sf(0.35f32) >= liquid) && !(g.total_assets * sf(0.3f32) >= liquid) {
                return;
            }
            f.financing_mode = 1;
        }
    }
    if f.financing_mode == 0 {
        return;
    }
    f.line_marker = PbStr::from("29710");
    if g.bonds_issued[e] > 0 || f.financing_mode == 2 {
        // stock issue
        f.financing_mode = 1;
        if sf(3.5f32) * rng.rnd() + Ext::from_f64(7.0) < cast::<Ext, _>(g.base_interest_rate) {
            return;
        }
        if sf(1.5f32) * rng.rnd() - Ext::from_f64(2.0) > cast::<Ext, _>(g.gdp_growth) {
            return;
        }
        f.amt_long = to_quad(Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(5.0));
        if g.industry_num[e] == 69 {
            f.amt_long = to_quad(sf(1.5f32) * cast::<Ext, _>(f.amt_long));
        }
        f.amount = g.market_cap[e] * cast::<Ext, _>(f.amt_long);
        let ind = g.industry_num[e];
        if 2 < ind && ind < 69 {
            if f.amount > cast::<Ext, _>(g.max_stock_offering_amt) * sf(0.25f32) {
                f.amount = Ext::from_f64(0.0);
            }
        } else {
            if f.amount > cast::<Ext, _>(g.max_stock_offering_amt) * sf(0.3f32) {
                f.amt_long = 5;
            }
            f.amount = g.market_cap[e] * cast::<Ext, _>(f.amt_long);
        }
        if !(f.amount >= Ext::from_f64(50.0)) {
            return;
        }
        if g.industry_num[e] == 1 {
            g.bank_c_ds[e] = f.amount * sf(0.9f32) + g.bank_c_ds[e];
            g.bank_demand_deposits[e] = f.amount * sf(0.1f32) + g.bank_demand_deposits[e];
            g.cash[e] = f.amount + g.cash[e];
        }
        if sf(0.78f32) > cast::<Ext, _>(g.market_valuation) {
            return;
        }
        if g.earnings_yr3_ago[e] == Ext::from_f64(0.0) {
            return;
        }
        let ind = g.industry_num[e];
        if 2 < ind && ind < 70 {
            if g.business_assets[e] * sf(0.7f32) < g.t_bills[e] + g.cash[e] {
                return;
            }
            if !(7 >= g.credit_rating_num[e]) {
                return;
            }
        }
        f.idx = 0;
        if !f.skips(0x498a15) {
            f.idx = super::ai_corp::is_recent_offering(g, e);
        }
        if f.idx == 1 {
            return;
        }
        f.tmp_short = to_integer(cast::<Ext, _>(f.amt_long));
        if get_controller_type(g, e) == 1 {
            let mut ok = 0;
            if !f.skips(0x4be9a8) {
                ok = super::asset_sales::ask_autopilot_permission(
                    g,
                    ui,
                    rng,
                    e,
                    0,
                    f.tmp_short,
                    0,
                    3,
                    f.time_secs,
                );
            }
            if ok == 0 {
                return;
            }
        }
        f.idx = rng.rnd_range(10, 1);
        if g.recent_offerings[f.idx] == 0 {
            g.recent_offerings[f.idx] = e;
        }
        g.tmp_ext1 = cast::<Ext, _>(f.amt_long);
        if !f.skips(0x5223a3) {
            super::bonds::issue_stock(g, ui, rng, e, 1);
        }
        if !g.news_msg.is_empty() {
            post_news(g, ui, 0, 0);
        }
    } else {
        // bond issue
        if sf(2.5f32) * rng.rnd() + Ext::from_f64(9.0) < cast::<Ext, _>(g.base_interest_rate)
            || g.net_worth < Ext::from_f64(100.0)
            || g.ai_target_corp == e
        {
            return;
        }
        f.amount = g.net_worth * sf(0.35f32);
        if g.total_assets > g.exchange_rate * Ext::from_f64(200000.0) {
            f.amount = g.net_worth * sf(0.2f32);
        }
        if g.total_assets > g.exchange_rate * Ext::from_f64(500000.0) {
            f.amount = g.net_worth * sf(0.1f32);
        }
        let ind = g.industry_num[e];
        if ind < 3 || 68 < ind || f.need_funding == Ext::from_f32(1.0f32) {
            if f.amount < g.exchange_rate * Ext::from_f64(5000.0) {
                f.amount = Ext::from_f64(2.0) * f.amount;
            }
        }
        let floor = g.exchange_rate * cast::<Ext, _>(rng.rnd_range(200, 125));
        if f.amount < floor || !(f.amount <= cast::<Ext, _>(g.max_bond_issue)) {
            return;
        }
        f.tmp_long = to_quad(f.amount * sf(0.01f32));
        f.amt_long = to_quad(cast::<Ext, _>(f.tmp_long) * Ext::from_f64(100.0));
        g.tmp_ext1 = cast::<Ext, _>(f.amt_long);
        if !f.skips(0x5231bc) {
            super::bonds::issue_bonds(g, ui, rng, e, Ext::from_f64(0.0), 0);
        }
        f.financing_mode = 2;
    }
    calc_entity_financials(g, ui, rng, e);
    f.rating_str = g.credit_rating_str.clone();
    f.total_assets_save = g.total_assets;
    g.tmp_bank_loan = g.bank_debt[e];
    if f.financing_mode < 2 {
        return;
    }
    if 6 < g.credit_rating_num[e] {
        f.financing_mode = 3;
    }
    f.amount = cast::<Ext, _>(g.bonds_issued[e]) * sf(0.2f32);
    if !(f.amount >= Ext::from_f64(50.0) * rng.rnd() + Ext::from_f64(100.0)) {
        return;
    }
    if f.financing_mode == 3 {
        f.amt_long = to_quad(f.amount * (sf(0.5f32) * rng.rnd() + sf(0.8f32)));
        f.tmp_long = cast::<i64, _>(rng.rnd_range(85, 11));
        if g.entity_status[cast::<i32, _>(f.tmp_long)] != 1 {
            f.tmp_long = cast::<i64, _>(rng.rnd_range(130, 11));
        }
        let b = cast::<i32, _>(f.tmp_long);
        if g.entity_status[b] == 1
            && (cast::<Ext, _>(f.amt_long * 5)) < g.t_bills[b] + g.cash[b]
            && b != e
            && 10 < g.corp_controller[b]
        {
            buy_bond_issue(g, f, b);
        }
    }
    if f.financing_mode <= 1 {
        return;
    }
    f.amt_long = to_quad(f.amount * (sf(0.5f32) * rng.rnd() + sf(0.8f32)));
    f.tmp_long = cast::<i64, _>(rng.rnd_range(130, 86));
    let b = cast::<i32, _>(f.tmp_long);
    if !(g.entity_status[b] == 1 && cast::<Ext, _>(f.amt_long * 5) <= g.t_bills[b] + g.cash[b]) {
        f.tmp_long = cast::<i64, _>(rng.rnd_range(130, 86));
    }
    let b = cast::<i32, _>(f.tmp_long);
    if g.entity_status[b] == 1
        && (cast::<Ext, _>(f.amt_long * 5)) < g.t_bills[b] + g.cash[b]
        && b != e
        && 10 < g.corp_controller[b]
    {
        buy_bond_issue(g, f, b);
    }
}

/// Bank/insurer `b` buys `amtLong` of entity `entId`'s new bonds at par (SimEntity_AiFinancing).
fn buy_bond_issue(g: &mut Globals, f: &mut Frame, b: i32) {
    let e = f.ent_id;
    f.cash_flow_score = b;
    f.slot = find_bond_holding_slot(g, f.cash_flow_score, e);
    if 0 < f.slot {
        if g.hold_bond_id[(f.slot, b)] == 0 {
            g.hold_bond_id[(0, b)] += 1;
            g.hold_bond_id[(f.slot, b)] = e;
        }
        g.hold_bond_face[(f.slot, b)] = f.amt_long;
        g.bond_hold_cost[(f.slot, b)] = cast::<Ext, _>(f.amt_long);
        g.cash[b] = g.cash[b] - cast::<Ext, _>(f.amt_long);
    }
}
