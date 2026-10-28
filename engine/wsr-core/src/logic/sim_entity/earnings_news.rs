//! GOSUB blocks of SimProcessNextEntity (see [`super`]): bond interest and maturities, the quarterly
//! earnings record and the earnings news.
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::Frame;
#[allow(unused_imports)]
use super::prelude::*;

use super::bonds::{bond_buyback, call_bonds, convert_or_redeem_convertibles};
use crate::logic::earnings::calc_projected_earnings;
use crate::logic::finance::{
    calc_entity_financials, get_controller_type, get_industry_category, get_managing_player,
};
use crate::logic::news::is_in_recent_headlines;
use crate::logic::valuation::{calc_stock_price, check_stock_split};
use crate::pb::numeric::Cur;

/// `VAL(MID$(BondIssueInfo(e), 1, 4))`: the maturity year of the bond issue.
fn bond_maturity_val(g: &Globals, e: i32) -> Ext {
    val(mid(g.bond_issue_info[e].as_bytes(), 1, 4).as_bytes())
}

/// `VAL(RIGHT$(BondIssueInfo(e), 5))` stored to a SINGLE: the coupon rate.
fn bond_coupon(g: &Globals, e: i32) -> f32 {
    cast::<f32, _>(val(right(g.bond_issue_info[e].as_bytes(), 5).as_bytes()))
}

/// GOSUB SimEntity_PayBondInterest (0x4d5ae2): once per tick (`bondIntPaid`), the issuer pays the
/// quarter's coupon on its bonds (accrued P&L, cash) to every bondholder (players, banks, insurers,
/// funds; with amortization of premium/discount) — or, for a weak issuer that can't pay, defaults:
/// the interest is added to the bonds (`bondIntStatus = -9999`) with a headline.
pub(super) fn pay_bond_interest(g: &mut Globals, _ui: &mut dyn Ui, _rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    f.line_marker = PbStr::from("29800");
    let e = f.ent_id;
    if 10 < e && g.entity_status[e] != 1 {
        return;
    }
    f.line_marker = PbStr::from(__content.string(10800));
    if f.bond_int_paid == "Y" {
        return;
    }
    f.line_marker = PbStr::from(__content.string(10801));
    if 10 < e {
        f.industry = g.industry_num[e];
        f.line_marker =
            PbStr::from(__content.string(10802)) + &PbStr::from(str_num(cast::<Ext, _>(f.industry), 7).as_str());
        if g.bonds_issued[e] > 0 {
            f.bond_int_paid = PbStr::from("Y");
            f.coupon_rate = bond_coupon(g, e);
            f.maturity_year = to_long(bond_maturity_val(g, e));
            f.amt_long = to_quad((cast::<Ext, _>(f.maturity_year) - cast::<Ext, _>(g.cur_year)) * Ext::from_f64(4.0));
            if 0 < f.amt_long {
                f.amt_long = to_quad(cast::<Ext, _>(f.amt_long) - (cast::<Ext, _>(g.cur_quarter) - Ext::from_f64(2.0)));
            }
            f.interest_amt = cast::<Ext, _>(g.bonds_issued[e]) * sf(0.0025f32) * cast::<Ext, _>(f.coupon_rate);
            g.accrued_pl[e] = g.accrued_pl[e] - f.interest_amt;
            f.ok_flag = 1;
            if f.amt_long <= 0 || f.rating_str == "D" {
                if f.interest_amt > g.t_bills[e] + g.cash[e] {
                    f.ok_flag = 0;
                }
                if f.industry < 3 {
                    f.ok_flag = 0;
                }
                if g.bank_debt[e] > Ext::from_f64(0.0) {
                    f.ok_flag = 0;
                }
                if f.credit_rating == 0 {
                    f.ok_flag = 0;
                }
                if 2 < f.credit_rating {
                    f.ok_flag = 1;
                }
            }
            if f.ok_flag == 0 {
                g.bonds_issued[e] = to_quad(f.interest_amt + cast::<Ext, _>(g.bonds_issued[e]));
                f.bond_int_status = -Ext::from_f64(9999.0);
            } else {
                g.cash[e] = g.cash[e] - f.interest_amt;
            }
            f.idx = 1;
            while f.idx <= 1583 {
                let h = f.idx;
                if h < 131 || 1580 < h {
                    f.slot = 1;
                    while f.slot < 16 {
                        let s = f.slot;
                        if e == g.hold_bond_id[(s, h)] {
                            f.amount = Ext::from_f64(0.0);
                            f.interest_amt =
                                cast::<Ext, _>(g.hold_bond_face[(s, h)]) * sf(0.0025f32) * cast::<Ext, _>(f.coupon_rate);
                            if f.ok_flag == 1 {
                                f.amount =
                                    cast::<Ext, _>(g.hold_bond_face[(s, h)]) - g.bond_hold_cost[(s, h)];
                                if f.amt_long < 1 {
                                    f.amt_long = 1;
                                }
                                if f.amount != Ext::from_f64(0.0) {
                                    f.amount = f.amount / cast::<Ext, _>(f.amt_long);
                                }
                                if f.amount < Ext::from_f64(0.0)
                                    && 0 < instr(1, g.bond_issue_info[e].as_bytes(), b"cv")
                                {
                                    f.amount = Ext::from_f64(0.0);
                                }
                                g.bond_hold_cost[(s, h)] = f.amount + g.bond_hold_cost[(s, h)];
                                g.accrued_pl[h] = (f.interest_amt + g.accrued_pl[h]) + f.amount;
                                g.cash[h] = f.interest_amt + g.cash[h];
                            } else {
                                g.hold_bond_face[(s, h)] =
                                    to_quad(f.interest_amt + cast::<Ext, _>(g.hold_bond_face[(s, h)]));
                            }
                            f.slot = 15;
                        }
                        f.slot += 1;
                    }
                }
                f.idx += 1;
            }
        }
        f.line_marker = PbStr::from(__content.string(10803));
        if g.bonds_issued[e] > 0 && f.ok_flag == 0 {
            g.news_msg =
                g.ent_symbol[e].clone() + __content.string(10804);
            if 0 < is_in_recent_headlines(g, __content.bytes(10805)) && 10 < g.corp_controller[e] {
                g.news_msg = PbStr::new();
            }
        }
    }
    f.line_marker = PbStr::from(__content.string(10806));
}

