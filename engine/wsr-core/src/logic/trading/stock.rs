//! The stock trade primitives: `BuyStock` (0x4efe60, every purchase of a stake, by players, companies
//! and the computer), its takeover-premium price `CalcTakeoverPremiumPrice` (0x528736), the player's
//! `SellStock` (0x5ab449) and the private buyer for unlisted stock `PrivateBuyoutOffer` (0x5bcd00).

// Keep the original's statement shapes.
#![allow(clippy::assign_op_pattern, clippy::collapsible_if, clippy::collapsible_else_if)]
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use crate::logic::control::update_corp_control;
use crate::logic::earnings::calc_projected_earnings;
use crate::logic::etf_derivs::corp_options::force_close_excess_naked_options;
use crate::logic::etf_derivs::etf_mgmt::cover_negative_cash;
use crate::logic::events::antitrust::antitrust_review_acquisition;
use crate::logic::finance::{calc_entity_financials, get_controller_type};
use crate::logic::holdings::find_holding_slot;
use crate::logic::reports_corp::check_cross_ownership;
use crate::logic::reports_lists::{show_portfolio_listing, update_control_chain};
use crate::logic::restructure::liquidate_sub::liquidate_sub_into_parent;
use crate::logic::shorts::find_short_positions;
use crate::logic::sim_entity::asset_sales::calc_group_stake_pct;
use crate::logic::sim_entity::quarter_start::recapture_excess_loss;
use crate::logic::sim_entity::transactions::{pack_holdings, sell_stock_holding};
use crate::logic::valuation::{calc_stock_price, update_price_history_tick};

use super::select::get_selected_player_etf;
use crate::pb::ext80::Ext80;

