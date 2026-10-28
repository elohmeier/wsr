//! Corporate bond and stock issuance: `BondBuyback` (0x4e4128), `CallBonds` (0x4f7c76),
//! `ConvertOrRedeemConvertibles` (0x686c0f), `CountBondIssuers` (0x687f52), `CountConvertibleIssuers`
//! (0x687e68), `IssueBonds` (0x5231bc), `IssueStock` (0x5223a3) and their helpers `NormalizeHoldingPcts`
//! (0x5212b0), `AdjustConversionPrice` (0x6890c5), `AdjustPriceAlertsForStock` (0x6845d0),
//! `ScaleEarningsHistory` (0x54e63b), `ForceRegulatedCallSales` (0x5f8e12).
//!
//! Bond issue info (`BondIssueInfo`) is `"YYYY@cc.cc"` (maturity year, coupon) or for convertibles
//! `"YYYYcv|<conversion market cap>|@cc.cc"`.

#![allow(
    clippy::assign_op_pattern,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord
)]
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::prelude::*;
use crate::logic::cashflow::get_etf_adviser_human_status;
use crate::logic::control::{find_option_positions, update_corp_control};
use crate::logic::earnings::calc_accrued_bond_interest;
use crate::logic::finance::{calc_credit_rating, calc_entity_financials, get_controller_type};
use crate::logic::holdings::{
    PackEvent, find_bond_holding_slot, find_holding_slot, find_stock_holding, recapture_excess_loss,
};
use crate::logic::news::is_in_recent_headlines;
use crate::logic::options::close_option_positions;
use crate::logic::report::show_quick_message;
use crate::logic::reports_corp::calc_bond_call_price_pct;
use crate::logic::reports_lists::update_control_chain;
use crate::logic::trading::borrow_to_cover_purchase;
use crate::logic::sim_entity::dividends::adjust_options_for_corp_action;
use crate::logic::valuation::{
    adjust_index_divisor, price_pct_change_months, price_trend_streak, scale_price_history,
};
use crate::pb::string::{CaseMode, mcase, parse_count};
use crate::pb::ext80::Ext80;
use crate::pb::numeric::format_ext;
use crate::pb::numeric::val_ext;
use crate::pb::numeric::Cur;

/// `RTRIM$(LTRIM$(FORMAT$(x, mask)))`.
fn fm(x: Ext, mask: &str) -> PbStr {
    trim(&format(x, mask.as_bytes()))
}

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// Port of CountBondIssuers (0x687f52).
/// Number of active entities 11..1600 with bonds outstanding.
pub fn count_bond_issuers(g: &Globals) -> i32 {
    let mut count = 0;
    for corp_id in 11..=1600 {
        if g.entity_status[corp_id] == 1 && g.bonds_issued[corp_id] != 0 {
            count += 1;
        }
    }
    count
}

/// Port of CountConvertibleIssuers (0x687e68).
/// Number of active entities 11..1600 with convertible bonds (`"cv"` in `BondIssueInfo`) outstanding.
pub fn count_convertible_issuers(g: &Globals) -> i32 {
    let mut count = 0;
    for corp_id in 11..=1600 {
        if g.entity_status[corp_id] == 1
            && g.bonds_issued[corp_id] != 0
            && 0 < instr(1, g.bond_issue_info[corp_id].as_bytes(), b"cv")
        {
            count += 1;
        }
    }
    count
}

/// Port of BondBuyback (0x4e4128).
/// Issuer `issuer_id` buys back its bonds in the market. Inputs in globals (cleared at entry): `TmpExt1` =
/// face amount to buy (mode 0), `TmpExt2` = mode (0 = player order with confirmation; 1/2 = AI, which picks
/// 6..20% of the issue or all public bonds and borrows any shortfall from its bank; 2 also reports the
/// borrowed amount / face bought / gain in `TmpExt1/2/3` and skips well-priced or long bonds),
/// `TmpExt3 = -1` = forced (no rating/bank-debt guard). Pays price + 1% + accrued interest, books the
/// gain to `ExtraordinaryPL`, raises `BondPrice` by 3 and sets `NewsMsg` (mode 0 posts it and confirms).
/// A player's cash shortfall (mode 0) is financed by `BorrowToCoverPurchase` (NO ends the buyback).
pub fn bond_buyback(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, issuer_id: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut distress_flag = to_long(g.tmp_ext3);
    if distress_flag != -1 {
        distress_flag = 0;
    }
    let mut bonds_to_buy: i64 = to_quad(g.tmp_ext1);
    let mode = to_long(g.tmp_ext2);
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_ext2 = Ext::from_f64(0.0);
    g.tmp_ext3 = Ext::from_f64(0.0);
    let id = issuer_id;
    if id < 11 || g.entity_status[id] != 1 {
        return;
    }
    if (cast::<Ext, _>(g.bonds_issued[id])) < Ext::from_f64(10.0) && mode == 2 {
        return;
    }
    calc_entity_financials(g, ui, rng, id);
    if g.bonds_public_held == 0 {
        return;
    }
    if g.credit_rating_num[id] < 5 && distress_flag != -1 {
        distress_flag = 1;
    }
    if distress_flag == 1 && g.exchange_rate * Ext::from_f64(100.0) < g.bank_debt[id] && 85 < id {
        return;
    }
    if 1 < mode {
        let mut years_to_maturity =
            to_long(val(mid(g.bond_issue_info[id].as_bytes(), 1, 4).as_bytes()));
        years_to_maturity = years_to_maturity - g.cur_year;
        if rng.rnd() * Ext::from_f64(10.0) + Ext::from_f64(60.0) < g.bond_price[id].get() && 6 < years_to_maturity {
            return;
        }
        if rng.rnd() * Ext::from_f64(10.0) + Ext::from_f64(85.0) < g.bond_price[id].get() {
            return;
        }
    }
    if !(Ext::from_f64(0.0) < g.liquid_funds) && 0 < mode {
        return;
    }
    if 0 < mode {
        let bph = g.bonds_public_held;
        if !((cast::<Ext, _>(bph)) >= g.exchange_rate * Ext::from_f64(100.0)) && (cast::<Ext, _>(bph)) < g.liquid_funds {
            bonds_to_buy = bph;
        } else {
            bonds_to_buy = to_quad(cast::<Ext, _>(g.bonds_issued[id]) * (rng.rnd() * sf(0.14f32) + sf(0.06f32)));
            if bph < bonds_to_buy {
                bonds_to_buy = bph;
            }
        }
    }
    let accrued_int = calc_accrued_bond_interest(g, bonds_to_buy, id);
    let cost = sf(0.01f32) * (Ext::from_f64(1.0) + g.bond_price[id].get()) * cast::<Ext, _>(bonds_to_buy);
    if !(cost >= Ext::from_f64(2.0)) && 0 < mode && (cast::<Ext, _>(g.bonds_issued[id])) > Ext::from_f64(5.0) {
        return;
    }
    if (cost + accrued_int) * sf(0.995f32) > g.liquid_funds + sf(0.5f32) {
        // LAB_004e509b
        if mode == 0 {
            let mut msg =
                s(__content.string(2167)) + &g.ent_name[id] + " LACKS " + __content.string(10722);
            show_quick_message(g, ui, &mut msg);
        }
        return;
    }
    let mut shortfall = (cost + accrued_int) - (g.cash[id] + g.t_bills[id]);
    if mode == 0 && shortfall > Ext::from_f64(0.0) && !(shortfall >= cost * sf(0.005f32)) {
        shortfall = Ext::from_f64(0.0);
    }
    if shortfall > Ext::from_f64(0.0) {
        if mode < 1 {
            let mut done = false;
            if 1585 < id
                && g.industry_num[id] == 71
                && g.sel_entity == id
                && get_etf_adviser_human_status(g, ui, rng, id) == 2
            {
                g.bank_debt[id] = shortfall + g.bank_debt[id];
                let l = g.lender_id;
                g.cash[l] = g.cash[l] - shortfall;
                g.cash[id] = shortfall + g.cash[id];
                done = true;
            }
            if !done {
                let premium = cost - g.bond_price[id].get() * cast::<Ext, _>(bonds_to_buy) * sf(0.01f32);
                if borrow_to_cover_purchase(g, ui, &mut shortfall, premium, bonds_to_buy) == 7 {
                    return;
                }
            }
        } else {
            if g.industry_num[id] == 1 {
                if mode == 0 {
                    let mut msg = s(__content.string(2167))
                        + &g.ent_name[id]
                        + " LACKS "
                        + __content.string(10722);
                    show_quick_message(g, ui, &mut msg);
                }
                return;
            }
            g.bank_debt[id] = shortfall + g.bank_debt[id];
            if mode == 2 {
                g.tmp_ext1 = shortfall;
            }
            let l = g.lender_id;
            g.cash[l] = g.cash[l] - shortfall;
            g.cash[id] = shortfall + g.cash[id];
        }
    }
    // LAB_004e4989
    g.cash[id] = g.cash[id] - (cost + accrued_int);
    g.accrued_pl[id] = g.accrued_pl[id] - accrued_int;
    let p = Ext::from_f64(3.0) + g.bond_price[id].get();
    g.bond_price[id] = Cur::from_f64(p);
    g.bonds_issued[id] = to_quad(cast::<Ext, _>(g.bonds_issued[id]) - cast::<Ext, _>(bonds_to_buy));
    if mode == 2 {
        g.tmp_ext2 = cast::<Ext, _>(bonds_to_buy);
    }
    let gain: i64 = to_quad(cast::<Ext, _>(bonds_to_buy) - cost);
    if mode == 2 {
        g.tmp_ext3 = cast::<Ext, _>(gain);
    }
    g.extraordinary_pl[id] = to_quad(cast::<Ext, _>(gain) + cast::<Ext, _>(g.extraordinary_pl[id]));
    if g.rand_seed % 20 != 3 {
        let r = rng.rnd() * Ext::from_f64(100.0) + Ext::from_f64(250.0);
        if !(cast::<Ext, _>(gain) > r) && mode != 0 {
            return;
        }
    }
    if !((cast::<Ext, _>(gain)).abs() >= Ext::from_f64(1.0)) && 0 < mode {
        return;
    }
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    let mut msg_text = s(__content.string(10723));
    if g.rand_seed % 3 == 1 {
        msg_text = s(__content.string(10724));
    }
    let mut gain_word = s("gain");
    if (cast::<Ext, _>(gain)) < Ext::from_f64(0.0) {
        gain_word = s("loss");
    }
    if g.rand_seed % 5 < 2 || mode == 0 {
        g.news_msg = g.ent_symbol[id].clone()
            + &msg_text
            + &g.currency_symbol
            + &fm(cast::<Ext, _>(bonds_to_buy), "* ###,")
            + " "
            + &g.million_prefix_lc
            + __content.string(10725)
            + &gain_word
            + " of "
            + &g.currency_symbol
            + &fm((cast::<Ext, _>(gain)).abs(), "* ###,")
            + " "
            + &g.million_prefix_lc
            + "il."
            + &g.currency_suffix
            + "...";
    }
    if mode == 0 {
        post_news(g, ui, 0, 0);
        let cs = trim(g.currency_symbol.as_bytes());
        let msg = s(__content.string(9861))
            + "\r"
            + "\r"
            + &g.ent_name[id]
            + __content.string(10726)
            + &cs
            + &fm(cast::<Ext, _>(bonds_to_buy), "* ###,")
            + " "
            + &g.million_prefix_lc
            + "illion "
            + __content.string(10727)
            + &cs
            + &fm(cost, "* ###,.00")
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + __content.string(10728)
            + &cs
            + &fm(accrued_int, "* ###,.00")
            + " "
            + &g.million_prefix_lc
            + "illion.";
        ui.msgbox(&msg, 0x2000, &s(__content.string(10729)));
        g.turn_action_count += 1;
        if g.industry_num[id] == 71 && 0 < g.fund_bonus_year[id] {
            g.fund_bonus_year[id] = g.cur_year;
        }
    }
}