/// GOSUB SimEntity_BondMaturity (0x4d83dc): bonds maturing this year are paid off (to every holder,
/// at face, gains booked) with a headline — after a forced stock sale, asset sale and bond buyback for a
/// weak issuer, or bankruptcy (BankruptcyLiquidation) when the issuer can't pay; convertibles may be
/// converted instead.
pub(super) fn bond_maturity(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    f.line_marker = PbStr::from("30500");
    let e = f.ent_id;
    if g.entity_status[e] != 1 || g.bonds_issued[e] == 0 {
        return;
    }
    if (cast::<Ext, _>(g.cur_year)) < bond_maturity_val(g, e) {
        return;
    }
    calc_entity_financials(g, ui, rng, e);
    f.rating_str = g.credit_rating_str.clone();
    if g.credit_rating_num[e] < 4
        && g.industry_num[e] == 70
        && cast::<Ext, _>(g.bonds_issued[e]) * sf(0.75f32) > g.t_bills[e] + g.cash[e]
    {
        f.rating_str = PbStr::from("D");
    }
    if f.rating_str == "D" {
        f.bankrupt_code = 11;
        g.tmp_ext1 = cast::<Ext, _>(f.bankrupt_code);
        g.tmp_ext2 = Ext::from_f64(1.0);
        if !f.skips(0x4e7250) {
            super::liquidation::bankruptcy_liquidation(g, ui, rng, f.ent_id, f.time_secs);
        }
        f.bankrupt_code = 99;
    }
    if g.bonds_issued[e] == 0 {
        return;
    }
    calc_entity_financials(g, ui, rng, e);
    f.rating_str = g.credit_rating_str.clone();
    if 0 < f.bankrupt_code && g.cur_quarter < 4 {
        g.news_msg =
            g.ent_symbol[e].clone() + __content.string(10807);
        f.idx = 1;
        while f.idx <= 15 {
            if 0 < g.hold_stock_id[(f.idx, e)] {
                g.tmp_ext1 = cast::<Ext, _>(e);
                f.tmp_long = cast::<i64, _>(g.hold_stock_id[(f.idx, e)]);
                g.tmp_ext2 = cast::<Ext, _>(g.hold_stock_pct[(f.idx, e)]);
                g.tmp_ext3 = Ext::from_f64(2.0);
                f.other_id = cast::<i32, _>(f.tmp_long);
                if !f.skips(0x4ed93b) {
                    super::transactions::sell_stock_holding(g, ui, rng, f.other_id);
                }
                f.idx = 15;
                f.sold_list =
                    f.sold_list.clone() + &trim(str_s(cast::<Ext, _>(f.other_id)).as_bytes()) + "^";
            }
            f.idx += 1;
        }
        if get_managing_player(g, e) == 0 && 2 < g.industry_num[e] && g.industry_num[e] < 70 {
            g.distressed_corp = e;
        }
        g.tmp_ext1 = Ext::from_f64(0.0);
        g.tmp_ext2 = Ext::from_f64(2.0);
        g.tmp_ext3 = -Ext::from_f64(1.0);
        if !f.skips(0x4e4128) {
            bond_buyback(g, ui, rng, e);
        }
        if cast::<Ext, _>(g.bonds_issued[e]) * sf(1.04f32) > g.t_bills[e] + g.cash[e] {
            post_news(g, ui, e, 0);
            return;
        }
    }
    if cast::<Ext, _>(g.bonds_issued[e]) * sf(1.04f32) > g.t_bills[e] + g.cash[e]
        && !(g.unused_credit_line > Ext::from_f64(0.0))
        && g.credit_rating_num[e] <= 5
    {
        f.ok_flag = 0;
        let ind = g.industry_num[e];
        if ind < 3 || 69 < ind {
            f.ok_flag = 1;
        } else {
            let bi = cast::<Ext, _>(g.bonds_issued[e]);
            if g.business_assets[e] < g.total_assets * sf(0.5f32) && bi < g.total_assets * sf(0.5f32) {
                f.ok_flag = 1;
            }
            if bi < g.total_assets * sf(0.15f32) {
                f.ok_flag = 1;
            }
            if bi < (g.t_bills[e] + g.cash[e]) * sf(1.1f32) {
                f.ok_flag = 1;
            }
            if g.bank_debt[e] < bi * sf(0.1f32) {
                let ba = g.business_assets[e];
                if f.rating_str == "B" && ba < g.total_assets * sf(0.6f32) {
                    f.ok_flag = 1;
                }
                if f.rating_str == "CCC" && ba < g.total_assets * sf(0.5f32) {
                    f.ok_flag = 1;
                }
                if f.rating_str == "CC" && ba < g.total_assets * sf(0.4f32) {
                    f.ok_flag = 1;
                }
                if f.rating_str == "C" && ba < g.total_assets * sf(0.3f32) {
                    f.ok_flag = 1;
                }
            }
            if f.ok_flag == 0 {
                if g.bond_price[e].get() > Ext::from_f64(85.0) {
                    g.bond_price[e] = Cur::from_f64(Ext::from_f64(9.0) * rng.rnd() + Ext::from_f64(75.0));
                }
                f.amount = g.bond_price[e].get() * sf(0.01f32) * cast::<Ext, _>(g.bonds_public_held) * sf(0.25f32);
                if g.business_assets[e] > f.amount {
                    g.business_assets[e] = g.business_assets[e] - f.amount;
                    g.cash[e] = f.amount * sf(0.9f32) + g.cash[e];
                    g.extraordinary_pl[e] =
                        to_quad(cast::<Ext, _>(g.extraordinary_pl[e]) - f.amount * sf(0.1f32));
                    g.cash[e] = g.working_capital[e] * sf(0.7f32) + g.cash[e];
                    g.working_capital[e] = sf(0.3f32) * g.working_capital[e];
                    g.tmp_ext1 = Ext::from_f64(0.0);
                    g.tmp_ext2 = Ext::from_f64(2.0);
                    g.tmp_ext3 = -Ext::from_f64(1.0);
                    if !f.skips(0x4e4128) {
                        bond_buyback(g, ui, rng, e);
                    }
                    calc_entity_financials(g, ui, rng, e);
                    f.rating_str = g.credit_rating_str.clone();
                    if 2 < g.credit_rating_num[e] {
                        f.ok_flag = 1;
                    }
                }
            }
        }
        if f.ok_flag != 1 {
            if f.bankrupt_code == 0 {
                f.bankrupt_code = 11;
                g.tmp_ext1 = cast::<Ext, _>(f.bankrupt_code);
                g.tmp_ext2 = Ext::from_f64(1.0);
                if !f.skips(0x4e7250) {
                    super::liquidation::bankruptcy_liquidation(g, ui, rng, f.ent_id, f.time_secs);
                }
                f.bankrupt_code = 99;
            }
            calc_entity_financials(g, ui, rng, e);
            f.rating_str = g.credit_rating_str.clone();
            if f.bankrupt_code == 11 && f.rating_str == "D" && g.cur_quarter < 4 {
                g.news_msg = g.ent_symbol[e].clone()
                    + __content.string(10807);
                post_news(g, ui, e, 0);
                f.bankrupt_code = 99;
                return;
            }
        }
    }
    f.tmp_long = g.bonds_issued[e];
    f.bonds_high = cast::<i32, _>(f.tmp_long >> 32);
    if f.tmp_long == 0 {
        return;
    }
    f.credit_rating = g.credit_rating_num[e];
    if !f.skips(0x4d5ae2) {
        pay_bond_interest(g, ui, rng, f);
    }
    if f.bond_int_status == -Ext::from_f64(9999.0) {
        f.bond_int_status = Ext::from_f64(0.0);
    }
    calc_entity_financials(g, ui, rng, e);
    f.rating_str = g.credit_rating_str.clone();
    if !f.skips(0x686c0f) && convert_or_redeem_convertibles(g, ui, rng, e, 1, 2) == 1 {
        return;
    }
    f.bond_int_status = -Ext::from_f64(999.0);
    f.coupon_rate = bond_coupon(g, e);
    f.interest_amt = cast::<Ext, _>(g.bonds_issued[e]) * sf(0.0025f32) * cast::<Ext, _>(f.coupon_rate);
    g.accrued_pl[e] = g.accrued_pl[e] - f.interest_amt;
    g.cash[e] = g.cash[e] - (f.interest_amt + cast::<Ext, _>(g.bonds_issued[e]));
    f.idx = 1;
    while f.idx <= 1583 {
        let h = f.idx;
        if h < 131 || 1580 < h {
            f.slot = 1;
            while f.slot < 16 {
                let s = f.slot;
                if e == g.hold_bond_id[(s, h)] {
                    f.amount = cast::<Ext, _>(g.hold_bond_face[(s, h)]) - g.bond_hold_cost[(s, h)];
                    if instr(1, g.bond_issue_info[e].as_bytes(), b"cv") == 0 && f.amount > Ext::from_f64(0.0) {
                        g.accrued_pl[h] = f.amount + g.accrued_pl[h];
                    } else if h > 10 {
                        g.extraordinary_pl[h] = to_quad(f.amount + cast::<Ext, _>(g.extraordinary_pl[h]));
                    } else {
                        g.player_cap_gains[h] = f.amount + g.player_cap_gains[h];
                    }
                    g.cash[h] = cast::<Ext, _>(g.hold_bond_face[(s, h)]) + g.cash[h];
                    g.hold_bond_id[(s, h)] = 0;
                    g.hold_bond_face[(s, h)] = 0;
                    g.bond_hold_cost[(s, h)] = Ext::from_f64(0.0);
                    f.slot = 15;
                }
                f.slot += 1;
            }
        }
        f.idx += 1;
    }
    f.msg_part1 = PbStr::from(__content.string(10808));
    f.msg_part2 = PbStr::from("at par.");
    if g.sim_co_count % 2 == 1 {
        f.msg_part1 = PbStr::from(__content.string(10809));
    }
    if g.rand_seed % 3 == 1 {
        f.msg_part2 = PbStr::from(__content.string(10810));
    }
    if g.rand_seed % 3 == 2 {
        f.msg_part2 = PbStr::from(__content.string(10811));
    }
    if 0 < f.bankrupt_code {
        f.msg_part2 = PbStr::from(__content.string(10812));
    }
    g.news_msg = g.ent_symbol[e].clone()
        + " "
        + &f.msg_part1
        + &g.currency_symbol
        + &fmt_trim(cast::<Ext, _>(f.tmp_long), b"* ###,")
        + " "
        + &g.million_prefix_lc
        + __content.string(10813)
        + &PbStr::from(str_num(cast::<Ext, _>(f.coupon_rate), 7).as_str())
        + __content.string(10814)
        + &f.msg_part2;
    if is_in_recent_headlines(g, __content.bytes(10815)) == 0 || g.cur_player == g.corp_controller[e] {
        post_news(g, ui, e, 0);
    } else {
        g.news_msg = PbStr::new();
    }
    g.bonds_issued[e] = 0;
    g.bond_issue_info[e] = PbStr::new();
    g.bond_price[e] = Cur(0);
}

