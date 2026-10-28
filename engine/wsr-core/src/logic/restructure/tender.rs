//! Stock buy-backs from the public or from one holder: `LboTenderOffer` (0x51f21c) and the premium
//! negotiation with a reluctant seller, `NegotiatePremiumBuyout` (0x528aef).
//!
//! `DATE$` / `TIME$` are read through `now` ([`LocalTime`]): `NegotiatePremiumBuyout` uses the day of the
//! month of `DATE$`, `CalcStockPrice` the seconds of `TIME$`.
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use crate::logic::control::update_corp_control;
use crate::logic::earnings::calc_projected_earnings;
use crate::logic::etf_derivs::corp_options::force_close_excess_naked_options;
use crate::logic::finance::{calc_entity_financials, get_controller_type};
use crate::logic::holdings::find_holding_slot;
use crate::logic::newgame::LocalTime;
use crate::logic::report::{number_to_words, show_quick_message};
use crate::logic::shorts::find_short_positions;
use crate::logic::sim_entity::bonds::{
    adjust_conversion_price, adjust_price_alerts_for_stock, normalize_holding_pcts, scale_earnings_history,
};
use crate::logic::sim_entity::dividends::adjust_options_for_corp_action;
use crate::logic::sim_entity::quarter_start::cancel_pending_merger;
use crate::logic::valuation::{adjust_index_divisor, calc_stock_price, scale_price_history, update_price_history_tick};
use crate::pb::string::{CaseMode, remove_any, ucase};

/// `RTRIM$(LTRIM$(CurrencySymbol))`.
fn cur_sym(g: &Globals) -> PbStr {
    trim(g.currency_symbol.as_bytes())
}