/// `STR$` of a LONG/QUAD/EXT value (15 digits).
fn str15(x: Ext) -> PbStr {
    PbStr::from(str_num(x, 15).as_str())
}

/// Port of CallBonds (0x4f7c76).
/// Issuer `issuer_id` redeems `TmpExt1` percent of [runtime resource wording]
/// ([`calc_bond_call_price_pct`] of the years to maturity) plus accrued interest. `TmpExt2` = mode
/// (0 = player order: confirmation box, may borrow; 1/2 = AI: only bonds maturing within 9 years, a cash
/// shortfall is borrowed from the lending bank). Both inputs are cleared. A convertible whose conversion
/// value exceeds the call price is not called (mode 2 with a value over 1,000 forces the conversion
/// instead, [`convert_or_redeem_convertibles`]). Holders get the payout and accrued interest, their
/// gain goes to `PlayerCapGains` (players, with news), `AccruedPL` (banks) or `ExtraordinaryPL`.
/// A player's cash shortfall (mode 0) is financed by `BorrowToCoverPurchase` (NO ends the call).
pub fn call_bonds(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, issuer_id: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let id = issuer_id;
    let call_pct = to_long(g.tmp_ext1);
    let call_frac = cast::<f32, _>(cast::<Ext, _>(call_pct) * sf(0.01f32));
    let face_called: i64 = to_quad(cast::<Ext, _>(call_frac) * cast::<Ext, _>(g.bonds_issued[id]));
    let mode = to_long(g.tmp_ext2);
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_ext2 = Ext::from_f64(0.0);
    let mut years_to_maturity =
        to_long(val(mid(g.bond_issue_info[id].as_bytes(), 1, 4).as_bytes()));
    years_to_maturity = years_to_maturity - g.cur_year;
    let call_price = calc_bond_call_price_pct(years_to_maturity);
    if 0 < instr(1, g.bond_issue_info[id].as_bytes(), b"cv") {
        let share_price = g.market_cap[id] / g.shares_outstanding[id];
        let issue_info = g.bond_issue_info[id].clone();
        let mut conv_price: Ext = val(parse_any(issue_info.as_bytes(), b"|", 2).as_bytes());
        conv_price = conv_price / g.shares_outstanding[id];
        let shares_per_bond: Ext = Ext::from_f64(1000.0) / conv_price;
        let conv_value = share_price * shares_per_bond * sf(0.1f32);
        if conv_value > cast::<Ext, _>(call_price) {
            if mode == 2 && conv_value > Ext::from_f64(1000.0) {
                convert_or_redeem_convertibles(g, ui, rng, id, 0, 1);
            }
            return;
        }
    }
    if 9 < years_to_maturity && 0 < mode {
        return;
    }
    let call_cost = sf(0.01f32) * cast::<Ext, _>(call_price) * cast::<Ext, _>(face_called);
    let accrued_int = calc_accrued_bond_interest(g, face_called, id);
    calc_entity_financials(g, ui, rng, id);
    if !(g.liquid_funds >= call_cost + accrued_int) {
        if mode == 0 {
            let msg = s(__content.string(2167))
                + &g.ent_name[id]
                + " LACKS "
                + __content.string(10730);
            ui.msgbox(&msg, 0x1010, &s(__content.string(10731)));
        }
        return;
    }
    let mut shortfall = (call_cost + accrued_int) - (g.cash[id] + g.t_bills[id]);
    if shortfall > Ext::from_f64(0.0) {
        if 0 < mode {
            g.cash[id] = shortfall + g.cash[id];
            g.bank_debt[id] = shortfall + g.bank_debt[id];
            let lb = cast::<i32, _>(g.lending_bank[id]);
            g.cash[lb] = g.cash[lb] - shortfall;
        } else {
            let call_premium = (call_cost + accrued_int) - cast::<Ext, _>(face_called);
            if borrow_to_cover_purchase(g, ui, &mut shortfall, call_premium, face_called) == 7 {
                return;
            }
        }
    }
    for holder_id in 1..=1583 {
        if 130 < holder_id && holder_id < 1581 {
            continue;
        }
        for slot in 1..=15 {
            if id != g.hold_bond_id[(slot, holder_id)] {
                continue;
            }
            let payout = sf(0.01f32)
                * cast::<Ext, _>(call_price)
                * (cast::<Ext, _>(call_frac) * cast::<Ext, _>(g.hold_bond_face[(slot, holder_id)]));
            g.cash[holder_id] = payout + g.cash[holder_id];
            let holder_face_called: i64 =
                to_quad(cast::<Ext, _>(call_frac) * cast::<Ext, _>(g.hold_bond_face[(slot, holder_id)]));
            let holder_accrued = calc_accrued_bond_interest(g, holder_face_called, id);
            g.accrued_pl[holder_id] = holder_accrued + g.accrued_pl[holder_id];
            g.cash[holder_id] = holder_accrued + g.cash[holder_id];
            let gain;
            if call_pct == 100 {
                gain = payout - g.bond_hold_cost[(slot, holder_id)];
                g.hold_bond_id[(slot, holder_id)] = 0;
                g.hold_bond_face[(slot, holder_id)] = 0;
                g.bond_hold_cost[(slot, holder_id)] = Ext::from_f64(0.0);
            } else {
                let cost_removed =
                    g.bond_hold_cost[(slot, holder_id)] * (sf(0.01f32) * cast::<Ext, _>(call_pct));
                gain = payout - cost_removed;
                g.bond_hold_cost[(slot, holder_id)] =
                    g.bond_hold_cost[(slot, holder_id)] - cost_removed;
                g.hold_bond_face[(slot, holder_id)] =
                    to_quad(cast::<Ext, _>(g.hold_bond_face[(slot, holder_id)]) - cast::<Ext, _>(holder_face_called));
            }
            if g.num_players >= holder_id {
                g.player_cap_gains[holder_id] = gain + g.player_cap_gains[holder_id];
                g.news_msg = g.ent_symbol[id].clone()
                    + " calls"
                    + &str15(cast::<Ext, _>(call_pct))
                    + __content.string(10732)
                    + &g.ent_name[holder_id]
                    + " at"
                    + &str15(cast::<Ext, _>(call_price))
                    + ".";
                post_news(g, ui, 0, 0);
            } else if holder_id > 85 {
                g.extraordinary_pl[holder_id] =
                    to_quad(gain + cast::<Ext, _>(g.extraordinary_pl[holder_id]));
            } else {
                g.accrued_pl[holder_id] = gain + g.accrued_pl[holder_id];
            }
        }
    }
    let mut msg_text = right(g.bond_issue_info[id].as_bytes(), 5);
    if !(val(msg_text.as_bytes()) >= Ext::from_f64(10.0)) {
        msg_text = right(msg_text.as_bytes(), 4);
    }
    let mut pct_text = str15(cast::<Ext, _>(call_pct)) + "%";
    if call_pct == 100 {
        pct_text = s(" all");
    }
    g.news_msg = g.ent_symbol[id].clone()
        + " calls"
        + &pct_text
        + __content.string(3107)
        + &msg_text
        + __content.string(10733)
        + &mid(g.bond_issue_info[id].as_bytes(), 1, 4)
        + __content.string(10734)
        + &str15(cast::<Ext, _>(call_price))
        + ".";
    if is_in_recent_headlines(g, __content.bytes(10735)) == 0
        || mode == 0
        || g.cur_player == g.corp_controller[id]
    {
        post_news(g, ui, 0, 0);
    } else {
        g.news_msg = PbStr::new();
    }
    if mode == 0 {
        let cs = trim(g.currency_symbol.as_bytes());
        let msg = s(__content.string(10736))
            + "\r"
            + "\r"
            + &g.ent_name[id]
            + __content.string(10737)
            + &cs
            + &fm(cast::<Ext, _>(face_called), "* ###,")
            + " "
            + &g.million_prefix_lc
            + __content.string(10738)
            + __content.string(10739)
            + &str15(cast::<Ext, _>(call_price))
            + __content.string(10740)
            + __content.string(10741)
            + &cs
            + &fm(accrued_int, "* ###,.00")
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + ".";
        ui.msgbox(&msg, 0x2000, &s(__content.string(10742)));
        g.turn_action_count += 1;
        if g.industry_num[id] == 71 && 0 < g.fund_bonus_year[id] {
            g.fund_bonus_year[id] = g.cur_year;
        }
    }
    g.cash[id] = g.cash[id] - call_cost;
    g.bonds_issued[id] = to_quad(cast::<Ext, _>(g.bonds_issued[id]) - cast::<Ext, _>(face_called));
    g.accrued_pl[id] = g.accrued_pl[id] - (call_cost - cast::<Ext, _>(face_called));
    g.accrued_pl[id] = g.accrued_pl[id] - accrued_int;
    g.cash[id] = g.cash[id] - accrued_int;
    if call_pct == 100 {
        g.bonds_issued[id] = 0;
        g.bond_issue_info[id] = PbStr::new();
        g.bond_price[id] = Cur(0);
    } else {
        calc_entity_financials(g, ui, rng, id);
        while (cast::<Ext, _>(g.bonds_public_held)) < Ext::from_f64(0.0) {
            g.bonds_issued[id] = to_quad(Ext::from_f64(1.0) + cast::<Ext, _>(g.bonds_issued[id]));
            g.bonds_public_held = to_quad(Ext::from_f64(1.0) + cast::<Ext, _>(g.bonds_public_held));
        }
    }
}