/// Port of BuyStock (0x4efe60).
///
/// `buyer_id` buys `ROUND(TmpExt1)` percent of `target_id`; `ROUND(TmpExt2)` is the mode (0 = the
/// player's purchase with confirmations, 1 = merger step, 2+ = computer deals). `TmpExt1..3` are cleared.
/// Funds (1581..1585 of industry 71) do not buy here; in modes > 0 a fund (industry 71) target is
/// refused and a fund buyer that already holds the stock buys at most 1%.
///
/// The price first moves: a stake taking the buyer's group (or, for an uncontrolled buyer, its own
/// holding) over 5% of a non-fund pays the takeover premium ([`calc_takeover_premium_price`]),
/// otherwise the cap rises by `0.3 * 0.005 * DifficultyLevel * pct` (funds: +1%); mode 1 uses
/// `old * 1.05`. More than the public float, or a control-relevant purchase barred by the antitrust
/// review (`AntitrustReviewAcquisition(2)`) or by the banking regulators (a bank buying a bank of
/// another controller, unless `(CurYear + CurQuarter + target + buyer) MOD 6 = 1`), restores the old cap
/// and ends. In modes > 0 a buyer whose cash plus T-bills don't cover `cap * pct` also ends there.
///
/// Then the holding, basis, cash and investor sentiment are booked; any pending `NewsMsg` is posted,
/// a tender-offer headline (over 5%) and the purchase headline follow. Mode 0 shows the confirmation
/// box, counts a turn action and runs `FindShortPositions`. The cap settles halfway back to the old
/// one, a 100% purchase of a same-industry company (3..69) may be liquidated into the buyer
/// ([`liquidate_sub_into_parent`], which gets `time_secs`), and control chain and cross-ownership are
/// updated.
// PB locals start zeroed; the port keeps them initialised like the original.
#[allow(unused_assignments)]
pub fn buy_stock(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, buyer_id: i32, target_id: i32, time_secs: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut group_pct = 0;
    let mut msg_text;
    if target_id == buyer_id {
        return;
    }
    if 1580 < buyer_id && buyer_id < 1586 && g.industry_num[buyer_id] == 71 {
        return;
    }
    let mut pct_to_buy = to_long(g.tmp_ext1);
    let mode = to_long(g.tmp_ext2);
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_ext2 = Ext::from_f64(0.0);
    g.tmp_ext3 = Ext::from_f64(0.0);
    if 0 < mode {
        if 10 < buyer_id && g.industry_num[buyer_id] == 71 && 1 < pct_to_buy {
            let exist_slot = find_holding_slot(g, buyer_id, target_id);
            if 0 < exist_slot && 0 < g.hold_stock_id[(exist_slot, buyer_id)] {
                pct_to_buy = 1;
            }
        }
        if 10 < target_id && g.industry_num[target_id] == 71 {
            return;
        }
    }
    let slot = find_holding_slot(g, buyer_id, target_id);
    if slot == 0 {
        return;
    }
    let pct_held = cast::<i32, _>(g.hold_stock_pct[(slot, buyer_id)]);
    let old_price = g.market_cap[target_id];
    update_corp_control(g, ui, rng, target_id);
    let public_pct_local: i16 = g.public_pct;
    let stake_pct = if 0 < get_controller_type(g, buyer_id) {
        let group_controller = if buyer_id < 11 { buyer_id } else { g.corp_controller[buyer_id] };
        group_pct = calc_group_stake_pct(g, group_controller, target_id);
        g.tmp_ext1 = Ext::from_f64(0.0);
        pct_to_buy + group_pct
    } else {
        pct_held + pct_to_buy
    };
    if 5 < stake_pct && g.industry_num[target_id] != 71 {
        g.market_cap[target_id] = calc_takeover_premium_price(g, buyer_id, target_id, pct_to_buy, public_pct_local);
    } else if g.industry_num[target_id] == 71 {
        g.market_cap[target_id] = sf(1.01f32) * g.market_cap[target_id];
    } else {
        let factor = sf(0.3f32) * sf(0.005f32) * cast::<Ext, _>(g.difficulty_level) * cast::<Ext, _>(pct_to_buy) + Ext::from_f64(1.0);
        g.market_cap[target_id] = factor * g.market_cap[target_id];
    }
    if mode == 1 {
        g.market_cap[target_id] = sf(1.05f32) * old_price;
    }
    if (cast::<i32, _>(g.public_pct)) < pct_to_buy {
        g.market_cap[target_id] = old_price;
        return;
    }
    let public_pct = cast::<i32, _>(g.public_pct);
    let control_relevant = !(81 <= public_pct - pct_to_buy
        || g.parent80_owner != 0
        || (10 < g.largest_holder && 59 < public_pct - pct_to_buy)
        || 49 < g.largest_holder_pct
        || ((pct_to_buy + pct_held) < cast::<i32, _>(g.largest_holder_pct)
            && public_pct == cast::<i32, _>(g.public_float_pct) - pct_held));
    if control_relevant {
        let mut run_mode_review = true;
        if mode == 0 {
            if g.cur_player != g.corp_controller[target_id] {
                // Would the purchase give the current player control?
                if pct_held == 0 {
                    g.hold_stock_id[(slot, buyer_id)] = cast::<i16, _>(target_id);
                }
                g.hold_stock_pct[(slot, buyer_id)] = cast::<i16, _>(pct_to_buy + pct_held);
                g.ctrl_change_msg_mode = 1;
                update_corp_control(g, ui, rng, target_id);
                g.hold_stock_pct[(slot, buyer_id)] = cast::<i16, _>(pct_held);
                if pct_held == 0 {
                    g.hold_stock_id[(slot, buyer_id)] = 0;
                }
                let mut antitrust_ok = 1;
                if g.cur_player == g.corp_controller[target_id] {
                    antitrust_ok = 0;
                }
                g.ctrl_change_msg_mode = 1;
                update_corp_control(g, ui, rng, target_id);
                if antitrust_ok == 0 {
                    g.tmp_ext1 = cast::<Ext, _>(buyer_id);
                    g.tmp_ext2 = cast::<Ext, _>(target_id);
                    g.tmp_ext3 = Ext::from_f64(0.0);
                    if antitrust_review_acquisition(g, ui, rng, 2) != 1 {
                        let mut m = s(__content.string(12808));
                        show_quick_message(g, ui, &mut m);
                        g.market_cap[target_id] = old_price;
                        return;
                    }
                }
                run_mode_review = false;
            }
        }
        if run_mode_review && mode != 0 {
            g.tmp_ext1 = cast::<Ext, _>(buyer_id);
            g.tmp_ext2 = cast::<Ext, _>(target_id);
            g.tmp_ext3 = cast::<Ext, _>(mode);
            let antitrust_ok = antitrust_review_acquisition(g, ui, rng, 2);
            if antitrust_ok != 1 {
                g.market_cap[target_id] = old_price;
                return;
            }
        }
        // Banking regulators.
        if 10 < buyer_id
            && g.industry_num[buyer_id] == 1
            && g.industry_num[target_id] == 1
            && mode != 1
            && g.corp_controller[target_id] != g.corp_controller[buyer_id]
            && (g.cur_year + g.cur_quarter + target_id + buyer_id) % 6 != 1
        {
            if mode == 0 {
                msg_text = s(__content.string(12809))
                    + &g.ent_name[target_id]
                    + " BY "
                    + &g.ent_name[buyer_id]
                    + __content.string(9697);
                let country_idx = g.country_idx[target_id];
                let mut ministry_name = g.country_name[country_idx].clone();
                if country_idx < 1 {
                    msg_text = replace(msg_text.as_bytes(), __content.bytes(12810), b"F.D.I.C.");
                } else {
                    if country_idx == 11 || country_idx == 30 || country_idx == 35 || country_idx == 50 {
                        ministry_name = s("THE ") + &ministry_name;
                    }
                    ministry_name = s(__content.string(12811)) + &ministry_name;
                    msg_text = replace(msg_text.as_bytes(), b"MINISTRY", ministry_name.as_bytes());
                }
                ui.msgbox(&msg_text, 8192, &s(__content.string(9702)));
            } else {
                g.news_msg = s(__content.string(12812))
                    + &g.ent_symbol[target_id]
                    + " by "
                    + &g.ent_symbol[buyer_id];
                if mid(g.news_headline[60].as_bytes(), 1, 7) != "Banking" {
                    post_news(g, ui, buyer_id, target_id);
                }
            }
            g.market_cap[target_id] = old_price;
            return;
        }
    }
    if slot == 0 {
        // (Dead: `slot` is never 0 here.)
        if mode == 0 {
            let mut m = s(__content.string(11818))
                + &g.ent_symbol[buyer_id]
                + __content.string(12813);
            show_quick_message(g, ui, &mut m);
        }
        g.market_cap[target_id] = old_price;
        return;
    }
    let cost = g.market_cap[target_id] * cast::<Ext, _>(pct_to_buy);
    if g.t_bills[buyer_id] + g.cash[buyer_id] < cost && 0 < mode {
        return;
    }
    g.hold_stock_id[(slot, buyer_id)] = cast::<i16, _>(target_id);
    g.hold_stock_pct[(slot, buyer_id)] = (cast::<i16, _>(pct_to_buy)).wrapping_add(g.hold_stock_pct[(slot, buyer_id)]);
    g.hold_stock_basis[(slot, buyer_id)] = cost + g.hold_stock_basis[(slot, buyer_id)];
    g.cash[buyer_id] = g.cash[buyer_id] - cost;
    g.investor_sentiment[target_id] =
        cast::<i32, _>(to_quad(cast::<Ext, _>(pct_to_buy) * sf(0.2f32) + cast::<Ext, _>(g.investor_sentiment[target_id])));
    if 5 < pct_to_buy {
        g.investor_sentiment[target_id] = 5 + g.investor_sentiment[target_id];
    }
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    if 5 < group_pct + pct_to_buy && g.industry_num[target_id] != 71 {
        g.valuation_mode = 0;
        calc_stock_price(g, ui, rng, target_id, 0); // mode 0 does not use TIME$
        g.news_msg = g.ent_symbol[buyer_id].clone()
            + __content.string(12814)
            + &g.ent_symbol[target_id]
            + " at "
            + &g.currency_symbol
            + &g.stock_price_str
            + __content.string(1071);
        if len(g.ent_symbol[buyer_id].as_bytes()) < 12 {
            g.news_msg = replace(g.news_msg.as_bytes(), b"makes", __content.bytes(12815));
        }
        post_news(g, ui, 0, 0);
    }
    msg_text = s(__content.string(10971));
    if Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd() {
        msg_text = s(__content.string(12816));
    }
    g.news_msg = g.ent_symbol[buyer_id].clone()
        + &msg_text
        + &str_s(cast::<Ext, _>(pct_to_buy))
        + "% of "
        + &g.ent_symbol[target_id];
    if sf(0.7f32) <= cost && mode != 1 {
        g.news_msg = g.news_msg.clone()
            + " for "
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(cost, "* ###,")
            + " "
            + &g.million_prefix_uc
            + ".";
    }
    if mode != 1
        && cast::<i32, _>(g.hold_stock_id[(slot, buyer_id)]) == target_id
        && pct_to_buy < cast::<i32, _>(g.hold_stock_pct[(slot, buyer_id)])
        && len(g.news_msg.as_bytes()) < 52
    {
        g.news_msg = g.news_msg.clone()
            + __content.string(12817)
            + &str_s(cast::<Ext, _>(g.hold_stock_pct[(slot, buyer_id)]))
            + __content.string(12818);
    }
    if 1585 < buyer_id && g.industry_num[buyer_id] == 71 {
        g.news_msg = PbStr::new();
    } else {
        post_news(g, ui, 0, 0);
    }
    update_price_history_tick(g, ui, rng, target_id);
    if 1 < mode {
        g.investor_sentiment[target_id] =
            cast::<i32, _>(to_quad(cast::<Ext, _>(pct_to_buy) * sf(0.3f32) + cast::<Ext, _>(g.investor_sentiment[target_id])));
    }
    if mode == 0 {
        msg_text = s(__content.string(12819));
        let mut tender_text = PbStr::new();
        if 5 < group_pct + pct_to_buy && g.industry_num[target_id] != 71 {
            msg_text = s(__content.string(12820));
            tender_text = s(__content.string(12821));
        } else if g.industry_num[target_id] == 71 {
            msg_text = s(__content.string(12822));
        }
        let mut buyer_text = g.ent_name[buyer_id].clone() + " has";
        if 31 < len(g.ent_name[buyer_id].as_bytes()) + len(g.ent_name[target_id].as_bytes()) {
            buyer_text = g.ent_symbol[buyer_id].clone() + " has";
        }
        if g.cur_player == buyer_id {
            buyer_text = s(__content.string(1506));
        }
        let text = msg_text.clone()
            + __content.string(12823)
            + "\r"
            + "\r"
            + &buyer_text
            + __content.string(2763)
            + &fmt_trim(cast::<Ext, _>(pct_to_buy) * g.shares_outstanding[target_id], "* ##########,.00")
            + " "
            + &g.million_prefix_lc
            + __content.string(12824)
            + &str_s(cast::<Ext, _>(pct_to_buy))
            + __content.string(12825)
            + __content.string(12826)
            + &g.ent_name[target_id]
            + ", "
            + &tender_text
            + __content.string(10850)
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(cost, "* ###,.00")
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + "...";
        ui.msgbox(&text, 8192, &s("CONFIRMATION"));
        g.turn_action_count = g.turn_action_count + 1;
        find_short_positions(g, ui, rng, 0, 0);
    }
    if Ext::from_f64(0.0) < old_price && g.industry_num[target_id] != 71 {
        g.market_cap[target_id] = (old_price + g.market_cap[target_id]) / Ext::from_f64(2.0);
    }
    if pct_to_buy == 100
        && 10 < buyer_id
        && g.industry_num[buyer_id] == g.industry_num[target_id]
        && 2 < g.industry_num[buyer_id]
        && g.industry_num[buyer_id] < 70
    {
        if g.rand_seed % 2 == 1 && 0 < mode && Ext::from_f64(0.0) <= g.tax_loss_carry[target_id] {
            g.tmp_ext1 = cast::<Ext, _>(mode);
            liquidate_sub_into_parent(g, ui, rng, buyer_id, target_id, time_secs);
        } else if mode == 0 {
            g.tmp_ext1 = Ext::from_f64(0.0);
            liquidate_sub_into_parent(g, ui, rng, buyer_id, target_id, time_secs);
        }
    }
    update_control_chain(g, ui, rng, target_id);
    check_cross_ownership(g, ui, rng, buyer_id, 1);
    check_cross_ownership(g, ui, rng, target_id, 1);
}