/// GOSUB SimEntity_BuildEarningsRecord (0x4ce98d): rebuilds `EntMiscData(entId)` as the 41-field `|`
/// record of the quarter (earnings, dividend change and payout, debt payments, margins, taxes, bond
/// operations, asset sales, ...) read by the earnings report and the next quarter.
pub(super) fn build_earnings_record(g: &mut Globals, f: &mut Frame) {
    f.line_marker = PbStr::from("26200");
    let e = f.ent_id;
    if g.entity_status[e] != 1 {
        return;
    }
    let money = f.fmt_money.clone();
    let whole = f.fmt_whole.clone();
    let m = |x: Ext| fmt_trim(x, money.as_bytes());
    let w = |x: Ext| fmt_trim(x, whole.as_bytes());
    let v = |s: &PbStr| val(s.as_bytes());
    let d = &mut f.dim_arr;
    f.covenant_pay = f.misc_debt_field - f.bank_debt_now;
    d[1] = g.ent_misc_data[e].clone();
    g.ent_misc_data[e] = PbStr::new();
    f.amount = g.earnings_y1[e] * sf(0.25f32) * cast::<Ext, _>(g.cur_quarter);
    d[2] = m(f.amount);
    if f.amount == Ext::from_f64(0.0) {
        d[2] = PbStr::new();
    }
    d[3] = m(f.net_earnings);
    d[4] = m(g.earnings_y1[e] * sf(0.25f32));
    if v(&d[4]) == Ext::from_f64(0.0) {
        d[4] = PbStr::new();
    }
    d[5] = m(f.earnings_ytd_save);
    if f.earnings_ytd_save == Ext::from_f64(0.0) {
        d[5] = PbStr::new();
    }
    d[6] = str_s(cast::<Ext, _>(g.cur_quarter));
    d[7] = w(f.misc_debt_field);
    if f.misc_debt_field == Ext::from_f64(0.0) {
        d[7] = PbStr::new();
    }
    d[8] = w(f.extra_pl_adj);
    if f.extra_pl_adj == Ext::from_f64(0.0) {
        d[8] = PbStr::new();
    }
    d[9] = f.sold_list.clone();
    d[10] = fmt_trim(cast::<Ext, _>(f.start_growth), b"###");
    if f.start_growth == 0 {
        d[10] = PbStr::from("0");
    }
    f.div_change = 0.0f32;
    if f.start_div_per_share > g.dividend_per_share[e] {
        f.div_change = -1.0f32;
    } else if g.dividend_per_share[e] > f.start_div_per_share {
        f.div_change = 1.0f32;
    }
    if f.start_div_per_share == Ext::from_f64(0.0) && f.div_change == Ext::from_f32(1.0f32) {
        f.div_change = 2.0f32;
    }
    d[11] = fmt_trim(cast::<Ext, _>(f.div_change), b"##");
    if g.industry_num[e] == 71 {
        f.div_change = 0.0f32;
    }
    if f.div_change == Ext::from_f32(0.0f32) {
        d[11] = PbStr::new();
    }
    d[12] = m(f.div_payout);
    if f.div_payout == Ext::from_f64(0.0) {
        d[12] = PbStr::new();
    }
    d[13] = m(f.covenant_pay);
    if f.covenant_pay == Ext::from_f64(0.0) {
        d[13] = PbStr::new();
    }
    d[14] = f.margin_text.clone();
    if v(&d[14]) == Ext::from_f64(0.0) {
        d[14] = PbStr::from("0");
    }
    d[15] = w(f.bond_int_status);
    if f.bond_int_status == Ext::from_f64(0.0) {
        d[15] = PbStr::new();
    }
    d[16] = w(cast::<Ext, _>(f.industry_orig));
    f.tmp_long = to_quad(f.reserve_provision);
    d[17] = w(cast::<Ext, _>(f.tmp_long));
    if f.tmp_long == 0 {
        d[17] = PbStr::new();
    }
    d[18] = w(f.uw_profit_ytd);
    if f.uw_profit_ytd == Ext::from_f64(0.0) {
        d[18] = PbStr::new();
    }
    d[19] = w(f.liquid_cash);
    if f.liquid_cash == Ext::from_f64(0.0) {
        d[19] = PbStr::from("0");
    }
    d[20] = w(f.bank_debt_now);
    if f.bank_debt_now == Ext::from_f64(0.0) {
        d[20] = PbStr::new();
    }
    d[21] = w(f.consumer_write_off);
    if f.consumer_write_off == Ext::from_f64(0.0) {
        d[21] = PbStr::new();
    }
    d[22] = w(f.subprime_write_down);
    if f.subprime_write_down == Ext::from_f64(0.0) {
        d[22] = PbStr::from("0");
    }
    d[23] = f.asset_sale_text.clone();
    d[24] = w(cast::<Ext, _>(f.bond_op_code2));
    if f.bond_op_code2 == 0 {
        d[24] = PbStr::new();
    }
    d[25] = w(cast::<Ext, _>(f.bond_op_code3));
    if f.bond_op_code3 == 0 {
        d[25] = PbStr::new();
    }
    d[26] = w(f.cap_gain_dist);
    if 1580 < e && g.industry_num[e] == 71 && g.cur_quarter < 4 {
        d[26] = PbStr::new();
    }
    if f.cap_gain_dist == Ext::from_f64(0.0) {
        d[26] = PbStr::new();
    }
    d[27] = m(f.income_tax);
    if f.income_tax == Ext::from_f64(0.0) {
        d[27] = PbStr::new();
    }
    d[28] = w(cast::<Ext, _>(f.tax_parent_id));
    if f.tax_parent_id == 0 {
        d[28] = PbStr::new();
    }
    d[29] = w(cast::<Ext, _>(f.disc_spend));
    if f.disc_spend == 0 || 69 < g.industry_num[e] {
        d[29] = PbStr::new();
    }
    d[30] = fmt_trim(Ext::from_f64(100.0) * cast::<Ext, _>(f.margin_frac), b"###.0");
    if v(&d[30]) == Ext::from_f64(0.0) {
        d[30] = PbStr::from("0");
        if 69 < g.industry_num[e] {
            d[30] = PbStr::new();
        }
    }
    d[31] = w(cast::<Ext, _>(f.tax_flags));
    if v(&d[31]) == Ext::from_f64(0.0) {
        d[31] = PbStr::from("0");
    }
    let zero_to_0 = |d: &mut Vec<PbStr>, k: usize, s: PbStr| {
        d[k] = s;
        if val(d[k].as_bytes()) == Ext::from_f64(0.0) {
            d[k] = PbStr::from("0");
        }
    };
    zero_to_0(d, 32, m(f.goodwill_charge));
    zero_to_0(d, 33, m(f.carbon_tax));
    zero_to_0(d, 34, m(f.oil_windfall_tax));
    zero_to_0(d, 35, m(f.holding_co_tax));
    zero_to_0(d, 36, m(f.corp_shares_tax));
    zero_to_0(d, 37, m(f.health_care_tax));
    if e < 131 {
        f.amount = Ext::from_f64(0.0);
    } else {
        f.amount = g.corp_hedging_gain[e];
    }
    zero_to_0(d, 38, m(f.amount));
    zero_to_0(d, 39, m(f.tax_credit_used));
    d[40] = m(f.reserve_adj);
    if val(d[40].as_bytes()).abs() < Ext::from_f64(1.0) {
        d[40] = PbStr::from("0");
    }
    zero_to_0(d, 41, m(f.proj_earnings));
    if 130 < e {
        g.corp_hedging_gain[e] = Ext::from_f64(0.0);
    }
    f.idx = 1;
    while f.idx <= 41 {
        let k = cast::<usize, _>(f.idx);
        g.ent_misc_data[e] = g.ent_misc_data[e].clone() + &f.dim_arr[k] + "|";
        f.dim_arr[k] = PbStr::new();
        f.idx += 1;
    }
    f.line_marker = PbStr::from("26201");
}