/// Port of ConvertOrRedeemConvertibles (0x686c0f).
/// For issuer `corp_id` with bonds outstanding: non-convertible bonds are called in full ([`call_bonds`],
/// mode 2) unless `convert_only = 1`. Convertibles (`"cv"`) maturing within 9 years (or any if
/// `call_mode <> 2`) are converted when the conversion value (share price x shares per 1,000 bond / 10)
/// exceeds the call price (100 for `convert_only = 1`, which also requires the market cap to exceed the
/// conversion cap): each bondholder (players, banks, insurers, funds) either receives shares (new % added
/// to its holding, basis = bond cost) or, if its share would stay below 1.5% or it cannot hold the stock,
/// is cashed out at the conversion value (gain to cap gains / AccruedPL / ExtraordinaryPL). Then the issue
/// is retired, shares outstanding grow, every holding is diluted and the percentages normalised. Otherwise
/// (and `convert_only <> 1`) the bonds are called. Returns 1 if the bonds were converted or fully called.
pub fn convert_or_redeem_convertibles(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    corp_id: i32,
    convert_only: i32,
    call_mode: i32,
) -> i32 {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut result = 0;
    let issuer_id = corp_id;
    let id = issuer_id;
    if g.bonds_issued[id] == 0 || g.entity_status[id] != 1 {
        return result;
    }
    if instr(1, g.bond_issue_info[id].as_bytes(), b"cv") == 0 {
        if convert_only != 1 {
            g.tmp_ext1 = Ext::from_f64(100.0);
            g.tmp_ext2 = Ext::from_f64(2.0);
            call_bonds(g, ui, rng, id);
            if g.bonds_issued[id] == 0 {
                result = 1;
            }
        }
        return result;
    }
    let mut years_left = to_long(val(mid(g.bond_issue_info[id].as_bytes(), 1, 4).as_bytes()));
    years_left = years_left - g.cur_year;
    if 9 < years_left && call_mode == 2 {
        return result;
    }
    post_news(g, ui, 0, 0);
    calc_entity_financials(g, ui, rng, id);
    update_corp_control(g, ui, rng, id);
    let mut private_flag = 0;
    if g.bonds_public_held == 0 && g.public_pct == 0 {
        private_flag = 1;
    }
    let share_price = g.market_cap[id] / g.shares_outstanding[id];
    let mut info_text = g.bond_issue_info[id].clone();
    let mut conv_price = val(parse_any(info_text.as_bytes(), b"|", 2).as_bytes());
    if !(conv_price < g.market_cap[id]) && convert_only == 1 {
        return result;
    }
    conv_price = conv_price / g.shares_outstanding[id];
    let shares_per_bond: Ext = Ext::from_f64(1000.0) / conv_price;
    let conv_value = share_price * shares_per_bond * sf(0.1f32);
    let years_to_maturity =
        to_long(val(mid(g.bond_issue_info[id].as_bytes(), 1, 4).as_bytes()) - cast::<Ext, _>(g.cur_year));
    let call_price_pct = if convert_only == 1 {
        100
    } else {
        calc_bond_call_price_pct(years_to_maturity)
    };
    if !(conv_value > cast::<Ext, _>(call_price_pct)) {
        if convert_only != 1 {
            g.tmp_ext1 = Ext::from_f64(100.0);
            g.tmp_ext2 = Ext::from_f64(2.0);
            call_bonds(g, ui, rng, id);
            if g.bonds_issued[id] == 0 {
                result = 1;
            }
        }
        return result;
    }
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    let new_shares: i64 = to_quad(shares_per_bond * cast::<Ext, _>(g.bonds_issued[id]) * Ext::from_f64(1000.0));
    let old_shares: i64 = to_quad(Ext::from_f64(100_000_000.0) * g.shares_outstanding[id]);
    for holder_id in 1..=1583 {
        if 130 < holder_id && holder_id < 1581 {
            continue;
        }
        let mut cashed_out = 0;
        if holder_id < 11 && g.num_players < holder_id {
            continue;
        }
        if g.entity_status[holder_id] != 1 || g.hold_bond_id[(0, holder_id)] == 0 {
            continue;
        }
        let bond_slot = find_bond_holding_slot(g, holder_id, id);
        if bond_slot == 0 || id != g.hold_bond_id[(bond_slot, holder_id)] {
            continue;
        }
        let mut holder_value =
            shares_per_bond * cast::<Ext, _>(g.hold_bond_face[(bond_slot, holder_id)]) * Ext::from_f64(1000.0);
        let new_pct_frac: Ext = holder_value / cast::<Ext, _>(old_shares);
        let hold_slot = find_stock_holding(g, holder_id, id);
        if 0 < hold_slot {
            holder_value = g.shares_outstanding[id]
                * cast::<Ext, _>(g.hold_stock_pct[(hold_slot, holder_id)])
                * Ext::from_f64(1_000_000.0)
                + holder_value;
        }
        let holder_share: Ext = holder_value / (cast::<Ext, _>(new_shares) + cast::<Ext, _>(old_shares));
        let cash_out = sf(0.015f32) > holder_share
            || find_stock_holding(g, holder_id, id) < 0
            || find_stock_holding(g, id, holder_id) > 0
            || find_holding_slot(g, holder_id, id) == 0;
        if cash_out {
            let cash_paid = sf(0.01f32) * conv_value * cast::<Ext, _>(g.hold_bond_face[(bond_slot, holder_id)]);
            g.cash[holder_id] = cash_paid + g.cash[holder_id];
            let gain = cash_paid - g.bond_hold_cost[(bond_slot, holder_id)];
            if holder_id < 11 {
                g.player_cap_gains[holder_id] = gain + g.player_cap_gains[holder_id];
            } else if holder_id < 86 {
                g.accrued_pl[holder_id] = gain + g.accrued_pl[holder_id];
            } else {
                g.extraordinary_pl[holder_id] =
                    to_quad(gain + cast::<Ext, _>(g.extraordinary_pl[holder_id]));
            }
            cashed_out = 1;
            private_flag = 0;
        } else {
            let pct_to_add = to_long(Ext::from_f64(100.0) * new_pct_frac);
            let mut stock_slot = find_stock_holding(g, holder_id, id);
            if stock_slot == 0 {
                stock_slot = find_holding_slot(g, holder_id, id);
                g.hold_stock_id[(stock_slot, holder_id)] = cast::<i16, _>(id);
            }
            let p = pct_to_add.wrapping_add(cast::<i32, _>(g.hold_stock_pct[(stock_slot, holder_id)]));
            g.hold_stock_pct[(stock_slot, holder_id)] = cast::<i16, _>(p);
            g.hold_stock_basis[(stock_slot, holder_id)] = g.bond_hold_cost[(bond_slot, holder_id)]
                + g.hold_stock_basis[(stock_slot, holder_id)];
        }
        g.hold_bond_id[(0, holder_id)] -= 1;
        g.hold_bond_id[(bond_slot, holder_id)] = 0;
        g.hold_bond_face[(bond_slot, holder_id)] = 0;
        g.bond_hold_cost[(bond_slot, holder_id)] = Ext::from_f64(0.0);
        if get_controller_type(g, holder_id) == 1
            || g.sel_entity == holder_id
            || g.last_sel_entity == holder_id
        {
            if cashed_out == 1 {
                if convert_only == 1 {
                    g.news_msg = g.ent_symbol[holder_id].clone()
                        + __content.string(10743)
                        + &g.ent_symbol[id]
                        + __content.string(10744);
                } else {
                    g.news_msg = g.ent_symbol[holder_id].clone()
                        + __content.string(10745)
                        + &g.ent_symbol[id]
                        + __content.string(10746);
                }
            } else {
                g.news_msg = g.ent_symbol[holder_id].clone()
                    + __content.string(10747)
                    + &g.ent_symbol[id]
                    + __content.string(10748);
            }
            post_news(g, ui, holder_id, 0);
        }
    }
    info_text = g.ent_name[id].clone();
    if len(info_text.as_bytes()) > 20 {
        info_text = g.ent_symbol[id].clone();
    }
    g.news_msg =
        s("All ") + &g.ent_symbol[id] + __content.string(10749) + &info_text + " stock";
    post_news(g, ui, id, 0);
    if conv_value > Ext::from_f64(1000.0) {
        g.news_msg = s(__content.string(9428))
            + &g.ent_symbol[id]
            + __content.string(10750);
        if g.cur_player == g.corp_controller[id] {
            let mut msg = g.news_msg.clone()
                + __content.string(10751)
                + __content.string(10752)
                + &g.stock_price_str
                + " "
                + __content.string(10753);
            show_quick_message(g, ui, &mut msg);
        }
    }
    post_news(g, ui, 0, 0);
    let mut share_ratio: Ext = cast::<Ext, _>(new_shares) / cast::<Ext, _>(old_shares);
    let dilution_pct = to_long(Ext::from_f64(100.0) * share_ratio);
    adjust_index_divisor(g, id, dilution_pct);
    share_ratio = Ext::from_f64(1.0) + share_ratio;
    g.shares_outstanding[id] = share_ratio * g.shares_outstanding[id];
    g.bonds_issued[id] = 0;
    g.bond_issue_info[id] = PbStr::new();
    g.bond_price[id] = Cur(0);
    if Ext::from_f64(0.0) < share_ratio {
        let dilution_factor: Ext = Ext::from_f64(1.0) / share_ratio;
        for holder_id in 1..=1600 {
            if holder_id < 11 && g.num_players < holder_id {
                continue;
            }
            if g.entity_status[holder_id] != 1 || find_stock_holding(g, holder_id, id) == 0 {
                continue;
            }
            let hold_slot = find_holding_slot(g, holder_id, id);
            if hold_slot == 0 || cast::<i32, _>(g.hold_stock_id[(hold_slot, holder_id)]) != id {
                continue;
            }
            let new_pct =
                to_quad(dilution_factor * cast::<Ext, _>(g.hold_stock_pct[(hold_slot, holder_id)]));
            g.hold_stock_pct[(hold_slot, holder_id)] = cast::<i16, _>(new_pct);
            if g.hold_stock_pct[(hold_slot, holder_id)] == 0 {
                g.hold_stock_pct[(hold_slot, holder_id)] = 1;
            }
        }
    }
    normalize_holding_pcts(g, ui, rng, id, private_flag);
    1
}