/// Port of CalcTakeoverPremiumPrice (0x528736).
/// The market cap of `target_id` with a takeover premium for `buyer_id` buying `pct_to_buy` percent
/// when `holder_pct` (the public float before the purchase) is available: premium =
/// `MIN(pct, 25) * 0.1 + pct * 0.05 + 2 + (target + buyer + CurYear + CurQuarter) MOD 6 +
/// CEIL(pct / holder_pct * 5)` percent, at least 5 (above 15 it becomes 18); result
/// `MarketCap * (1 + premium / 100) * 1.01`. With `holder_pct = 0` the ratio is infinite and the
/// x87 "integer indefinite" (-2^31) is added, so the premium falls to the minimum of 5%.
pub fn calc_takeover_premium_price(g: &Globals, buyer_id: i32, target_id: i32, pct_to_buy: i32, holder_pct: i16) -> Ext {
    let mut capped_pct = 25;
    if pct_to_buy < 25 {
        capped_pct = pct_to_buy;
    }
    let ratio: Ext = crate::pb::numeric::double(cast::<Ext, _>(pct_to_buy) / cast::<Ext, _>(holder_pct)); // stored as DOUBLE
    // CEIL(ratio * 5) on the x87: the product is not rounded to a double (0.2 * 5 > 1 there).
    let x = Ext80::from_ext(ratio).fmul(Ext80::from_i64(5));
    let ceil_part = if x.is_nan() {
        i32::MIN
    } else {
        let r = x.frndint();
        if r.cmp_val(x) == std::cmp::Ordering::Less { r.fadd(Ext80::from_i64(1)) } else { r }.to_i32()
    };
    let mut premium_pct: Ext = cast::<Ext, _>(pct_to_buy) * sf(0.05f32)
        + Ext::from_f64(2.0)
        + fmod(cast::<Ext, _>(target_id + buyer_id + g.cur_year + g.cur_quarter), Ext::from_f64(6.0));
    premium_pct = cast::<Ext, _>(capped_pct) * sf(0.1f32) + premium_pct + cast::<Ext, _>(ceil_part);
    if Ext::from_f64(5.0) > premium_pct {
        premium_pct = Ext::from_f64(5.0);
    }
    if Ext::from_f64(15.0) < premium_pct {
        premium_pct = Ext::from_f64(18.0);
    }
    (sf(0.01f32) * premium_pct + Ext::from_f64(1.0)) * g.market_cap[target_id] * sf(1.01f32)
}