/// GOSUB SimEntity_EarningsNews (0x4cff86): weak AI corps cut discretionary spending; EPS headline
/// fields; bond buybacks, calls and conversions; the quarter's earnings record (GOSUB
/// SimEntity_BuildEarningsRecord); earnings and price headlines and ticker; earnings surprise for the
/// selected entity; new projections and valuation; year-end roll of the earnings history; tax-shelter
/// profit taking; AI growth cuts and distressed corps.
pub(super) fn earnings_news(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    f.line_marker = PbStr::from("26890");
    let e = f.ent_id;
    if g.entity_status[e] != 1 {
        return;
    }
    if f.managing_player == 0
        && g.industry_num[e] < 70
        && (f.rating_str == "D" || f.rating_str == "C" || f.rating_str == "CC")
        && 1 < g.disc_spend_pct[e]
        && g.rand_seed % 3 == 2
        && 1 < g.industry_num[e]
    {
        f.msg_part1 = PbStr::from("R & D.");
        f.msg_part2 = PbStr::from(__content.string(2823));
        if g.rand_seed % 3 == 1 {
            f.msg_part2 = PbStr::from(__content.string(10816));
        }
        f.msg_part3 = g.ent_name[e].clone() + " (" + &g.ent_symbol[e] + ")";
        if g.rand_seed % 3 == 2 {
            f.msg_part2 = PbStr::from(" lowers");
        }
        f.idx = get_industry_category(g.industry_num[e]);
        if f.idx == 2 {
            f.msg_part1 = PbStr::from(__content.string(10817));
            f.msg_part3 = g.ent_symbol[e].clone();
        }
        if g.rand_seed % 11 == 2 || g.cur_player == g.corp_controller[e] {
            g.news_msg = f.msg_part3.clone() + &f.msg_part2 + __content.string(10818) + &f.msg_part1;
        }
        g.disc_spend_pct[e] = cast::<i16, _>(to_quad(sf(0.4f32) * cast::<Ext, _>(g.disc_spend_pct[e])));
        if g.disc_spend_pct[e] == 1 {
            g.disc_spend_pct[e] = 0;
        }
    }
    f.msg_part1 = fmt_trim(
        (sf(0.01f32) * f.net_earnings) / g.shares_outstanding[e],
        b"* ###.00",
    );
    f.msg_part2 = fmt_trim(
        (sf(0.0025f32) * g.earnings_y1[e]) / g.shares_outstanding[e],
        b"* ###.00",
    ) + " ";
    f.msg_part3 = PbStr::from(__content.string(7716));
    if val(f.msg_part1.as_bytes()) > val(f.msg_part2.as_bytes()) {
        f.msg_part3 = PbStr::from("up");
    }
    if val(f.msg_part1.as_bytes()) < val(f.msg_part2.as_bytes()) {
        f.msg_part3 = PbStr::from("down");
    }
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_ext2 = Ext::from_f64(0.0);
    g.tmp_ext3 = Ext::from_f64(0.0);
    if get_controller_type(g, e) != 1 {
        bond_operations(g, ui, rng, f);
    }
    let e = f.ent_id;
    if g.bonds_issued[e] > 0
        && 0 < instr(1, g.bond_issue_info[e].as_bytes(), b"cv")
        && g.bond_price[e].get() > Ext::from_f64(1000.0)
    {
        g.valuation_mode = 0;
        calc_stock_price(g, ui, rng, e, f.time_secs);
        if !f.skips(0x686c0f) {
            convert_or_redeem_convertibles(g, ui, rng, e, 0, 1);
        }
    }
    f.bond_ops_total = g.tmp_ext1 + f.bond_ops_total;
    f.bond_op_code2 = to_quad(g.tmp_ext2);
    f.bond_op_code3 = to_quad(g.tmp_ext3);
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_ext2 = Ext::from_f64(0.0);
    g.tmp_ext3 = Ext::from_f64(0.0);
    f.liquid_cash = g.t_bills[e] + g.cash[e];
    f.bank_debt_now = g.bank_debt[e];
    if !f.skips(0x4ce98d) {
        build_earnings_record(g, f);
    }
    if f.msg_part3 == __content.string(7716) {
        f.msg_part2 = PbStr::new();
    }
    f.amt_long = to_quad(cast::<Ext, _>(g.sim_co_count) - cast::<Ext, _>(g.last_news_co_cnt));
    f.line_marker = PbStr::from("26890A");
    if g.news_msg.is_empty() && f.amt_long > 30 && !(10 < e && g.industry_num[e] == 71) {
        if !(0 < instr(1, g.news_headline[60].as_bytes(), __content.bytes(1059)))
            && !(0 < instr(1, g.news_headline[59].as_bytes(), __content.bytes(1059)))
        {
            g.news_msg = g.ent_symbol[e].clone()
                + " (Q"
                + &fmt(cast::<Ext, _>(g.cur_quarter), b"#")
                + __content.string(10819)
                + &f.msg_part1
                + __content.string(10820)
                + &f.msg_part3
                + " from "
                + &f.msg_part2
                + __content.string(10821);
        }
    }
    f.cap_change = g.market_cap[e] - f.start_mkt_cap;
    f.msg_part1 = PbStr::from("_");
    if f.cap_change > Ext::from_f64(0.0) {
        f.msg_part1 = PbStr::from("_+");
    }
    f.msg_part2 = fmt(f.cap_change / g.shares_outstanding[e], b"####.00");
    f.msg_part2 = trim(f.msg_part2.as_bytes());
    f.msg_part3 = PbStr::new();
    if val(f.msg_part2.as_bytes()) == Ext::from_f64(0.0) {
        f.msg_part3 = PbStr::from("_UNCH");
    } else {
        f.msg_part3 = f.msg_part1.clone() + &f.msg_part2;
    }
    g.valuation_mode = 0;
    if g.market_cap[e] / g.shares_outstanding[e] > Ext::from_f64(300.0) && f.managing_player == 0 {
        check_stock_split(g, e);
    }
    calc_stock_price(g, ui, rng, e, f.time_secs);
    ticker(g, rng, f);
    let e = f.ent_id;
    if 10 < e
        && g.entity_status[e] == 1
        && f.proj_earnings != Ext::from_f64(0.0)
        && (g.sel_entity == e || g.active_entity == e || g.last_sel_entity == e)
        && (f.proj_earnings > Ext::from_f64(0.0) || f.net_earnings > Ext::from_f64(0.0))
    {
        f.amount = f.net_earnings - f.proj_earnings;
        if f.amount.abs() > f.proj_earnings.abs() * sf(0.1f32) && g.industry_num[e] != 71 {
            if f.net_earnings > f.proj_earnings {
                f.msg_part1 = PbStr::from(__content.string(10822));
                if f.amount > f.proj_earnings.abs() * sf(0.15f32) {
                    f.msg_part1 = replace(f.msg_part1.as_bytes(), __content.bytes(10823), __content.bytes(10824));
                }
                if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
                    f.msg_part1 = replace(f.msg_part1.as_bytes(), __content.bytes(2816), __content.bytes(10825));
                }
                g.market_cap[e] = sf(1.025f32) * g.market_cap[e];
                g.investor_sentiment[e] = g.investor_sentiment[e].wrapping_add(5);
            } else {
                f.msg_part1 = PbStr::from(__content.string(10826));
                if f.amount.abs() > f.proj_earnings * sf(0.15f32) {
                    f.msg_part1 = replace(f.msg_part1.as_bytes(), b"below", __content.bytes(8392));
                }
                g.market_cap[e] = sf(0.955f32) * g.market_cap[e];
                g.investor_sentiment[e] = g.investor_sentiment[e].wrapping_sub(5);
            }
            if 85 < e {
                g.news_msg = g.ent_symbol[e].clone() + &f.msg_part1;
                post_news(g, ui, 0, 0);
            }
        }
    }
    calc_projected_earnings(g, ui, rng, e);
    g.valuation_mode = 2;
    calc_stock_price(g, ui, rng, e, f.time_secs);
    f.prior_yr_earnings = g.earnings_y1[e];
    if 2 < g.industry_num[e] && g.industry_num[e] < 70 {
        g.extraordinary_pl[e] = to_quad(f.extra_pl_adj + cast::<Ext, _>(g.extraordinary_pl[e]));
    }
    calc_entity_financials(g, ui, rng, e);
    if g.cur_quarter == 4 {
        g.earnings_yr4_ago[e] = g.earnings_yr3_ago[e];
        g.earnings_yr3_ago[e] = g.earnings_yr2_ago[e];
        g.earnings_yr2_ago[e] = g.earnings_y1[e];
        g.earnings_y1[e] = g.earnings_ytd[e];
        g.earnings_ytd[e] = Ext::from_f64(0.0);
        g.corp_prior_net_worth[e] = g.net_worth;
    }
    if g.tmp_subsid_stock_value > Ext::from_f64(0.0)
        && g.tax_loss_carry[e] < g.exchange_rate * -Ext::from_f64(100.0)
        && !f.skips(0x4c4739)
    {
        super::ai_corp::ai_take_profits_tax_shelter(g, ui, rng);
    }
    if 85 < e && f.managing_player == 0 && !(0 < g.monetary_stance) {
        if 10 < g.growth_rate[e] {
            let r = rng.rnd_range(6, 4);
            if g.credit_rating_num[e] < r {
                g.growth_rate[e] = cast::<i16, _>(to_quad(cast::<Ext, _>(g.growth_rate[e]) - Ext::from_f64(8.0) * rng.rnd()));
            }
        }
        if g.credit_rating_str == "D"
            && 2 < g.industry_num[e]
            && g.industry_num[e] < 70
            && Ext::from_f64(10.0) * rng.rnd() < Ext::from_f64(1.0)
        {
            g.distressed_corp = e;
        }
    }
    f.line_marker = PbStr::from("26890-1");
}