/// Port of NormalizeHoldingPcts (0x5212b0).
/// Up to 100 passes: re-evaluates control ([`update_corp_control`]) and sums all non-negative holdings
/// of `corp_id`; while the sum exceeds 100 (or differs from 100 with `force_to_100 <> 0`) the largest
/// holding (first found) is moved one point towards 100.
pub fn normalize_holding_pcts(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    corp_id: i32,
    force_to_100: i32,
) {
    let mut adjust: i32 = 0;
    for _pass in 0..=99 {
        update_corp_control(g, ui, rng, corp_id);
        let mut total_pct: i32 = 0;
        let mut max_pct: i32 = 0;
        let mut max_slot: i16 = 0;
        let mut max_holder: i32 = 0;
        for holder_id in 1..=1600 {
            for slot in 1..=15 {
                if cast::<i32, _>(g.hold_stock_id[(slot, holder_id)]) == corp_id
                    && g.hold_stock_pct[(slot, holder_id)] >= 0
                {
                    let p = cast::<i32, _>(g.hold_stock_pct[(slot, holder_id)]);
                    total_pct += p;
                    if p > max_pct {
                        max_pct = p;
                        max_slot = cast::<i16, _>(slot);
                        max_holder = holder_id;
                    }
                }
            }
        }
        if total_pct <= 100 && force_to_100 == 0 {
            return;
        }
        if 100 < total_pct {
            adjust = -1;
        }
        if total_pct == 100 {
            return;
        }
        if total_pct < 100 {
            adjust = 1;
        }
        if 0 < max_holder {
            let p = adjust.wrapping_add(cast::<i32, _>(g.hold_stock_pct[(cast::<i32, _>(max_slot), max_holder)]));
            g.hold_stock_pct[(cast::<i32, _>(max_slot), max_holder)] = cast::<i16, _>(p);
        }
    }
}