/// Port of SellStock (0x5ab449).
///
/// The player's stock sale for the acting entity (`ActiveEntity`), or for an ETF the player advises
/// ([`get_selected_player_etf`]: YES = the fund sells, NO = the acting entity, CANCEL = back). The
/// holding is picked from the portfolio listing (`ShowPortfolioListing`, `ActionCode = "SELL"`,
/// `TargetCorp`); refusals ("CANNOT SELL ITSELF", stock not owned, unlisted stock) end in a "Sale
/// Canceled" box. Stock without a public float (difficulty 2+) is first offered to a private buyer
/// ([`private_buyout_offer`], done when accepted). The player enters the percent to sell (up to the
/// holding); for the acting entity a possible capital-gains tax (and the recapture of an excess loss
/// account below 80%) is warned about (YES/NO/CANCEL). The sale itself is `SellStockHolding`
/// (`TmpExt1` = seller, `TmpExt2` = percent). Afterwards for the acting entity: naked options over the
/// holding are closed, the projected earnings are recomputed for a company with proceeds, and stock of a
/// private or unseasoned company is bought by a random company that can pay (the "in private sale"
/// headline); an ETF seller only resets its fund-bonus year. `time_secs` is `VAL(RIGHT$(TIME$, 2))`
/// for the valuation (`CalcStockPrice`).
// PB locals start zeroed; the port keeps them initialised like the original.
#[allow(unused_assignments)]
pub fn sell_stock(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    let __content = crate::content::current();
    let mut seller_id = g.active_entity;
    let mut seller_name = g.active_entity_name.clone();
    let mut etf_seller = get_selected_player_etf(g, ui, rng);
    let mut msg_text = PbStr::new();
    let mut holding_count = 0;
    let mut pct_to_sell = 0;
    let mut held_pct = 0;
    let mut hold_slot: i16 = 0;
    if 1580 < etf_seller && (1585 < etf_seller || g.hold_stock_id[(0, etf_seller)] != 0) {
        let mut prompt_text = g.ent_name[etf_seller].clone()
            + " ("
            + &g.ent_symbol[etf_seller]
            + __content.string(3017)
            + __content.string(3018)
            + &g.ent_name[g.fund_adviser[etf_seller]]
            + ". "
            + __content.string(12827)
            + "\r"
            + "\r"
            + __content.string(3020)
            + &g.ent_name[etf_seller]
            + __content.string(12828)
            + "\r"
            + "\r"
            + __content.string(12829);
        if etf_seller < 1586 {
            prompt_text = remove(prompt_text.as_bytes(), __content.bytes(12830));
        }
        if g.hold_stock_id[(0, g.active_entity)] == 0 {
            prompt_text = prompt_text + __content.string(12831);
        } else {
            prompt_text = prompt_text + __content.string(12832) + &g.active_entity_name + __content.string(12833);
        }
        if g.active_entity < 11 {
            prompt_text = replace(prompt_text.as_bytes(), __content.bytes(9086), b".");
        }
        let answer = ui.msgbox(&prompt_text, 8483, &s(__content.string(12523)));
        if answer == 2 {
            return;
        }
        if answer == 7 {
            if g.hold_stock_id[(0, g.active_entity)] == 0 {
                return;
            }
            etf_seller = 0;
        } else {
            seller_id = etf_seller;
            seller_name = g.ent_name[etf_seller].clone();
        }
    }
    for i in 1..=15 {
        if 10 < g.hold_stock_id[(i, seller_id)] && 0 < g.hold_stock_pct[(i, seller_id)] {
            holding_count = holding_count + 1;
        }
    }
    if holding_count == 0 {
        msg_text = s("You do");
        if 10 < seller_id {
            msg_text = seller_name.clone() + " does";
        }
        let mut m = msg_text.clone() + __content.string(12834);
        show_quick_message(g, ui, &mut m);
        return;
    }
    g.tmp_symbol = PbStr::new();
    g.action_code = s("SELL");
    g.entity_pick_mode = s("GetCorp");
    g.target_corp = 0;
    g.tmp_ext1 = -Ext::from_f64(1.0);
    show_portfolio_listing(g, ui, rng, seller_id, 0, 0);
    if holding_count == 0 {
        return;
    }
    holding_count = 0;
    pct_to_sell = 0;
    if g.target_corp == 0 {
        g.action_code = PbStr::new();
        return;
    }
    let corp_id = g.target_corp;
    g.target_corp = 0;
    if seller_id == corp_id {
        msg_text = seller_name.clone() + __content.string(12835);
        if seller_id < 11 {
            msg_text = s(__content.string(12836));
        }
    }
    update_corp_control(g, ui, rng, corp_id);
    for i in 1..=15 {
        if cast::<i32, _>(g.hold_stock_id[(i, seller_id)]) == corp_id {
            held_pct = cast::<i32, _>(g.hold_stock_pct[(i, seller_id)]);
            pct_to_sell = held_pct;
            hold_slot = cast::<i16, _>(i);
            break;
        }
    }
    if pct_to_sell == 0 {
        msg_text = seller_name.clone() + __content.string(12837) + &g.ent_name[corp_id] + "!";
    } else if g.public_pct == 0 && 1 < g.difficulty_level && g.industry_num[corp_id] != 71 {
        if 0 < private_buyout_offer(g, ui, rng, seller_id, corp_id) {
            return;
        }
        if g.earnings_yr3_ago[corp_id] == Ext::from_f64(0.0) && g.earnings_yr4_ago[corp_id] == Ext::from_f64(0.0) {
            msg_text = g.ent_name[corp_id].clone()
                + __content.string(12838)
                + __content.string(12839)
                + __content.string(12840)
                + __content.string(12841)
                + __content.string(12842);
            if g.earnings_y1[corp_id] == Ext::from_f64(0.0) && g.earnings_yr2_ago[corp_id] == Ext::from_f64(0.0) {
                msg_text = replace(msg_text.as_bytes(), __content.bytes(12843), b"no");
            }
        } else {
            msg_text = g.ent_name[corp_id].clone()
                + " ("
                + &g.ent_symbol[corp_id]
                + ") is"
                + &str_s(cast::<Ext, _>(held_pct))
                + __content.string(12844)
                + &seller_name
                + __content.string(12845)
                + "\r"
                + "\r"
                + &g.ent_symbol[corp_id]
                + __content.string(12846)
                + __content.string(12847)
                + &seller_name
                + __content.string(12848)
                + __content.string(12849)
                + __content.string(12850);
        }
    }
    if !msg_text.is_empty() {
        ui.msgbox(&msg_text, 8208, &s(__content.string(12851)));
        g.action_code = PbStr::new();
        return;
    }
    let mut est_value = g.hold_stock_basis[(cast::<i32, _>(hold_slot), seller_id)] / cast::<Ext, _>(pct_to_sell);
    est_value = est_value / g.shares_outstanding[corp_id];
    let cost_text = fmt_trim(est_value, "############.00");
    calc_stock_price(g, ui, rng, corp_id, time_secs); // in the ValuationMode left by the last valuation
    if g.industry_num[corp_id] == 71 {
        let mut m = s(__content.string(4025))
            + &g.ent_name[corp_id]
            + __content.string(12852)
            + __content.string(3263)
            + &g.stock_price_str
            + __content.string(12853);
        show_quick_message(g, ui, &mut m);
    }
    while holding_count == 0 {
        msg_text = seller_name.clone()
            + " owns"
            + &str_s(cast::<Ext, _>(pct_to_sell))
            + __content.string(2163)
            + &g.ent_name[corp_id]
            + ". "
            + __content.string(12854)
            + &str_s(cast::<Ext, _>(pct_to_sell))
            + __content.string(12855)
            + &g.ent_symbol[seller_id]
            + __content.string(12856)
            + "\r\n"
            + "\r\n"
            + __content.string(12857)
            + &g.stock_price_str
            + "\r\n"
            + __content.string(12858)
            + &cost_text
            + "\r\n"
            + __content.string(12859);
        let title = g.ent_symbol[seller_id].clone() + __content.string(12860);
        let default = str_trim(cast::<Ext, _>(pct_to_sell)) + "%";
        let input_text = ui.inputbox(&msg_text, &title, &default);
        let entered = val(remove_any(input_text.as_bytes(), b"$ ,%").as_bytes());
        if entered <= cast::<Ext, _>(pct_to_sell) {
            holding_count = 1;
            pct_to_sell = to_long(entered);
        } else {
            msg_text = s(__content.string(2167))
                + &seller_name
                + __content.string(2168)
                + &str_s(cast::<Ext, _>(pct_to_sell))
                + "% OF "
                + &g.ent_symbol[corp_id]
                + "!";
            show_quick_message(g, ui, &mut msg_text);
        }
    }
    g.action_code = PbStr::new();
    if pct_to_sell < 1 {
        let mut m = s(__content.string(12014));
        show_quick_message(g, ui, &mut m);
        return;
    }
    if etf_seller == 0 {
        let ae = g.active_entity;
        for i in 1..=15 {
            if cast::<i32, _>(g.hold_stock_id[(i, ae)]) != corp_id {
                continue;
            }
            let mut sold_basis =
                cast::<Ext, _>(pct_to_sell) / cast::<Ext, _>(g.hold_stock_pct[(i, ae)]) * g.hold_stock_basis[(i, ae)];
            if g.cur_player == ae {
                if g.player_cap_gains[ae] < Ext::from_f64(0.0) {
                    sold_basis = sold_basis - g.player_cap_gains[ae];
                }
            } else {
                let est_tax0 =
                    to_quad(g.div_recvd1[ae] * sf(0.2f32) + g.accrued_pl[ae] + g.div_recvd2[ae] * sf(0.3f32));
                let est_tax = to_quad(
                    cast::<Ext, _>(g.extraordinary_pl[ae]) + cast::<Ext, _>(est_tax0) + g.consol_tax_inc_ytd[ae] + g.tax_loss_carry[ae],
                );
                if (cast::<Ext, _>(est_tax)) < Ext::from_f64(0.0) {
                    sold_basis = sold_basis - cast::<Ext, _>(est_tax);
                }
            }
            let mut est_value = g.market_cap[corp_id] * cast::<Ext, _>(pct_to_sell) * sf(0.9f32);
            if g.industry_num[corp_id] == 71 {
                est_value = g.market_cap[corp_id] * cast::<Ext, _>(pct_to_sell) * sf(0.99f32);
            }
            let mut est_tax = to_quad(est_value - sold_basis);
            if sold_basis * sf(1.1f32) < est_value && g.exchange_rate * Ext::from_f64(10.0) < cast::<Ext, _>(est_tax) {
                if g.cur_player == ae {
                    est_tax = to_quad(sf(0.01f32) * cast::<Ext, _>(g.cap_gains_tax_rate) * cast::<Ext, _>(est_tax));
                    msg_text = s(__content.string(12861));
                } else {
                    est_tax = to_quad(sf(0.01f32) * cast::<Ext, _>(g.corp_tax_rate) * cast::<Ext, _>(est_tax));
                    msg_text = g.active_entity_name.clone() + __content.string(12862);
                }
                msg_text = msg_text.clone()
                    + &trim(g.currency_symbol.as_bytes())
                    + &fmt_trim(cast::<Ext, _>(est_tax), "* ###,")
                    + " "
                    + &g.million_prefix_lc
                    + "illion"
                    + &g.currency_suffix
                    + "."
                    + "\r"
                    + "\r";
                if g.hold_stock_basis[(i, ae)] < Ext::from_f64(0.0) && 80 <= g.hold_stock_pct[(i, ae)] && g.cur_player != ae {
                    let remain_pct = cast::<i32, _>(g.hold_stock_pct[(i, ae)]) - pct_to_sell;
                    let excess_loss = (sf(0.01f32) * cast::<Ext, _>(remain_pct) * g.hold_stock_basis[(i, ae)]).abs();
                    if remain_pct < 80 && 0 < remain_pct && Ext::from_f64(10.0) < excess_loss {
                        msg_text = msg_text.clone()
                            + __content.string(12863)
                            + &g.ent_symbol[ae]
                            + __content.string(12864)
                            + __content.string(12865)
                            + __content.string(12866)
                            + __content.string(12867)
                            + &g.active_entity_name
                            + "."
                            + "\r"
                            + "\r";
                    }
                }
                msg_text = msg_text.clone() + __content.string(12868) + &g.ent_symbol[corp_id] + " STOCK?";
                let answer = ui.msgbox(&msg_text, 8227, &s(__content.string(12869)));
                if answer == 2 || answer == 7 {
                    return;
                }
            }
        }
    }
    update_corp_control(g, ui, rng, corp_id);
    let public_pct_before = g.public_pct;
    g.tmp_ext1 = cast::<Ext, _>(seller_id);
    g.tmp_ext2 = cast::<Ext, _>(pct_to_sell);
    g.tmp_ext3 = Ext::from_f64(0.0);
    sell_stock_holding(g, ui, rng, corp_id);
    if 0 < etf_seller {
        g.tmp_ext1 = Ext::from_f64(0.0);
        if 0 < g.fund_bonus_year[seller_id] {
            g.fund_bonus_year[seller_id] = g.cur_year;
        }
        pack_holdings(g, ui);
        return;
    }
    if g.cur_player == g.corp_controller[corp_id] && g.cur_player == seller_id {
        update_corp_control(g, ui, rng, corp_id);
        let cp = g.cur_player;
        force_close_excess_naked_options(g, ui, rng, cp, corp_id);
    } else if 10 < seller_id {
        let ae = g.active_entity;
        force_close_excess_naked_options(g, ui, rng, ae, corp_id);
    }
    let sale_proceeds = g.tmp_ext1;
    if Ext::from_f64(0.0) < sale_proceeds && 10 < g.active_entity {
        let ae = g.active_entity;
        calc_projected_earnings(g, ui, rng, ae);
    }
    g.tmp_ext1 = Ext::from_f64(0.0);
    holding_count = 1;
    if g.earnings_yr4_ago[corp_id] == Ext::from_f64(0.0) && g.earnings_yr3_ago[corp_id] == Ext::from_f64(0.0) {
        holding_count = 0;
    }
    if g.industry_num[corp_id] != 71 && (public_pct_before == 0 || holding_count == 0) {
        pack_holdings(g, ui);
        let start_idx = to_long(Ext::from_f64(70.0) * rng.rnd() + Ext::from_f64(86.0));
        let mut i = start_idx;
        while i <= 1600 {
            if g.entity_status[i] == 1
                && g.industry_num[i] != 71
                && g.active_entity != i
                && corp_id != i
                && g.hold_stock_id[(0, i)] <= 5
            {
                calc_entity_financials(g, ui, rng, i);
                if g.liquid_funds >= sale_proceeds && (g.corp_controller[i] <= 0 || 11 <= g.corp_controller[i]) {
                    g.cash[i] = g.cash[i] - sale_proceeds;
                    let buyer_slot = cast::<i32, _>(cast::<i16, _>(find_holding_slot(g, i, corp_id)));
                    g.hold_stock_id[(buyer_slot, i)] = cast::<i16, _>(corp_id);
                    g.hold_stock_pct[(buyer_slot, i)] =
                        (cast::<i16, _>(pct_to_sell)).wrapping_add(g.hold_stock_pct[(buyer_slot, i)]);
                    g.hold_stock_basis[(buyer_slot, i)] = sale_proceeds + g.hold_stock_basis[(buyer_slot, i)];
                    msg_text = g.news_headline[60].clone();
                    if 0 < instr(1, msg_text.as_bytes(), __content.bytes(12870)) {
                        msg_text = remove(msg_text.as_bytes(), __content.bytes(12870));
                        g.news_headline[60] = msg_text.clone();
                    }
                    g.news_msg = g.ent_symbol[i].clone()
                        + " buys"
                        + &str_s(cast::<Ext, _>(pct_to_sell))
                        + "% of "
                        + &g.ent_symbol[corp_id]
                        + __content.string(12023)
                        + &g.ent_symbol[g.active_entity]
                        + __content.string(12871);
                    post_news(g, ui, 0, 0);
                    g.investor_sentiment[corp_id] = pct_to_sell + g.investor_sentiment[corp_id];
                    i = 1600;
                    pack_holdings(g, ui);
                }
            }
            i += 1;
        }
    }
    update_control_chain(g, ui, rng, corp_id);
    let ae = g.active_entity;
    update_control_chain(g, ui, rng, ae);
}