/// The bond part of SimEntity_EarningsNews (non-human corps): buybacks of cheap bonds, calls of
/// expensive ones, conversions of convertibles.
fn bond_operations(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let e = f.ent_id;
    calc_entity_financials(g, ui, rng, e);
    f.rating_str = g.credit_rating_str.clone();
    if 85 < e
        && g.bonds_issued[e] > 0
        && 2 < g.credit_rating_num[e]
        && g.bond_price[e].get() < Ext::from_f64(90.0)
        && g.liquid_funds > cast::<Ext, _>(g.bonds_issued[e]) * sf(0.2f32)
    {
        f.maturity_year = to_long(bond_maturity_val(g, e));
        if f.maturity_year - g.cur_year <= 2 {
            if 2 < g.industry_num[e]
                && g.industry_num[e] < 70
                && g.liquid_funds < cast::<Ext, _>(g.bonds_issued[e]) * sf(0.7f32)
            {
                g.distressed_corp = e;
            }
            g.tmp_ext3 = -Ext::from_f64(1.0);
        }
        g.tmp_ext2 = Ext::from_f64(2.0);
        if !f.skips(0x4e4128) {
            bond_buyback(g, ui, rng, e);
        }
        return;
    }
    let bi = g.bonds_issued[e];
    if bi > 100 && g.bond_price[e].get() > Ext::from_f64(106.5) && g.liquid_funds > cast::<Ext, _>(bi * 3) {
        if Ext::from_f64(3.0) * rng.rnd() < Ext::from_f64(1.0) {
            if 6 < g.credit_rating_num[e] {
                g.tmp_ext1 = (sf(0.2f32) * rng.rnd() + sf(0.2f32)) * Ext::from_f64(100.0);
                g.tmp_ext2 = Ext::from_f64(2.0);
                if (cast::<Ext, _>(bi * 2)) < g.t_bills[e] + g.cash[e]
                    || !(cast::<Ext, _>(bi) >= Ext::from_f64(30.0) * rng.rnd() + Ext::from_f64(50.0))
                {
                    g.tmp_ext1 = Ext::from_f64(100.0);
                }
                if !f.skips(0x4f7c76) {
                    call_bonds(g, ui, rng, e);
                }
            }
        } else if Ext::from_f64(20.0) * rng.rnd() + Ext::from_f64(140.0) < g.bond_price[e].get()
            && (g.cur_year + e) % 2 == 1
            && (g.cur_quarter + e) % 2 == 0
        {
            f.tmp_short = to_integer(bond_maturity_val(g, e));
            f.tmp_short = f.tmp_short.wrapping_sub(cast::<i16, _>(g.cur_year));
            if f.tmp_short <= 9
                && 0 < instr(1, g.bond_issue_info[e].as_bytes(), b"cv")
                && !f.skips(0x686c0f)
            {
                convert_or_redeem_convertibles(g, ui, rng, e, 0, 2);
            }
        }
        return;
    }
    if 0 < instr(1, g.bond_issue_info[e].as_bytes(), b"cv")
        && Ext::from_f64(150.0) * rng.rnd() + Ext::from_f64(200.0) < g.bond_price[e].get()
    {
        f.tmp_short = to_integer(bond_maturity_val(g, e));
        f.tmp_short = f.tmp_short.wrapping_sub(cast::<i16, _>(g.cur_year));
        if f.tmp_short <= 9 && !f.skips(0x686c0f) {
            convert_or_redeem_convertibles(g, ui, rng, e, 0, 2);
        }
    }
}