/// Port of IssueBonds (0x5231bc).
/// Corp `corp_id` (with no bonds outstanding) issues bonds of face `TmpExt1` (cleared; 0 = automatic:
/// 30% (15% for very large corps) of net worth, rounded up to 100). `requested_rate` > 0 /
/// `requested_maturity` > 0 fix the coupon / maturity year (player orders); with `requested_rate = 0`
/// (AI) the issue is skipped when more than ~600..800 corps have bonds out, up to 75% of the bank debt is
/// paid down from the proceeds and the credit rating is re-evaluated first. Coupon = (spread + BorrowRate)
/// rounded to 1/20; the issue may become a convertible (higher price momentum / no earnings / few
/// convertibles out; lower coupon, conversion cap 11..20% above the market cap, size limited to the
/// share count). Coupons above 15% abort AI issues. Sets `BondIssueInfo`, `BondsIssued`, cash (99.5% of
/// face), posts news and nudges `InterestRateTarget`. Returns 0 (the result variable is never set).
pub fn issue_bonds(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    corp_id: i32,
    requested_rate: Ext,
    requested_maturity: i32,
) -> i32 {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let id = corp_id;
    let result = 0;
    if requested_rate == Ext::from_f64(0.0) {
        let thr = rng.rnd() * Ext::from_f64(200.0) + Ext::from_f64(600.0);
        if cast::<Ext, _>(count_bond_issuers(g)) > thr {
            return result;
        }
    }
    calc_entity_financials(g, ui, rng, id);
    let mut bond_amt: i64 = to_quad(g.tmp_ext1);
    g.tmp_ext1 = Ext::from_f64(0.0);
    if g.bonds_issued[id] != 0 {
        return result;
    }
    let mut auto_amount = 0;
    if bond_amt == 0 {
        bond_amt = to_quad(g.net_worth * sf(0.3f32));
        if g.total_assets > g.exchange_rate * Ext::from_f64(500000.0) {
            bond_amt = to_quad(g.net_worth * sf(0.15f32));
        }
        let mut rounded_amt = cast::<Ext, _>(bond_amt) * sf(0.01f32);
        rounded_amt = rounded_amt.ceil();
        bond_amt = to_quad(rounded_amt * Ext::from_f64(100.0));
        auto_amount = 1;
    }
    if !(cast::<Ext, _>(bond_amt) >= g.exchange_rate * Ext::from_f64(50.0)) {
        return result;
    }
    if bond_amt > g.max_bond_issue {
        bond_amt = g.max_bond_issue;
    }
    // -0x94: maturity year, later reused for the coupon x 20
    let mut slot94: i32 = 5 + g.cur_year + g.credit_rating_num[id];
    if cast::<Ext, _>(bond_amt) > g.exchange_rate * Ext::from_f64(5000.0) {
        slot94 += g.credit_rating_num[id];
    }
    if requested_maturity > 0 {
        slot94 = requested_maturity;
    }
    let mut maturity_text = trim(str_num(cast::<Ext, _>(slot94), 15).as_bytes());
    let mut rate_spread: Ext = if g.industry_num[id] == 1 { Ext::from_f64(1.25) } else { Ext::from_f64(1.0) };
    if val(maturity_text.as_bytes()) - (cast::<Ext, _>(g.cur_year)) < Ext::from_f64(10.0) {
        rate_spread = rate_spread - Ext::from_f64(0.25);
    }
    let mut debt_paydown: Ext = Ext::from_f64(0.0);
    if requested_rate == Ext::from_f64(0.0) {
        debt_paydown = g.bank_debt[id] * sf(0.75f32);
        if debt_paydown > cast::<Ext, _>(bond_amt) * sf(0.75f32) {
            debt_paydown = cast::<Ext, _>(bond_amt) * sf(0.75f32);
        }
        g.tmp_total_debt = (g.tmp_total_debt + cast::<Ext, _>(bond_amt)) - debt_paydown;
        if g.tmp_total_debt < Ext::from_f64(0.0) {
            g.tmp_total_debt = Ext::from_f64(0.0);
        }
        calc_credit_rating(g, id);
    }
    slot94 = to_long((rate_spread + cast::<Ext, _>(g.borrow_rate)) * Ext::from_f64(20.0));
    if requested_rate > Ext::from_f64(0.0) {
        slot94 = to_long(Ext::from_f64(20.0) * requested_rate);
    }
    let mut coupon_rate: Ext = cast::<Ext, _>(slot94) / Ext::from_f64(20.0);
    let mut want_convertible = 0;
    if price_pct_change_months(g, id, 6) >= 15 && rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) {
        want_convertible = 1;
    }
    if g.earnings_y1[id] == Ext::from_f64(0.0) && g.earnings_yr2_ago[id] == Ext::from_f64(0.0) && rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
        want_convertible = 1;
    }
    if price_trend_streak(g, id) > 0 && count_convertible_issuers(g) < 160 && rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0)
    {
        want_convertible = 1;
    }
    if g.industry_num[id] == 71 || g.credit_rating_num[id] < 3 || !(g.proj_qtr_earnings[id] > Ext::from_f64(0.0)) {
        want_convertible = 0;
    }
    if auto_amount == 1 && g.industry_num[id] != 71 && count_convertible_issuers(g) < 200 {
        want_convertible = 1;
    }
    if count_convertible_issuers(g) > 170 && rng.rnd() * Ext::from_f64(10.0) > Ext::from_f64(1.0) {
        want_convertible = 0;
    }
    if requested_rate != Ext::from_f64(0.0) {
        want_convertible = 0;
    }
    if g.shares_outstanding[id] > Ext::from_f64(0.0) {
        let thr = rng.rnd() * Ext::from_f64(10.0) + Ext::from_f64(15.0);
        if !(g.market_cap[id] / g.shares_outstanding[id] >= thr) {
            want_convertible = 0;
        }
    }
    let mut is_convertible = 0;
    let mut conv_price: Ext = Ext::from_f64(0.0);
    if want_convertible == 1 {
        is_convertible = 1;
        if g.start_year < g.cur_year && count_convertible_issuers(g) > 145 {
            if g.credit_rating_num[id] > 8
                && rng.rnd() * Ext::from_f64(4.0) > Ext::from_f64(1.0)
                && Ext::from_f64(6.0) > cast::<Ext, _>(g.base_interest_rate)
            {
                is_convertible = 0;
            }
            if g.credit_rating_num[id] == 8
                && rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0)
                && Ext::from_f64(6.0) > cast::<Ext, _>(g.base_interest_rate)
            {
                is_convertible = 0;
            }
            if auto_amount == 1 {
                is_convertible = 1;
            }
        }
        if is_convertible == 1 {
            slot94 = g.credit_rating_num[id] + g.cur_year + 3;
            if g.credit_rating_num[id] == 10 {
                slot94 = 15 + g.cur_year;
            }
            if 3 + g.start_year > g.cur_year && rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                let base = 2 + g.cur_year;
                slot94 = rng.rnd_range(1, 5) + base;
            }
            let mut rate_x20 = to_long(Ext::from_f64(20.0) * coupon_rate);
            let r = cast::<Ext, _>(rate_x20) * sf(0.6f32);
            rate_x20 = to_long(r * (rng.rnd() * sf(0.2f32) + sf(0.9f32)));
            if g.credit_rating_num[id] > 6 && rate_x20 > 50 {
                rate_x20 = rate_x20 - 20;
            }
            coupon_rate = cast::<Ext, _>(rate_x20) / Ext::from_f64(20.0);
            if Ext::from_f64(5.0) > coupon_rate && slot94 > 8 {
                slot94 = to_long(cast::<Ext, _>(slot94) - (sf(5.5f32) - coupon_rate));
                if slot94 < 5 {
                    slot94 = 5;
                }
            }
            maturity_text = trim(str_num(cast::<Ext, _>(slot94), 15).as_bytes());
            let max_conv_shares: i64 = to_quad(g.shares_outstanding[id] * Ext::from_f64(10_000_000.0));
            let mut conv_premium: Ext = Ext::from_f64(1.0) + sf(0.01f32) * (cast::<Ext, _>(g.credit_rating_num[id]) + Ext::from_f64(11.0));
            if !(sf(1.2f32) >= conv_premium) {
                conv_premium = sf(1.2f32);
            }
            conv_price = g.market_cap[id] * conv_premium;
            let shares_per_bond: Ext = Ext::from_f64(1000.0) / (conv_price / g.shares_outstanding[id]);
            let conv_shares: i64 = to_quad(cast::<Ext, _>(bond_amt) * Ext::from_f64(1000.0) * shares_per_bond);
            if conv_shares > max_conv_shares {
                let shrink_factor: Ext = cast::<Ext, _>(max_conv_shares) / cast::<Ext, _>(conv_shares);
                bond_amt = to_quad(cast::<Ext, _>(bond_amt) * shrink_factor);
            }
            if !(cast::<Ext, _>(bond_amt) >= g.exchange_rate * Ext::from_f64(100.0)) {
                return result;
            }
        }
    }
    if !(Ext::from_f64(15.0) >= coupon_rate) && requested_rate == Ext::from_f64(0.0) {
        return result;
    }
    g.bank_debt[id] = g.bank_debt[id] - debt_paydown;
    g.cash[id] = g.cash[id] - debt_paydown;
    let l = g.lender_id;
    g.cash[l] = debt_paydown + g.cash[l];
    let mut rate_text = fm(coupon_rate, "0#.00");
    if is_convertible == 1 {
        g.bond_issue_info[id] =
            maturity_text.clone() + "cv|" + &fm(conv_price, "* ###.00") + "|@" + &rate_text;
    } else {
        g.bond_issue_info[id] = maturity_text.clone() + "@" + &rate_text;
    }
    g.bonds_issued[id] = bond_amt;
    g.cash[id] = sf(0.995f32) * cast::<Ext, _>(bond_amt) + g.cash[id];
    if !(val(rate_text.as_bytes()) >= Ext::from_f64(10.0)) {
        rate_text = right(rate_text.as_bytes(), 4);
    }
    let mut news_suffix = PbStr::new();
    if len(g.ent_name[id].as_bytes()) < 11 && instr(1, g.ent_name[id].as_bytes(), b" ") == 0 {
        g.news_msg = g.ent_name[id].clone();
        if len(g.news_msg.as_bytes()) > 7 {
            g.news_msg = mcase(g.ent_name[id].as_bytes(), CaseMode::Ansi);
            if instr(1, g.news_msg.as_bytes(), b"'S") > 0 {
                g.news_msg = replace(g.news_msg.as_bytes(), b"'S", b"'s");
            }
        }
        g.news_msg = g.news_msg.clone() + " (" + &g.ent_symbol[id] + ")";
    } else {
        g.news_msg = g.ent_symbol[id].clone();
        if debt_paydown > g.exchange_rate * Ext::from_f64(100.0) && is_convertible == 0 && rng.rnd() * Ext::from_f64(4.0) > Ext::from_f64(3.0) {
            news_suffix = s(__content.string(10754));
            if g.bank_debt[id] == Ext::from_f64(0.0) {
                let lb = cast::<i32, _>(g.lending_bank[id]);
                news_suffix = s(__content.string(10755)) + &g.ent_symbol[lb];
            }
        }
    }
    if news_suffix.is_empty() {
        news_suffix = s(__content.string(10756)) + &maturity_text;
    }
    if is_convertible == 1 && instr(1, g.news_msg.as_bytes(), b"(") > 0 {
        news_suffix = PbStr::new();
    }
    g.news_msg = g.news_msg.clone()
        + __content.string(3114)
        + &g.currency_symbol
        + &fm(cast::<Ext, _>(bond_amt), " * ###,")
        + " "
        + &g.million_prefix_lc
        + "il. of "
        + &rate_text
        + "% bonds"
        + &news_suffix;
    if is_convertible == 1 {
        g.news_msg = replace(g.news_msg.as_bytes(), b"% bonds", __content.bytes(10757));
    }
    let cp = g.cur_player;
    if find_stock_holding(g, cp, id) != 0 {
        post_news(g, ui, id, 0);
    } else if is_in_recent_headlines(g, __content.bytes(10758)) == 1 && rng.rnd() * Ext::from_f64(10.0) > Ext::from_f64(1.0) {
        g.news_msg = PbStr::new();
    } else {
        post_news(g, ui, id, 0);
    }
    let mut cr: Ext = sf(0.01f32) * (cast::<Ext, _>(bond_amt) / Ext::from_f64(1000.0));
    if !(sf(0.05f32) >= cr) {
        cr = sf(0.05f32);
    }
    g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(cr + cast::<Ext, _>(g.interest_rate_target)));
    result
}