/// Locals of PrivateBuyoutOffer shared with its GOSUB PrivateBuyoutOffer_FindBuyer.
struct Buyout {
    seller_id: i32,
    corp_id: i32,
    offer_price: Ext,
    buyer_id: i32,
    buyer_slot: i32,
    start_id: i32,
    /// Passes of the search; not reset between the two searches.
    tries: i32,
}

/// Port of PrivateBuyoutOffer (0x5bcd00) with its GOSUBs PrivateBuyoutOffer_FindBuyer (0x5bd87e) and
/// PrivateBuyoutOffer_Exit (0x5bda81, the common exit returning the result).
///
/// A private buyer for `seller_id`'s holding of the unlisted `corp_id`. Price: 80% (seasoned: earnings
/// three or four years ago) or 72% of the appraised value for a control block, 70% / 56% for a minority
/// stake, each `+ (CurYear + CurQuarter + seller + corp) MOD 8`. A holding over 50% is first offered as a
/// 51% block (priced `0.51 * MarketCap * pricePct`, missing the `* 0.01`: BUGS.md #242); when
/// `(CurQuarter + corp) MOD 2 = 1` or no buyer is found, a minority stake is offered instead (over 20%:
/// 60..90% of it, over 30%: 18..26%). The buyer is a random active non-fund company (or `AiTargetCorp`)
/// not run by the current player or a human player, with the liquid funds and a free holding slot.
/// The player is asked (YES/NO); on YES the stake, cash, basis and the seller's capital gain (players)
/// or extraordinary P/L (companies) are booked, the buyer covers negative cash, the deal is shown and
/// posted, and holdings / cross-ownership are tidied up.
///
/// Returns 1 when sold, else 0 (no stake, no buyer, declined).
pub fn private_buyout_offer(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, seller_id: i32, corp_id: i32) -> i32 {
    let __content = crate::content::current();
    let seller_slot = find_holding_slot(g, seller_id, corp_id);
    let held_pct: i16 = g.hold_stock_pct[(seller_slot, seller_id)];
    let mut pct_to_sell: i16 = held_pct;
    if held_pct == 0 {
        return 0;
    }
    let mut is_seasoned = 0;
    let mut control_price_pct;
    let mut minority_price_pct;
    if g.earnings_yr3_ago[corp_id] == Ext::from_f64(0.0) && g.earnings_yr4_ago[corp_id] == Ext::from_f64(0.0) {
        control_price_pct = 72;
        minority_price_pct = 56;
    } else {
        is_seasoned = 1;
        control_price_pct = 80;
        minority_price_pct = 70;
    }
    let mut f = Buyout {
        seller_id,
        corp_id,
        offer_price: Ext::from_f64(0.0),
        buyer_id: 0,
        buyer_slot: 0,
        start_id: (g.cur_year + g.cur_quarter + seller_id + corp_id) % 8,
        tries: 0,
    };
    control_price_pct = control_price_pct + f.start_id;
    minority_price_pct = minority_price_pct + f.start_id;
    let mut control_deal = 0;
    let mut price_pct = 0;
    if 50 < pct_to_sell {
        f.offer_price = sf(0.51f32) * g.market_cap[corp_id] * cast::<Ext, _>(control_price_pct);
        buyout_find_buyer(g, ui, rng, &mut f);
        if 10 < f.buyer_id {
            control_deal = 1;
            pct_to_sell = 51;
            price_pct = control_price_pct;
        }
    }
    if (g.cur_quarter + corp_id) % 2 == 1 {
        control_deal = 0;
        f.buyer_slot = 0;
    }
    if control_deal == 0 {
        if 20 < pct_to_sell {
            pct_to_sell = to_integer(cast::<Ext, _>(pct_to_sell) * (sf(0.3f32) * rng.rnd() + sf(0.6f32)));
        }
        if 30 < pct_to_sell {
            pct_to_sell = to_integer(Ext::from_f64(8.0) * rng.rnd() + Ext::from_f64(18.0));
        }
        f.offer_price = cast::<Ext, _>(pct_to_sell) * g.market_cap[corp_id] * cast::<Ext, _>(minority_price_pct) * sf(0.01f32);
        buyout_find_buyer(g, ui, rng, &mut f);
        if f.buyer_id < 11 {
            return 0;
        }
        price_pct = minority_price_pct;
    }
    let buyer_id = f.buyer_id;
    let buyer_slot = f.buyer_slot;
    let offer_price = f.offer_price;
    let mut msg_text = s(__content.string(1093))
        + &g.ent_name[corp_id]
        + __content.string(12872)
        + &g.ent_symbol[seller_id]
        + " "
        + __content.string(12873);
    if held_pct < 100 {
        let with = PbStr::from(str_num(cast::<Ext, _>(held_pct), 7).as_str()) + "%";
        msg_text = replace(msg_text.as_bytes(), b" wholly", with.as_bytes());
    }
    let mut reason_text = s(__content.string(12874));
    if is_seasoned == 1 {
        reason_text = s(".");
    }
    msg_text = msg_text + &reason_text + "\r" + "\r";
    msg_text = msg_text
        + __content.string(12875)
        + __content.string(12876)
        + &PbStr::from(str_num(cast::<Ext, _>(pct_to_sell), 7).as_str())
        + __content.string(12877)
        + &str_s(cast::<Ext, _>(price_pct))
        + __content.string(9121)
        + __content.string(12878)
        + &g.ent_name[buyer_id];
    if right(msg_text.as_bytes(), 1) != "." {
        msg_text = msg_text + ".";
    }
    msg_text = msg_text + "\r" + "\r";
    msg_text = msg_text + __content.string(12879) + &g.ent_symbol[corp_id] + __content.string(12880);
    let answer = ui.msgbox(&msg_text, 8228, &s(__content.string(12881)));
    if answer != 6 {
        return 0;
    }
    g.cash[seller_id] = offer_price + g.cash[seller_id];
    let mut basis_sold = Ext::from_f64(0.0);
    let held_now = cast::<i32, _>(g.hold_stock_pct[(seller_slot, seller_id)]);
    if 0 < held_now {
        basis_sold = g.hold_stock_basis[(seller_slot, seller_id)] * (cast::<Ext, _>(pct_to_sell) / cast::<Ext, _>(held_now));
    }
    if cast::<i32, _>(pct_to_sell) == held_now {
        basis_sold = g.hold_stock_basis[(seller_slot, seller_id)];
    }
    if seller_id < 11 {
        g.player_cap_gains[seller_id] = (offer_price - basis_sold) + g.player_cap_gains[seller_id];
    } else {
        g.extraordinary_pl[seller_id] = to_quad((offer_price - basis_sold) + cast::<Ext, _>(g.extraordinary_pl[seller_id]));
    }
    g.hold_stock_basis[(seller_slot, seller_id)] = g.hold_stock_basis[(seller_slot, seller_id)] - basis_sold;
    g.hold_stock_pct[(seller_slot, seller_id)] = g.hold_stock_pct[(seller_slot, seller_id)].wrapping_sub(pct_to_sell);
    if g.hold_stock_pct[(seller_slot, seller_id)] == 0 {
        g.hold_stock_id[(seller_slot, seller_id)] = 0;
        g.hold_stock_basis[(seller_slot, seller_id)] = Ext::from_f64(0.0);
    }
    g.cash[buyer_id] = g.cash[buyer_id] - offer_price;
    if g.cash[buyer_id] < Ext::from_f64(0.0) {
        cover_negative_cash(g, ui, rng, buyer_id, 1);
    }
    if g.hold_stock_id[(buyer_slot, buyer_id)] == 0 {
        g.hold_stock_id[(buyer_slot, buyer_id)] = cast::<i16, _>(corp_id);
    }
    g.hold_stock_pct[(buyer_slot, buyer_id)] = pct_to_sell.wrapping_add(g.hold_stock_pct[(buyer_slot, buyer_id)]);
    g.hold_stock_basis[(buyer_slot, buyer_id)] = offer_price + g.hold_stock_basis[(buyer_slot, buyer_id)];
    g.news_msg = g.ent_symbol[buyer_id].clone()
        + __content.string(10971)
        + &PbStr::from(str_num(cast::<Ext, _>(pct_to_sell), 7).as_str())
        + "% of "
        + &g.ent_symbol[corp_id]
        + __content.string(12882)
        + &g.ent_symbol[seller_id];
    let mut m = g.news_msg.clone() + __content.string(12883);
    show_quick_message(g, ui, &mut m);
    post_news(g, ui, 0, 0);
    if 10 < seller_id {
        recapture_excess_loss(g, ui, corp_id, seller_id);
    }
    pack_holdings(g, ui);
    check_cross_ownership(g, ui, rng, corp_id, 1);
    check_cross_ownership(g, ui, rng, buyer_id, 1);
    1
}