/// The ticker-tape part of SimEntity_EarningsNews: the stock's price and change appended to
/// `TickerText` when the tape is short or the move is notable.
fn ticker(g: &mut Globals, rng: &mut Rng, f: &mut Frame) {
    let e = f.ent_id;
    let tl = len(g.ticker_text.as_bytes());
    if 115 <= tl {
        let skip = Ext::from_f64(18.0) * rng.rnd() >= Ext::from_f64(1.0)
            && f.cap_change.abs() >= f.start_mkt_cap * sf(0.013f32)
            && g.cur_player != g.corp_controller[e];
        if skip {
            return;
        }
    }
    f.ok_flag = 1;
    if len(g.ticker_text.as_bytes()) < 150 {
        let r = cast::<Ext, _>(rng.rnd_range(900, 150));
        if val(g.stock_price_str.as_bytes()) >= r {
            f.ok_flag = 0;
        } else {
            f.ok_flag = 1;
        }
    } else {
        f.ok_flag = 0;
    }
    if f.public_pct == 0 && 99 < len(g.ticker_text.as_bytes()) {
        f.ok_flag = 0;
    }
    if len(g.ticker_text.as_bytes()) < 100 {
        f.ok_flag = 1;
    }
    if f.ok_flag == 1 {
        g.ticker_text = g.ticker_text.clone()
            + "   "
            + &g.ent_name[e]
            + " ("
            + &g.ent_symbol[e]
            + ") "
            + &g.stock_price_str
            + &f.msg_part3;
    }
}