/// `RecaptureExcessLoss(corpId, holderId)` (0x499b0b) with its news: the state part is
/// [`recapture_excess_loss`]; each recapture then posts the original's two headlines
/// (`PostNews(ownerId, corpId)` after each). The news is posted after the scan; the scan does not read
/// anything PostNews changes.
fn recapture_excess_loss_news(g: &mut Globals, ui: &mut dyn Ui, corp_id: i32, holder_id: i32) {
    let __content = crate::content::current();
    let mut events = Vec::new();
    recapture_excess_loss(g, corp_id, holder_id, &mut events);
    for ev in events {
        let PackEvent::ExcessLossRecaptured {
            owner_id,
            corp_id,
            amount,
        } = ev;
        g.news_msg = s(__content.string(5698))
            + &g.ent_symbol[owner_id]
            + __content.string(5699)
            + &g.ent_symbol[corp_id]
            + __content.string(5700);
        post_news(g, ui, owner_id, corp_id);
        g.news_msg = g.ent_symbol[owner_id].clone()
            + __content.string(5701)
            + &trim(g.currency_symbol.as_bytes())
            + &fm(amount, "* ###,")
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix;
        post_news(g, ui, owner_id, corp_id);
    }
}

/// Port of ScaleEarningsHistory (0x54e63b).
/// After a stock issue of `pct_change` percent (|pct| <= 99, nonzero) scales the earnings history,
/// projected earnings and prior net worth of active corp `corp_id` by `(pct + 100)%` (a SINGLE factor),
/// and the first five fields of its earnings record `EntMiscData` (if it has >= 32 fields; the record
/// is rebuilt with 41 fields). Nothing happens for records shorter than 21 characters.
pub fn scale_earnings_history(g: &mut Globals, corp_id: i32, pct_change: i32) {
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let c = corp_id;
    if c < 11 || pct_change == 0 || (cast::<Ext, _>(pct_change)).abs() > Ext::from_f64(99.0) || g.entity_status[c] != 1 {
        return;
    }
    if len(g.ent_misc_data[c].as_bytes()) < 21 {
        return;
    }
    let factor = cast::<Ext, _>(cast::<f32, _>((cast::<Ext, _>(pct_change) + Ext::from_f64(100.0)) * sf(0.01f32)));
    g.earnings_y1[c] = factor * g.earnings_y1[c];
    g.earnings_yr2_ago[c] = factor * g.earnings_yr2_ago[c];
    g.earnings_yr3_ago[c] = factor * g.earnings_yr3_ago[c];
    g.earnings_yr4_ago[c] = factor * g.earnings_yr4_ago[c];
    g.earnings_ytd[c] = factor * g.earnings_ytd[c];
    g.proj_qtr_earnings[c] = factor * g.proj_qtr_earnings[c];
    g.corp_prior_net_worth[c] = factor * g.corp_prior_net_worth[c];
    let field_count = parse_count(g.ent_misc_data[c].as_bytes(), b"|");
    if field_count < 32 {
        return;
    }
    let mut new_misc = PbStr::new();
    for field_idx in 1..=41 {
        let mut field = parse_any(g.ent_misc_data[c].as_bytes(), b"|", field_idx);
        if field_idx < 6 {
            // EXT local: VAL in 80 bits times the SINGLE factor, formatted from 80 bits (an f64
            // intermediate can move a value off/onto a x.xx5 tie)
            let scaled = Ext80::from_ext(factor).fmul(val_ext(field.as_bytes()));
            field = trim(&format_ext(scaled, b"##############.00"));
        }
        new_misc = new_misc + &field + "|";
    }
    g.ent_misc_data[c] = new_misc;
}