/// Port of PrivateBuyoutOffer_FindBuyer (0x5bd87e), GOSUB of PrivateBuyoutOffer.
/// Scans from a random company 86..1586 (or `AiTargetCorp`, one time in two) up to 1600 for the first
/// active non-fund company other than seller and stock, not controlled by the current player nor by a
/// human player, whose `LiquidFunds` cover `offer_price` and that has a holding slot for the stock.
/// Without a match, restarts at 86..106 while `tries` (shared by both searches) is at most 2.
fn buyout_find_buyer(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Buyout) {
    f.buyer_id = 0;
    f.start_id = to_long(Ext::from_f64(1500.0) * rng.rnd() + Ext::from_f64(86.0));
    if 10 < g.ai_target_corp && Ext::from_f64(2.0) * rng.rnd() < Ext::from_f64(1.0) {
        f.start_id = g.ai_target_corp;
    }
    loop {
        f.tries = f.tries + 1;
        let mut cand = f.start_id;
        while cand <= 1600 {
            if g.entity_status[cand] == 1 && f.seller_id != cand && f.corp_id != cand && g.industry_num[cand] != 71 {
                let controller_id = g.corp_controller[cand];
                if g.cur_player != controller_id
                    && !(0 < controller_id && controller_id < 11 && g.is_computer_player[controller_id] == 0)
                {
                    calc_entity_financials(g, ui, rng, cand);
                    if g.liquid_funds >= f.offer_price {
                        f.buyer_slot = find_holding_slot(g, cand, f.corp_id);
                        if f.buyer_slot != 0 {
                            f.buyer_id = cand;
                            cand = 1600;
                        }
                    }
                }
            }
            cand += 1;
        }
        if 2 < f.tries {
            return;
        }
        if f.buyer_id != 0 || f.start_id <= 106 {
            return;
        }
        f.start_id = to_long(Ext::from_f64(20.0) * rng.rnd() + Ext::from_f64(86.0));
    }
}