/// Port of LboTenderOffer (0x51f21c).
/// Corp `corp_id_ref` buys back its own stock: from the public (`seller_id < 1`, "LBO": a tender offer at
/// a premium of 0.75% per percent bought, max 25%) or from the holder `seller_id` ("GM", greenmail: a
/// negotiated premium via [`negotiate_premium_buyout`] for the player, 25% for the computer). The mode
/// and amount come in the globals: `TmpExt1` = AI mode (0 = player with dialogs, 1/2 = computer; 2 also
/// skips the credit check and the action count), `TmpExt2` = percent to buy; both are cleared.
///
/// Charges ~1% legal fees (refunded / expensed to `AccruedPL` depending on the outcome); banks and
/// insurers (industry 1/2) may not shrink net worth by >15% or assets by >5% unless rated 9+; the buy-back
/// must stay below net worth, may not drop the credit rating below 4 (tested on a pro-forma balance sheet)
/// and must be financed from cash + T-bills or borrowing. On success: pays the seller (basis, gain to
/// `ExtraordinaryPL`), shrinks market cap and shares, scales every holder's percentage by
/// `100 / (100 - pct)`, adjusts options, convertibles, price alerts, index divisor, earnings and price
/// history, sentiment, cancels a pending merger for the corp, normalizes the holdings, posts news.
///
/// A player's cash shortfall is financed by `BorrowToCoverPurchase` (NO cancels). Buying 100% raises
/// error 151 (`ERROR 151` before `100 / (100 - pct)`): the ON ERROR handler calls `ShowRuntimeError`
/// (silent for error 151 unless `DataPath` contains "PBWIN") and leaves the routine after the cash has
/// been paid.
pub fn lbo_tender_offer(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, corp_id_ref: i32, seller_id: i32, now: &LocalTime) {
    let __content = crate::content::current();
    // locals (zeroed by the frame setup)
    let mut holder_slot: i32 = 0;
    let mut holder_pct: i32 = 0;
    let mut force_to_100: i32 = 0;
    let mut legal_fees: i64 = 0;
    let total_cost: Ext;
    let mut seller_slot: i16 = 0;
    let mut seller_pct: i16 = 0;
    let mut msg_text: PbStr;

    let ai_mode = to_long(g.tmp_ext1);
    let mut pct_to_buy = to_integer(g.tmp_ext2);
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_ext2 = Ext::from_f64(0.0);
    let corp_id = corp_id_ref;
    calc_entity_financials(g, ui, rng, corp_id);
    update_corp_control(g, ui, rng, corp_id);
    let largest_holder = cast::<i32, _>(g.largest_holder);
    if g.wholly_owned_by == 0 && 0 < largest_holder && seller_id != largest_holder {
        holder_slot = find_holding_slot(g, largest_holder, corp_id);
        holder_pct = cast::<i32, _>(g.hold_stock_pct[(holder_slot, largest_holder)]);
    }
    let deal_is_gm;
    if seller_id < 1 {
        deal_is_gm = false;
        if g.public_pct < pct_to_buy {
            pct_to_buy = g.public_pct;
        }
        if pct_to_buy < g.public_pct && pct_to_buy < 5 {
            if ai_mode == 0 {
                let mut m = g.ent_name[g.active_entity].clone()
                    + __content.string(9822)
                    + __content.string(9823);
                show_quick_message(g, ui, &mut m);
            }
            return;
        }
        if pct_to_buy == g.public_pct {
            force_to_100 = 1;
        }
        let mut premium_pct = cast::<Ext, _>(pct_to_buy) * sf(0.0075f32);
        if sf(0.25f32) < premium_pct {
            premium_pct = sf(0.25f32);
        }
        let offer_cap = g.market_cap[corp_id] + premium_pct * g.market_cap[corp_id];
        total_cost = offer_cap * cast::<Ext, _>(pct_to_buy);
        legal_fees = to_quad(total_cost * sf(0.01f32));
        g.cash[corp_id] = g.cash[corp_id] - cast::<Ext, _>(legal_fees);
        if ai_mode == 0 {
            let premium_pct_int = to_long(premium_pct * Ext::from_f64(100.0));
            let price = offer_cap / g.shares_outstanding[corp_id];
            msg_text = s(__content.string(9824))
                + &fmt_trim(price, b"* ###.00")
                + " per "
                + __content.string(9825)
                + &number_to_words(premium_pct_int, 0, 1)
                + __content.string(9826)
                + &g.ent_symbol[corp_id]
                + __content.string(9827)
                + __content.string(9828)
                + &cur_sym(g)
                + &fmt_trim(total_cost, b"* ###,.00")
                + " "
                + &g.million_prefix_lc
                + "illion"
                + &g.currency_suffix
                + " "
                + __content.string(9829)
                + "\r"
                + "\r"
                + __content.string(9830);
            let answer = ui.msgbox(&msg_text, 0x2123, &s(__content.string(9831)));
            if answer != 6 {
                g.cash[corp_id] = g.cash[corp_id] + cast::<Ext, _>(legal_fees);
                let mut m = s(__content.string(9832));
                show_quick_message(g, ui, &mut m);
                return;
            }
        }
    } else {
        deal_is_gm = true;
        let mut slot = 1;
        while slot < 16 {
            if cast::<i32, _>(g.hold_stock_id[(slot, seller_id)]) == corp_id {
                seller_slot = cast::<i16, _>(slot);
                seller_pct = g.hold_stock_pct[(slot, seller_id)];
                slot = 15;
            }
            slot += 1;
        }
        if seller_pct < pct_to_buy {
            pct_to_buy = seller_pct;
        }
        if pct_to_buy < seller_pct && pct_to_buy < 5 {
            if ai_mode == 0 {
                let mut m = s(__content.string(2535))
                    + &g.ent_name[seller_id]
                    + __content.string(9833)
                    + __content.string(9834);
                show_quick_message(g, ui, &mut m);
            }
            return;
        }
        if g.public_pct == 0 {
            force_to_100 = 1;
        }
        if ai_mode < 1 {
            let neg_pct = cast::<i32, _>(pct_to_buy);
            let neg_premium = negotiate_premium_buyout(g, ui, rng, corp_id, seller_id, corp_id, neg_pct, now);
            total_cost = g.market_cap[corp_id] * (cast::<Ext, _>(neg_premium) * sf(0.01f32) + Ext::from_f64(1.0)) * cast::<Ext, _>(neg_pct);
            legal_fees = to_quad(total_cost * sf(0.01f32));
            if (cast::<Ext, _>(legal_fees)) < Ext::from_f64(1.0) {
                legal_fees = 1;
            }
            if 0 < neg_premium {
                g.cash[corp_id] = g.cash[corp_id] - cast::<Ext, _>(legal_fees);
            }
            if neg_premium == 5 {
                legal_fees = to_quad(cast::<Ext, _>(legal_fees) * sf(0.2f32));
                g.cash[corp_id] = g.cash[corp_id] + cast::<Ext, _>(legal_fees);
                g.accrued_pl[corp_id] = g.accrued_pl[corp_id] - cast::<Ext, _>(legal_fees);
            }
            if neg_premium < 11 {
                return;
            }
        } else {
            total_cost = g.market_cap[corp_id] * sf(1.25f32) * cast::<Ext, _>(pct_to_buy);
        }
    }
    if pct_to_buy == 0 && 0 < ai_mode {
        return;
    }
    if g.net_worth * sf(0.15f32) < total_cost || g.total_assets * sf(0.05f32) < total_cost || g.credit_rating_num[corp_id] < 9 {
        if g.industry_num[corp_id] < 3 {
            if ai_mode == 0 {
                let regulator = if g.industry_num[corp_id] == 2 { s(__content.string(2878)) } else { s("Banking") };
                msg_text = regulator.clone()
                    + __content.string(9835)
                    + &g.ent_symbol[corp_id]
                    + __content.string(9836)
                    + __content.string(9837)
                    + __content.string(9838)
                    + "\""
                    + "AA,"
                    + "\""
                    + __content.string(9839)
                    + "\r\n"
                    + "\r\n"
                    + __content.string(9840);
                show_quick_message(g, ui, &mut msg_text);
                g.news_msg = regulator + __content.string(9841) + &g.ent_symbol[corp_id] + __content.string(9842);
                post_news(g, ui, 0, 0);
            }
            g.accrued_pl[corp_id] = g.accrued_pl[corp_id] - cast::<Ext, _>(legal_fees);
            return;
        }
    }
    if g.net_worth <= total_cost {
        if ai_mode == 0 {
            let mut m = g.ent_symbol[corp_id].clone() + __content.string(9843);
            show_quick_message(g, ui, &mut m);
        }
        g.accrued_pl[corp_id] = g.accrued_pl[corp_id] - cast::<Ext, _>(legal_fees);
        return;
    }
    let mut funds_ok = 0;
    if ai_mode < 2 {
        let saved_cash = g.cash[corp_id];
        let saved_bank_debt = g.bank_debt[corp_id];
        g.cash[corp_id] = g.cash[corp_id] - total_cost;
        if g.cash[corp_id] < Ext::from_f64(0.0) {
            g.bank_debt[corp_id] = g.bank_debt[corp_id] - g.cash[corp_id];
            g.cash[corp_id] = Ext::from_f64(0.0);
        }
        calc_entity_financials(g, ui, rng, corp_id);
        g.cash[corp_id] = saved_cash;
        g.bank_debt[corp_id] = saved_bank_debt;
        if g.credit_rating_num[corp_id] < 4 {
            if ai_mode == 0 {
                ui.msgbox(
                    &(g.ent_symbol[corp_id].clone()
                        + __content.string(9844)
                        + __content.string(9845)
                        + "\r"
                        + "\r"
                        + __content.string(9846)),
                    0x2010,
                    &s(__content.string(9847)),
                );
                g.news_msg = g.ent_symbol[corp_id].clone() + __content.string(9848);
                post_news(g, ui, 0, 0);
            }
            g.accrued_pl[corp_id] = g.accrued_pl[corp_id] - cast::<Ext, _>(legal_fees);
            return;
        }
        calc_entity_financials(g, ui, rng, corp_id);
        funds_ok = 0;
        if ai_mode == 0 && g.merger_target == corp_id && 10 < g.merger_acquirer {
            msg_text = s(__content.string(9849))
                + &g.ent_symbol[corp_id]
                + __content.string(9850)
                + __content.string(9851)
                + &g.ent_symbol[g.merger_acquirer]
                + __content.string(9852)
                + "\r"
                + "\r"
                + __content.string(9853);
            let answer = ui.msgbox(&msg_text, 0x2124, &s(__content.string(9854)));
            if answer == 7 {
                g.accrued_pl[corp_id] = g.accrued_pl[corp_id] - cast::<Ext, _>(legal_fees);
                return;
            }
        }
    }
    if g.cash[corp_id] + g.t_bills[corp_id] < total_cost {
        if total_cost < g.liquid_funds {
            let mut shortfall = total_cost - (g.cash[corp_id] + g.t_bills[corp_id]);
            if ai_mode == 0 {
                let answer = crate::logic::trading::borrow_to_cover_purchase(g, ui, &mut shortfall, total_cost, 0);
                if answer == 7 {
                    return;
                }
            } else {
                g.cash[corp_id] = g.cash[corp_id] + shortfall;
                g.bank_debt[corp_id] = g.bank_debt[corp_id] + shortfall;
                let bank = cast::<i32, _>(g.lending_bank[corp_id]);
                g.cash[bank] = g.cash[bank] - shortfall;
            }
            funds_ok = 1;
        }
    } else {
        funds_ok = 1;
    }
    if funds_ok == 0 && ai_mode != 1 {
        if ai_mode == 0 {
            let mut m = g.ent_symbol[corp_id].clone() + __content.string(9855);
            show_quick_message(g, ui, &mut m);
        }
        g.accrued_pl[corp_id] = g.accrued_pl[corp_id] - cast::<Ext, _>(legal_fees);
        return;
    }
    g.cash[corp_id] = g.cash[corp_id] - total_cost;
    if pct_to_buy == 100 {
        // ERROR 151 -> ON ERROR handler (0x5211f7): ShowRuntimeError and leave the routine
        let diag = s(__content.string(9856)) + &str_s(cast::<Ext, _>(corp_id)) + __content.string(9857) + &str_s(cast::<Ext, _>(seller_id));
        super::merge::show_runtime_error(g, ui, 151, &s(__content.string(9858)), &diag, 0);
        return;
    }
    // 0x5205f3: the local is DOUBLE. Preserve this spill before holder rounding.
    let scale_factor = cast::<f64, _>(Ext::from_f64(100.0) / (Ext::from_f64(100.0) - cast::<Ext, _>(pct_to_buy)));
    let scale_factor = Ext::from_f64(scale_factor);
    if deal_is_gm {
        let ss = cast::<i32, _>(seller_slot);
        g.cash[seller_id] = g.cash[seller_id] + total_cost;
        let mut basis_sold = Ext::from_f64(0.0);
        if 0 < g.hold_stock_pct[(ss, seller_id)] && Ext::from_f64(0.0) < g.hold_stock_basis[(ss, seller_id)] {
            basis_sold = (cast::<Ext, _>(pct_to_buy) / cast::<Ext, _>(g.hold_stock_pct[(ss, seller_id)])) * g.hold_stock_basis[(ss, seller_id)];
            g.hold_stock_basis[(ss, seller_id)] = g.hold_stock_basis[(ss, seller_id)] - basis_sold;
        }
        let seller_gain = total_cost - basis_sold;
        g.extraordinary_pl[seller_id] = to_quad(cast::<Ext, _>(g.extraordinary_pl[seller_id]) + seller_gain);
        g.hold_stock_pct[(ss, seller_id)] = g.hold_stock_pct[(ss, seller_id)].wrapping_sub(pct_to_buy);
        if g.hold_stock_pct[(ss, seller_id)] == 0 {
            g.hold_stock_id[(ss, seller_id)] = 0;
            g.hold_stock_basis[(ss, seller_id)] = Ext::from_f64(0.0);
        }
    }
    if !deal_is_gm && 0 < ai_mode {
        g.investor_sentiment[corp_id] = cast::<i32, _>(pct_to_buy) + g.investor_sentiment[corp_id];
    } else if ai_mode == 0 {
        if -30 < g.investor_sentiment[corp_id] {
            g.investor_sentiment[corp_id] = g.investor_sentiment[corp_id] - 10;
            if 0 < g.investor_sentiment[corp_id] {
                g.investor_sentiment[corp_id] = g.investor_sentiment[corp_id] - 20;
            }
            if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                calc_projected_earnings(g, ui, rng, corp_id);
                g.valuation_mode = 3;
                calc_stock_price(g, ui, rng, corp_id, now.time_secs());
            }
        }
    }
    if ai_mode < 2 {
        g.turn_action_count = g.turn_action_count + 1;
    }
    if g.merger_target == corp_id && 10 < g.merger_acquirer {
        let mut m = s(__content.string(9859))
            + &g.ent_name[g.merger_acquirer]
            + __content.string(5266)
            + &g.ent_name[g.merger_target]
            + __content.string(9860);
        show_quick_message(g, ui, &mut m);
        cancel_pending_merger(g, ui, 0);
    }
    let pct_change = -(cast::<i32, _>(pct_to_buy));
    adjust_index_divisor(g, corp_id, pct_change);
    scale_earnings_history(g, corp_id, pct_change);
    g.market_cap[corp_id] = g.market_cap[corp_id] * (Ext::from_f64(100.0) - cast::<Ext, _>(pct_to_buy)) * sf(0.01f32);
    g.shares_outstanding[corp_id] = g.shares_outstanding[corp_id] * (Ext::from_f64(100.0) - cast::<Ext, _>(pct_to_buy)) * sf(0.01f32);
    let inv_scale_pct = (Ext::from_f64(1.0) / scale_factor) * Ext::from_f64(100.0);
    let scale_pct_int = to_long(scale_factor * Ext::from_f64(100.0));
    adjust_options_for_corp_action(g, ui, corp_id, inv_scale_pct, scale_pct_int, 0);
    adjust_conversion_price(g, corp_id, Ext::from_f64(0.0), cast::<Ext, _>(pct_change));
    adjust_price_alerts_for_stock(g, corp_id, Ext::from_f64(0.0), cast::<Ext, _>(pct_change));
    for holder_id in 1..=1600 {
        let mut slot = 1;
        while slot < 16 {
            if cast::<i32, _>(g.hold_stock_id[(slot, holder_id)]) == corp_id {
                let p = cast::<Ext, _>(g.hold_stock_pct[(slot, holder_id)]);
                g.hold_stock_pct[(slot, holder_id)] = cast::<i16, _>(to_quad(scale_factor * p));
                slot = 15;
            }
            slot += 1;
        }
    }
    if 0 < holder_pct && cast::<i32, _>(g.hold_stock_pct[(holder_slot, largest_holder)]) == holder_pct {
        g.hold_stock_pct[(holder_slot, largest_holder)] = g.hold_stock_pct[(holder_slot, largest_holder)].wrapping_add(1);
    }
    normalize_holding_pcts(g, ui, rng, corp_id, force_to_100);
    if ai_mode == 0 {
        ui.msgbox(
            &(s(__content.string(9861))
                + "\r"
                + "\r"
                + __content.string(9862)
                + "\r"
                + "\r"
                + __content.string(9863)
                + &g.currency_symbol
                + &fmt_trim(total_cost, b"* ###,.00")
                + " "
                + &g.million_prefix_uc
                + "ILLION"
                + &ucase(g.currency_suffix.as_bytes(), CaseMode::Ansi)),
            0x2000,
            &s(__content.string(9864)),
        );
    }
    g.news_msg = g.ent_symbol[corp_id].clone() + __content.string(9865) + &str_s(cast::<Ext, _>(pct_to_buy)) + __content.string(9866);
    if 0 < seller_id {
        g.news_msg = g.news_msg.clone() + " from " + &g.ent_symbol[seller_id];
    }
    if Ext::from_f64(3.0) < total_cost {
        g.news_msg = g.news_msg.clone()
            + " for "
            + &g.currency_symbol
            + &fmt_trim(total_cost, b"* ###,")
            + " "
            + &g.million_prefix_lc
            + "il.";
    }
    post_news(g, ui, 0, 0);
    if Ext::from_f64(0.0) < cast::<Ext, _>(legal_fees) {
        g.news_msg = g.ent_symbol[corp_id].clone()
            + __content.string(9867)
            + &cur_sym(g)
            + &fmt_trim(cast::<Ext, _>(legal_fees), b"* ###,")
            + " "
            + &g.million_prefix_lc
            + __content.string(9868);
        post_news(g, ui, 0, 0);
    }
    find_short_positions(g, ui, rng, 0, 0);
    scale_price_history(g, corp_id, -(cast::<i32, _>(pct_to_buy)));
    update_price_history_tick(g, ui, rng, corp_id);
    if deal_is_gm && 10 < seller_id {
        force_close_excess_naked_options(g, ui, rng, seller_id, corp_id);
    }
}