/// Port of AdjustConversionPrice (0x6890c5).
/// For a convertible issue of `corp_id`: moves the conversion cap (field 2 of `BondIssueInfo`) by
/// `cap_delta / 100` (if `pct_change = 0`) or scales it by `(pct_change + 100)%`, floor 0.05, and rebuilds
/// the info as `MID$(info, 1, 4) + "cv|" + cap + "|@" + RIGHT$(info, 5)`.
pub fn adjust_conversion_price(g: &mut Globals, corp_id: i32, cap_delta: Ext, pct_change: Ext) {
    let issuer_id = corp_id;
    let cap_delta = sf(0.01f32) * cap_delta;
    let bond_info = g.bond_issue_info[corp_id].clone();
    if !(cast::<Ext, _>(g.bonds_issued[issuer_id]) > Ext::from_f64(0.0))
        || instr(1, g.bond_issue_info[issuer_id].as_bytes(), b"cv") <= 0
    {
        return;
    }
    let mut conv_value = val(parse_any(bond_info.as_bytes(), b"|", 2).as_bytes());
    if pct_change == Ext::from_f64(0.0) {
        conv_value = cap_delta + conv_value;
    } else {
        conv_value = sf(0.01f32) * (pct_change + Ext::from_f64(100.0)) * conv_value;
    }
    if !(conv_value >= sf(0.05f32)) {
        conv_value = sf(0.05f32);
    }
    let maturity_part = mid(bond_info.as_bytes(), 1, 4);
    let coupon_part = right(bond_info.as_bytes(), 5);
    g.bond_issue_info[issuer_id] =
        maturity_part + "cv|" + &fm(conv_value, "* ###.00") + "|@" + &coupon_part;
}

/// Port of AdjustPriceAlertsForStock (0x6845d0).
/// Moves the target value (field 4) of every price alert on `stock_id` by `cap_delta / 100` (if
/// `pct_change = 0`) or scales it by `(pct_change + 100)%`; the record is rebuilt with `STR$` values.
pub fn adjust_price_alerts_for_stock(
    g: &mut Globals,
    stock_id: i32,
    cap_delta: Ext,
    pct_change: Ext,
) {
    let corp_id = stock_id;
    let cap_delta = sf(0.01f32) * cap_delta;
    if val(g.price_alerts[0].as_bytes()) == Ext::from_f64(0.0) {
        return;
    }
    let sep = s("|");
    let n = to_long(val(g.price_alerts[0].as_bytes()));
    let mut slot = 1;
    while slot <= n {
        let alert_rec = g.price_alerts[slot].clone();
        let a = alert_rec.as_bytes();
        if !alert_rec.is_empty() && val(parse_any(a, b"|", 1).as_bytes()) == cast::<Ext, _>(corp_id) {
            let fld2 = parse_any(a, b"|", 2) + &sep;
            let fld3 = parse_any(a, b"|", 3) + &sep;
            let mut target_value = val(parse_any(a, b"|", 4).as_bytes());
            if pct_change == Ext::from_f64(0.0) {
                target_value = cap_delta + target_value;
            } else {
                target_value = sf(0.01f32) * (pct_change + Ext::from_f64(100.0)) * target_value;
            }
            let fld5 = parse_any(a, b"|", 5) + &sep;
            let fld6 = parse_any(a, b"|", 6) + &sep;
            let fld7 = parse_any(a, b"|", 7);
            g.price_alerts[slot] = trim(str_num(cast::<Ext, _>(corp_id), 15).as_bytes())
                + &sep
                + &fld2
                + &fld3
                + &trim(str_num(target_value, 15).as_bytes())
                + &sep
                + &fld5
                + &fld6
                + &fld7;
        }
        slot += 1;
    }
}

/// Port of ForceRegulatedCallSales (0x5f8e12).
/// Banks and insurers (industry < 3) may not own call options: every long call record (type 1,
/// contracts >= 0) of such a holder is closed (`CloseOptionPositions(holder, corp, 1, 2)` until
/// `FindOptionPositions(holder, corp, 1, 0)` finds none, max 1,000 rounds) with a regulator headline.
/// Other corporate option positions are checked for excess naked options (once per holder/corp pair
/// in a row, `logic::etf_derivs::corp_options::force_close_excess_naked_options`).
pub fn force_regulated_call_sales(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut last_holder = 0;
    let mut last_corp = 0;
    for rec_idx in 1..=1000 {
        let holder_id = to_long(val(
            parse_any(g.option_recs[rec_idx].as_bytes(), b"|", 3).as_bytes()
        ));
        if holder_id < 11 {
            continue;
        }
        let corp_id = to_long(val(
            parse_any(g.option_recs[rec_idx].as_bytes(), b"|", 1).as_bytes()
        ));
        if val(parse_any(g.option_recs[rec_idx].as_bytes(), b"|", 2).as_bytes()) == Ext::from_f64(1.0)
            && !(val(parse_any(g.option_recs[rec_idx].as_bytes(), b"|", 4).as_bytes()) < Ext::from_f64(0.0))
        {
            if g.industry_num[holder_id] < 3 {
                let mut pos_code = 1;
                let mut iter = 0;
                while pos_code > 0 {
                    g.tmp_ext1 = Ext::from_f64(0.0);
                    g.tmp_ext2 = Ext::from_f64(0.0);
                    close_option_positions(g, ui, rng, holder_id, corp_id, 1, 2);
                    iter += 1;
                    if iter > 1000 {
                        break;
                    }
                    pos_code = find_option_positions(g, ui, holder_id, corp_id, 1, 0);
                }
                if iter <= 1000 {
                    let mut regulator = s("Bank");
                    if g.industry_num[holder_id] == 2 {
                        regulator = s(__content.string(2878));
                    }
                    g.news_msg = regulator
                        + __content.string(10759)
                        + &g.ent_symbol[holder_id]
                        + __content.string(10760)
                        + &g.ent_symbol[corp_id];
                    post_news(g, ui, 0, 0);
                }
            }
            continue;
        }
        if val(parse_any(g.option_recs[rec_idx].as_bytes(), b"|", 2).as_bytes()) == Ext::from_f64(2.0)
            && val(parse_any(g.option_recs[rec_idx].as_bytes(), b"|", 4).as_bytes()) < Ext::from_f64(0.0)
        {
            continue;
        }
        if last_holder != holder_id || last_corp != corp_id {
            if holder_id != last_holder {
                last_holder = holder_id;
            }
            if corp_id != last_corp {
                last_corp = corp_id;
            }
            crate::logic::etf_derivs::corp_options::force_close_excess_naked_options(
                g, ui, rng, holder_id, corp_id,
            );
        }
    }
}