/// Port of NegotiatePremiumBuyout (0x528aef).
/// The holder `seller_id` of `corp_id` stock only sells `pct_to_buy` percent to `buyer_id` at a premium:
/// asks whether to make an offer (YES/NO), computes the seller's minimum premium (quarter, year, seller id,
/// the day of `DATE$`, a repeated bidder, computer-player / computer-controlled / fund sellers, the size of
/// the block), asks the premium with `INPUTBOX$` (default "10%") and judges it: too low ("You are not
/// serious."), too high for a corporate buyer (reckless), rejected (news; returns 5) or accepted (returns the
/// premium). Sets `LastPremiumBidder` once an offer is considered. Returns 0 when no offer is made.
pub fn negotiate_premium_buyout(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    corp_id: i32,
    seller_id: i32,
    buyer_id: i32,
    pct_to_buy: i32,
    now: &LocalTime,
) -> i32 {
    let __content = crate::content::current();
    let mut ask_adjust: i32 = 0;
    let mut result: i32 = 0;
    let mut msg_text = g.ent_name[seller_id].clone()
        + __content.string(9869)
        + &g.ent_name[corp_id]
        + __content.string(9870)
        + __content.string(9871)
        + "\r"
        + "\r"
        + __content.string(9872)
        + __content.string(9873)
        + __content.string(9874)
        + "\r"
        + "\r"
        + __content.string(9875);
    let answer = ui.msgbox(&msg_text, 0x2024, &(s(__content.string(9876)) + &g.ent_name[seller_id] + "?"));
    if answer != 6 {
        let mut m = s(__content.string(5028));
        show_quick_message(g, ui, &mut m);
        return result;
    }
    let slot = find_holding_slot(g, seller_id, corp_id);
    let seller_pct = cast::<i32, _>(g.hold_stock_pct[(slot, seller_id)]);
    let mut min_premium: i32 = g.cur_quarter * 3 + 7;
    // (VAL(MID$(DATE$, 4, 2)) + CurYear + sellerId) MOD 5 (float MOD; all operands are integers here)
    let day = cast::<Ext, _>(now.day);
    let m5 = (day + cast::<Ext, _>(g.cur_year) + cast::<Ext, _>(seller_id)) % Ext::from_f64(5.0);
    min_premium = to_long(m5 * Ext::from_f64(4.0) + cast::<Ext, _>(min_premium));
    if 30 < min_premium {
        min_premium -= 10;
    }
    if seller_id == g.last_premium_bidder {
        let base = min_premium + 3;
        min_premium = base + rng.rnd_range(5, 1);
    }
    if 0 < seller_id && seller_id <= g.num_players && g.is_computer_player[seller_id] == 1 {
        ask_adjust = 20;
    }
    if 10 < seller_id && get_controller_type(g, seller_id) == 2 {
        ask_adjust = 10;
    }
    if 1580 < seller_id && g.industry_num[seller_id] == 71 && 17 < min_premium {
        ask_adjust = -8;
    }
    min_premium += ask_adjust;
    msg_text = s(__content.string(9877));
    let reply = ui.inputbox(&msg_text, &(g.ent_symbol[g.active_entity].clone() + __content.string(9878)), &s("10%"));
    let mut premium_pct = to_long(val(remove_any(reply.as_bytes(), b"$ ,%").as_bytes()));
    if premium_pct < 1 {
        let mut m = s(__content.string(9879));
        show_quick_message(g, ui, &mut m);
        return result;
    }
    let remaining_pct = seller_pct - pct_to_buy;
    if seller_pct < 80 || 79 < remaining_pct || seller_id < 11 {
        if seller_pct < 51 || 50 < remaining_pct || remaining_pct < 1 || seller_id < 11 {
            if seller_pct < 20 || 19 < remaining_pct || remaining_pct < 1 || seller_id < 11 {
                if seller_pct < 5 {
                    min_premium -= 3;
                }
            } else {
                min_premium += 6;
            }
        } else {
            min_premium += 8;
        }
    } else {
        min_premium += 10;
    }
    if pct_to_buy == seller_pct && 15 < min_premium {
        min_premium -= 2;
    }
    if min_premium < 10 {
        min_premium = 10;
    }
    let mut reply_text: PbStr;
    if premium_pct < min_premium - 5 {
        reply_text = s(__content.string(9880));
        let mut rebuff_tail = s(__content.string(9881));
        if g.rand_seed % 3 == 1 && premium_pct < 20 {
            reply_text = s(__content.string(9882));
            rebuff_tail = s(__content.string(9883));
        }
        if g.rand_seed % 3 == 2 {
            reply_text = s(__content.string(9884));
            rebuff_tail = s(__content.string(9885));
        }
        msg_text = reply_text + __content.string(9886) + &rebuff_tail;
        show_quick_message(g, ui, &mut msg_text);
    } else if ask_adjust + 50 < premium_pct && 10 < buyer_id {
        msg_text = g.ent_name[buyer_id].clone()
            + __content.string(9887)
            + __content.string(9888)
            + __content.string(9889)
            + &g.ent_symbol[buyer_id]
            + __content.string(9890)
            + "\r\n"
            + "\r\n"
            + __content.string(9891);
        show_quick_message(g, ui, &mut msg_text);
    } else {
        msg_text = s(__content.string(9892)) + &g.ent_name[seller_id] + __content.string(9893);
        show_quick_message(g, ui, &mut msg_text);
        if premium_pct < min_premium {
            msg_text = g.ent_name[seller_id].clone()
                + __content.string(9894)
                + "\r\n"
                + "\r\n"
                + __content.string(9895);
            reply_text = PbStr::new();
            if buyer_id < 11 {
                reply_text = s("not ");
            }
            g.news_msg = g.ent_symbol[seller_id].clone()
                + __content.string(9896)
                + &str_s(cast::<Ext, _>(premium_pct))
                + __content.string(9897)
                + &g.ent_symbol[corp_id]
                + " stock";
            if corp_id != buyer_id && len(g.ent_symbol[buyer_id].as_bytes()) < 22 {
                g.news_msg = g.news_msg.clone() + " by " + &g.ent_symbol[buyer_id];
            }
            post_news(g, ui, 0, 0);
            premium_pct = 5;
        } else {
            msg_text = s(__content.string(9898))
                + &str_s(cast::<Ext, _>(premium_pct))
                + __content.string(9899)
                + __content.string(9900)
                + __content.string(9901);
            reply_text = s("not ");
        }
        g.last_premium_bidder = seller_id;
        msg_text = msg_text + "\r\n" + "\r\n" + __content.string(9902) + &reply_text + __content.string(9903);
        if reply_text == "not " && 10 < buyer_id {
            msg_text = msg_text + __content.string(9904);
        } else {
            msg_text = msg_text + ".)";
        }
        show_quick_message(g, ui, &mut msg_text);
        result = premium_pct;
    }
    result
}