/// Port of IssueStock (0x5223a3).
/// Corp `corp_id` (11..1600, active) sells `TmpExt1` percent (5..49; cleared) new shares at its market
/// cap: `offer_type` 1 = public offering / IPO, 2 = private placement. A bank [runtime resource wording]
/// may only do a public offering when undercapitalised (net worth + reserve <= 4% of deposits); a public
/// offering above `MaxStockOfferingAmt` by a corp not controlled by a corp (and not the current player's)
/// is cancelled with news. Adds the proceeds to cash, sets `NewsMsg` (posted for adviser-run funds;
/// cleared for tiny offerings or non-player companies, else left for the caller), lowers investor
/// sentiment (IPO: +25 - RND(1, 30)), scales shares, options, conversion caps and price alerts, dilutes
/// every holding (holdings rounding to 0 are cashed out at the old value), scales the market cap (IPO:
/// x1.05..1.25 bonus), index divisor, earnings and price history, re-evaluates the control chain,
/// recaptures excess losses and enforces the bank/insurer call-option rule.
pub fn issue_stock(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, corp_id: i32, offer_type: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let c = corp_id;
    if g.entity_status[c] != 1 || c < 11 || c > 1600 {
        return;
    }
    let pct_to_issue = to_long(g.tmp_ext1);
    g.tmp_ext1 = Ext::from_f64(0.0);
    if pct_to_issue < 5 || pct_to_issue > 49 {
        return;
    }
    update_corp_control(g, ui, rng, c);
    let mut is_ipo: i16 = 0;
    if g.public_pct == 0 {
        is_ipo = 1;
    }
    if g.voting_controller > 10 && c < 86 && offer_type == 1 {
        calc_entity_financials(g, ui, rng, c);
        let mut cust_deposits = g.bank_cust_deposits[c];
        if !(cust_deposits >= Ext::from_f64(0.0)) {
            cust_deposits = Ext::from_f64(0.0);
        }
        let deposits = (cust_deposits + g.bank_c_ds[c]) + g.bank_demand_deposits[c];
        if g.net_worth + g.bad_debt_reserve[c] > deposits * sf(0.04f32) {
            return;
        }
    }
    let proceeds: i64 = to_quad(cast::<Ext, _>(pct_to_issue) * g.market_cap[c]);
    if proceeds > g.max_stock_offering_amt
        && g.cur_player != g.corp_controller[c]
        && offer_type == 1
        && g.corp_controller[c] < 11
    {
        g.news_msg = g.ent_symbol[c].clone()
            + __content.string(10761)
            + &trim(g.currency_symbol.as_bytes())
            + &fm(cast::<Ext, _>(proceeds), "##################,")
            + " "
            + &g.million_prefix_lc
            + "il."
            + &g.currency_suffix;
        post_news(g, ui, 0, 0);
        return;
    }
    g.cash[c] = cast::<Ext, _>(proceeds) + g.cash[c];
    let amt_text = trim(g.currency_symbol.as_bytes()) + &fm(cast::<Ext, _>(proceeds), "####,") + " ";
    let mut offer_desc = s(__content.string(10762));
    if offer_type == 2 {
        offer_desc = s(__content.string(10763));
    }
    offer_desc = offer_desc + __content.string(10764);
    if g.public_pct == 0 && offer_type == 1 {
        offer_desc = s("IPO");
    }
    g.news_msg = g.tmp_symbol.clone()
        + __content.string(10765)
        + &amt_text
        + &g.million_prefix_lc
        + "illion"
        + &g.currency_suffix
        + " in "
        + &offer_desc
        + ".";
    if get_etf_adviser_human_status(g, ui, rng, c) > 0 {
        post_news(g, ui, 0, 0);
    }
    if offer_type == 2 && g.action_code == "AUTO" {
        g.news_msg = g.tmp_symbol.clone() + __content.string(10766);
    }
    if !(cast::<Ext, _>(proceeds) >= Ext::from_f64(5.0)) {
        g.news_msg = PbStr::new();
    }
    if get_controller_type(g, c) != 1 {
        g.news_msg = PbStr::new();
    }
    let saved_news = g.news_msg.clone();
    if is_ipo == 0 {
        g.investor_sentiment[c] = g.investor_sentiment[c].wrapping_sub(pct_to_issue);
    } else {
        let r = rng.rnd_range(30, 1);
        g.investor_sentiment[c] = 25i32.wrapping_add(g.investor_sentiment[c]).wrapping_sub(r);
    }
    g.shares_outstanding[c] = (cast::<Ext, _>(pct_to_issue) * sf(0.01f32) + Ext::from_f64(1.0)) * g.shares_outstanding[c];
    let scale_pct = (cast::<Ext, _>(pct_to_issue) * sf(0.01f32) + Ext::from_f64(1.0)) * Ext::from_f64(100.0);
    let inv_scale_pct = to_long((Ext::from_f64(100.0) / (cast::<Ext, _>(pct_to_issue) + Ext::from_f64(100.0))) * Ext::from_f64(100.0));
    adjust_options_for_corp_action(g, ui, c, scale_pct, inv_scale_pct, 0);
    adjust_conversion_price(g, c, Ext::from_f64(0.0), cast::<Ext, _>(pct_to_issue));
    adjust_price_alerts_for_stock(g, c, Ext::from_f64(0.0), cast::<Ext, _>(pct_to_issue));
    for holder_id in 1..=1600 {
        if g.entity_status[holder_id] != 1 {
            continue;
        }
        let mut slot = 1;
        while slot < 16 {
            if cast::<i32, _>(g.hold_stock_id[(slot, holder_id)]) == c {
                let hold_value = cast::<Ext, _>(g.hold_stock_pct[(slot, holder_id)]) * g.market_cap[c];
                let new_pct = to_quad(
                    (Ext::from_f64(100.0) / (cast::<Ext, _>(pct_to_issue) + Ext::from_f64(100.0)))
                        * cast::<Ext, _>(g.hold_stock_pct[(slot, holder_id)]),
                );
                g.hold_stock_pct[(slot, holder_id)] = cast::<i16, _>(new_pct);
                if g.hold_stock_pct[(slot, holder_id)] == 0 {
                    g.cash[holder_id] = hold_value + g.cash[holder_id];
                    g.hold_stock_id[(slot, holder_id)] = 0;
                    if g.num_players < holder_id {
                        let x = hold_value - g.hold_stock_basis[(slot, holder_id)];
                        g.extraordinary_pl[holder_id] =
                            to_quad(x + cast::<Ext, _>(g.extraordinary_pl[holder_id]));
                    } else {
                        if hold_value < Ext::from_f64(0.0) {
                            g.hold_stock_basis[(slot, holder_id)] =
                                -g.hold_stock_basis[(slot, holder_id)];
                        }
                        g.player_cap_gains[holder_id] = (hold_value
                            - g.hold_stock_basis[(slot, holder_id)])
                            + g.player_cap_gains[holder_id];
                    }
                    g.hold_stock_basis[(slot, holder_id)] = Ext::from_f64(0.0);
                }
                slot = 15;
            }
            slot += 1;
        }
    }
    g.market_cap[c] = (cast::<Ext, _>(pct_to_issue) * sf(0.01f32) + Ext::from_f64(1.0)) * g.market_cap[c];
    if offer_type == 1 && g.industry_num[c] != 71 {
        adjust_index_divisor(g, c, pct_to_issue);
        if is_ipo == 1 {
            g.market_cap[c] = (rng.rnd() * sf(0.2f32) + sf(1.05f32)) * g.market_cap[c];
        }
    }
    if g.industry_num[c] != 71 {
        scale_earnings_history(g, c, pct_to_issue);
    }
    scale_price_history(g, c, pct_to_issue);
    update_control_chain(g, ui, rng, c);
    recapture_excess_loss_news(g, ui, c, 0);
    g.news_msg = saved_news;
    force_regulated_call_sales(g, ui, rng);
}
