//! The player's stock purchase `BuyStockMenu` (0x58d471) with its GOSUBs: open-market purchases and
//! tender offers (through [`buy_stock`]), private purchases from a holder (with a negotiated premium for an
//! uncontrolled seller), purchases from a posted sale offer (`AssetSaleOffers`) and ETF purchases by a
//! fund the player advises.

// Keep the original's statement shapes.
#![allow(
    clippy::assign_op_pattern,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::if_same_then_else,
    clippy::manual_range_contains,
    clippy::needless_late_init,
    // NaN-aware comparisons mirror the x87 branches
    clippy::neg_cmp_op_on_partial_ord
)]
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use super::select::{get_selected_player_etf, in_box_symbol};
use super::short_menu::cover_short_position;
use super::stock::{buy_stock, calc_takeover_premium_price};
use super::borrow::borrow_to_cover_purchase;
use crate::logic::control::update_corp_control;
use crate::logic::corp_finance::stock::stock_buyback_menu;
use crate::logic::earnings::{calc_projected_earnings, sell_t_bills_to_cash};
use crate::logic::etf_derivs::corp_options::force_close_excess_naked_options;
use crate::logic::events::antitrust::antitrust_review_acquisition;
use crate::logic::finance::{calc_entity_financials, get_controller_type};
use crate::logic::holdings::{find_holding_slot, find_stock_holding};
use crate::logic::market::select_target_corp;
use crate::logic::newgame::LocalTime;
use crate::logic::newgame_helpers::pick_etf_holding_stock;
use crate::logic::report::number_to_words;
use crate::logic::reports_corp::check_cross_ownership;
use crate::logic::reports_lists::update_control_chain;
use crate::logic::research::get_etf_description;
use crate::logic::restructure::merge::show_runtime_error;
use crate::logic::restructure::tender::negotiate_premium_buyout;
use crate::logic::sim_entity::asset_sales::{
    add_remove_asset_sale_offer, calc_group_stake_pct, notify_discount_offer, reduce_asset_sale_offer,
};
use crate::logic::sim_entity::bonds::force_regulated_call_sales;
use crate::logic::sim_entity::quarter_start::recapture_excess_loss;
use crate::logic::valuation::{calc_stock_price, price_trend_streak};
use crate::pb::string::mcase;

/// Locals of BuyStockMenu shared with its GOSUBs.
#[derive(Default)]
struct Bsm {
    buyer_id: i32,
    buyer_name: PbStr,
    etf_buyer: i32,
    target_id: i32,
    offer_seller_id: i32,
    seller_id: i32,
    pct_to_buy: i32,
    max_pct: i16,
    avail_pct: i16,
    group_stake_pct: i32,
    us_corp_id: i32,
    sec_notice_shown: i32,
    price_per_pct: Ext,
    total_cost: Ext,
    buying_power: Ext,
    is_private_tender: i32,
    premium_negotiated: i32,
    negotiated_premium: i32,
    legal_fees: i64,
    msg_text: PbStr,
    aux_text: PbStr,
    /// The routine's loop variables `i` / `j` (also used by the GOSUBs).
    i: i32,
    j: i32,
    ent_idx: i32,
    tmp_slot: i32,
    short_corp_id: i32,
    group_pct: i32,
    orig_pct: i32,
    saved_slot: i32,
    answer: i32,
}

/// Port of BuyStockMenu (0x58d471) with its GOSUBs BuyStockMenu_CalcPrice (0x594362),
/// BuyStockMenu_LimitByFunds (0x594c07), BuyStockMenu_PremiumOffer (0x594c8b),
/// BuyStockMenu_ChargeLegalFees (0x594dc3), BuyStockMenu_InsiderConflictCheck (0x594f20),
/// BuyStockMenu_Max15HoldingsMsg (0x595811) and BuyStockMenu_ExitSub (0x595966, EXIT SUB).
///
/// The acting entity (`ActiveEntity`) — or an ETF (1586..1600) the player advises, if the player says
/// YES — buys stock. `TmpExt1` (cleared) may name a sale offer 1..100 of `AssetSaleOffers`
/// ("seller|stock|percent|..."); otherwise the company is entered in the symbol dialog ([`in_box_symbol`],
/// `ActionCode = "BUYSTOCK"`). Many checks may end the menu: bank regulators (a bank with over 50% of its
/// net worth in stocks), buying power, 15 holdings, ETF charter rules, insider purchases during a pending
/// merger, a self-purchase (a buyback via [`stock_buyback_menu`]), the bank-concentration rule, the
/// broker's warning on a falling stock, an own short position ([`cover_short_position`]), no float.
/// Holders the player may buy from (his own companies, uncontrolled companies) can be chosen in a pick
/// list; a private purchase from an uncontrolled holder needs a premium (planned premium, then
/// [`negotiate_premium_buyout`], with 1% legal fees). The percentage is entered (INPUTBOX); a fund buys
/// directly at the market price + 1%, a private purchase is booked here (with the antitrust review, the
/// seller's gain or disallowed related-party loss, news), an open-market purchase / tender offer is
/// confirmed and done by [`buy_stock`] (a cash shortfall is borrowed with
/// [`borrow_to_cover_purchase`]). Afterwards control, cross-ownership, regulated call sales and naked
/// options are updated; the "Japan" cheat may start scenario 104.
///
/// `now` is the local time (`TIME$` seconds for the valuations and `SelectTargetCorp`, `DATE$` for
/// `NegotiatePremiumBuyout`, both for `StockBuybackMenu`).
///
/// Error paths: the ON ERROR handler (ShowRuntimeError "FrontBuyStock", then EXIT SUB) is ported for
/// the seller pick list growing past `sellerLines(1 TO 100)`; the other error-9 spots need a target id
/// below 11, which the symbol dialog does not deliver.
// PB locals start zeroed; the port keeps them initialised like the original.
#[allow(unused_assignments)]
pub fn buy_stock_menu(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime) {
    let __content = crate::content::current();
    let mut f = Bsm::default();
    let mut seller_lines: Vec<PbStr> = vec![PbStr::new(); 101];
    let mut seller_count = 0;
    let mut seller_slot: i16 = 0;
    let mut buyer_slot;
    let mut held_pct = 0;
    let mut purchase_ready;
    let mut offer_avail_pct;
    let sale_offer_rec;
    let mut input_text = PbStr::new();
    let mut seller_text = PbStr::new();
    let mut seller_line;
    let mut premium_pct;
    let mut num;
    let mut affordable_ratio = Ext::from_f64(0.0);
    let mut borrow_needed = Ext::from_f64(0.0);
    let mut premium_cost;
    let mut sold_basis = Ext::from_f64(0.0);
    let mut group_etf_pct: i16 = 0;
    let mut etf_room_pct: i16 = 0;
    let saved_market_cap;
    let prev_controller;

    let sale_offer_idx = to_long(g.tmp_ext1);
    g.tmp_ext1 = Ext::from_f64(0.0);
    f.buyer_id = g.active_entity;
    f.buyer_name = g.active_entity_name.clone();
    f.etf_buyer = get_selected_player_etf(g, ui, rng);
    if 1585 < f.etf_buyer {
        let etf = f.etf_buyer;
        f.msg_text = g.ent_name[etf].clone()
            + " ("
            + &g.ent_symbol[etf]
            + __content.string(3017)
            + __content.string(3018)
            + &g.ent_name[g.fund_adviser[etf]]
            + ". "
            + __content.string(11898)
            + "\r"
            + "\r"
            + __content.string(3020)
            + &g.ent_name[etf]
            + __content.string(11899)
            + "\r"
            + "\r"
            + __content.string(3567)
            + &g.active_entity_name
            + __content.string(11900);
        f.answer = ui.msgbox(&f.msg_text, 8483, &s(__content.string(11901)));
        if f.answer == 2 {
            return;
        }
        if f.answer == 7 {
            f.etf_buyer = 0;
        } else {
            f.buyer_id = f.etf_buyer;
            f.buyer_name = g.ent_name[f.etf_buyer].clone();
        }
    } else if 1580 < f.etf_buyer {
        f.etf_buyer = 0;
    }
    let buyer_id = f.buyer_id;
    calc_entity_financials(g, ui, rng, buyer_id);
    f.buying_power = g.liquid_funds;
    if 10 < buyer_id && g.industry_num[buyer_id] == 1 {
        calc_entity_financials(g, ui, rng, buyer_id);
        if g.tmp_subsid_stock_value > g.net_worth * sf(0.5f32) || !(g.net_worth > Ext::from_f64(0.0)) {
            input_text = s("50");
            if g.tmp_subsid_stock_value > g.net_worth {
                input_text = s("100");
            }
            f.msg_text = f.buyer_name.clone()
                + __content.string(11902)
                + &input_text
                + __content.string(11903)
                + __content.string(11904)
                + __content.string(11905)
                + __content.string(11906)
                + "\r"
                + "\r"
                + __content.string(11907)
                + &g.ent_symbol[buyer_id]
                + __content.string(11908)
                + __content.string(11909)
                + __content.string(11910);
            if !(g.net_worth > Ext::from_f64(0.0)) {
                f.msg_text = f.buyer_name.clone()
                    + __content.string(11911)
                    + "\r"
                    + "\r"
                    + __content.string(11912)
                    + __content.string(11913);
            }
            ui.msgbox(&f.msg_text, 8208, &s(__content.string(11914)));
            return;
        }
    }
    if 0 < sale_offer_idx && sale_offer_idx < 101 {
        sale_offer_rec = g.asset_sale_offers[sale_offer_idx].clone();
        f.offer_seller_id = to_long(val(parse_any(sale_offer_rec.as_bytes(), b"|", 1).as_bytes()));
        f.target_id = to_long(val(parse_any(sale_offer_rec.as_bytes(), b"|", 2).as_bytes()));
        if f.target_id == 0 {
            f.offer_seller_id = 0;
        } else {
            f.pct_to_buy = to_long(val(parse_any(sale_offer_rec.as_bytes(), b"|", 3).as_bytes()));
        }
    }
    if f.offer_seller_id == 0 {
        g.tmp_symbol = PbStr::new();
        g.action_code = s("BUYSTOCK");
        f.msg_text = s(__content.string(11815));
        f.msg_text = f.msg_text.clone()
            + __content.string(11915)
            + &f.buyer_name
            + __content.string(11916)
            + __content.string(11917)
            + "\r\n"
            + "\r\n";
        g.input_dlg_label = PbStr::new();
        g.input_dlg_prompt = f.msg_text.clone();
        g.input_dlg_title = s(__content.string(11918)) + &f.buyer_name;
        g.lookup_filter = PbStr::new();
        if 1585 < f.etf_buyer {
            g.action_code = s("ETF_TRADES");
        }
        in_box_symbol(g, ui, rng);
        g.action_code = s("BUYSTOCK");
        f.msg_text = PbStr::new();
        if g.tmp_symbol.is_empty() {
            return;
        }
        f.target_id = g.target_corp;
        g.target_corp = 0;
    }
    let target_id = f.target_id;
    if 10 < target_id {
        f.msg_text = f.buyer_name.clone()
            + __content.string(11919)
            + &g.ent_name[target_id];
        let factor = if f.offer_seller_id == 0 {
            sf(1.011f32)
        } else if g.cur_player != f.offer_seller_id && g.cur_player != g.corp_controller[f.offer_seller_id] {
            sf(0.951f32)
        } else {
            sf(1.001f32)
        };
        if f.buying_power > factor * g.market_cap[target_id] {
            f.msg_text = PbStr::new();
        }
        if !f.msg_text.is_empty() {
            show_quick_message(g, ui, &mut f.msg_text);
            return;
        }
    }
    buyer_slot = find_holding_slot(g, buyer_id, target_id);
    if buyer_slot == 0 {
        max15_holdings_msg(g, ui, &mut f);
        return;
    }
    if 0 < f.etf_buyer {
        g.public_pct = 0;
        if target_id <= 10 {
            return;
        }
        update_corp_control(g, ui, rng, target_id);
        if g.public_pct == 0 {
            f.msg_text = s(__content.string(11920))
                + &g.ent_name[target_id]
                + __content.string(11921)
                + __content.string(11922);
        } else if g.public_pct < 11 && find_stock_holding(g, buyer_id, target_id) == 0 {
            f.msg_text = s(__content.string(11923))
                + &str7(cast::<Ext, _>(g.public_pct))
                + __content.string(2163)
                + &g.ent_name[target_id]
                + __content.string(11924)
                + __content.string(11925);
        } else if 1580 < target_id && g.industry_num[target_id] == 71 {
            f.msg_text = f.buyer_name.clone()
                + __content.string(11926)
                + __content.string(11927);
        } else if pick_etf_holding_stock(g, ui, rng, buyer_id, target_id) == 0 {
            f.msg_text = s(__content.string(11928));
            if buyer_id == 1593 {
                f.msg_text = f.msg_text.clone()
                    + &f.buyer_name
                    + __content.string(11929)
                    + __content.string(11930)
                    + &g.ent_name[target_id]
                    + __content.string(11931)
                    + &g.country_name[g.country_idx[target_id]]
                    + __content.string(11932);
            } else {
                f.msg_text = f.msg_text.clone()
                    + &g.ent_name[target_id]
                    + __content.string(11933)
                    + &g.ind_name[cast::<i32, _>(g.industry_num[target_id])]
                    + __content.string(1037)
                    + __content.string(11934)
                    + &f.buyer_name
                    + __content.string(11935)
                    + "\r"
                    + "\r"
                    + &get_etf_description(buyer_id)
                    + "\r"
                    + __content.string(11936);
            }
        } else {
            f.msg_text = PbStr::new();
        }
        if !f.msg_text.is_empty() {
            let m = s(__content.string(11937)) + "\r" + "\r" + &f.msg_text + "\r" + "\r" + __content.string(11938);
            ui.msgbox(&m, 8208, &s(__content.string(11939)));
            return;
        }
    }
    if 10 < target_id && g.merger_target == target_id && f.etf_buyer == 0 {
        let cp = g.cur_player;
        f.tmp_slot = calc_group_stake_pct(g, cp, target_id);
        let insider_pct = g.tmp_ext1 + cast::<Ext, _>(f.tmp_slot);
        if insider_pct >= Ext::from_f64(5.0) {
            f.msg_text = s(__content.string(11940))
                + __content.string(11941)
                + &g.ent_name[target_id]
                + __content.string(11942)
                + &g.ent_name[g.merger_acquirer]
                + __content.string(11943)
                + __content.string(11944)
                + "of "
                + &g.ent_name[target_id]
                + __content.string(11945)
                + __content.string(11946)
                + "\r"
                + "\r"
                + __content.string(11728)
                + &str_s(insider_pct)
                + "%.";
            g.tmp_ext1 = Ext::from_f64(0.0);
            ui.msgbox(&f.msg_text, 8256, &s(__content.string(11947)));
            return;
        }
    }
    g.tmp_ext1 = Ext::from_f64(0.0);
    if buyer_id == target_id && f.etf_buyer == 0 {
        update_corp_control(g, ui, rng, buyer_id);
        if 0 < f.offer_seller_id {
            if g.cur_player == g.corp_controller[f.offer_seller_id] {
                f.msg_text = s(__content.string(11948))
                    + &f.buyer_name
                    + __content.string(11949)
                    + __content.string(11950)
                    + "\r\n"
                    + "\r\n"
                    + __content.string(11951);
            } else {
                f.msg_text = s(__content.string(11952))
                    + &g.ent_name[f.offer_seller_id]
                    + __content.string(11953)
                    + __content.string(11954)
                    + &f.buyer_name
                    + ".";
                if f.offer_seller_id < 11 {
                    f.msg_text = replace(f.msg_text.as_bytes(), b"which", b"who");
                }
                add_remove_asset_sale_offer(g, &PbStr::new(), sale_offer_idx);
            }
            show_quick_message(g, ui, &mut f.msg_text);
            return;
        }
        if g.public_pct > 0 {
            input_text = s(__content.string(11955));
        } else {
            input_text = s(__content.string(11956));
        }
        let mut m = s("Since ")
            + &f.buyer_name
            + __content.string(11957)
            + &input_text
            + __content.string(11958)
            + __content.string(11959);
        show_quick_message(g, ui, &mut m);
        if g.public_pct > 0 {
            stock_buyback_menu(g, ui, rng, now, 2);
        } else {
            stock_buyback_menu(g, ui, rng, now, 1);
        }
        return;
    }
    if 10 < buyer_id
        && buyer_id < 86
        && g.industry_num[target_id] == 1
        && (cast::<Ext, _>(g.bank_merger_player)).abs() == cast::<Ext, _>(g.cur_player)
    {
        f.msg_text = s(__content.string(11960))
            + &f.buyer_name
            + __content.string(11961)
            + __content.string(11962)
            + __content.string(11963)
            + &g.ent_name[target_id]
            + __content.string(11964);
        show_quick_message(g, ui, &mut f.msg_text);
        return;
    }
    if g.broker_fall_warn_stock != target_id {
        num = price_trend_streak(g, target_id);
        if num < -4 {
            f.msg_text = s(__content.string(1093))
                + &g.ent_symbol[target_id]
                + __content.string(11965)
                + &number_to_words(num, 21, 0)
                + __content.string(11966)
                + "\r"
                + "\r"
                + __content.string(11967)
                + "\""
                + __content.string(11968);
            if Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd() {
                f.msg_text = replace(f.msg_text.as_bytes(), __content.bytes(11969), __content.bytes(11970));
            }
            if num < -10 {
                f.msg_text = replace(f.msg_text.as_bytes(), b"STOCK?", __content.bytes(11971));
            }
            f.msg_text = f.msg_text.clone() + "\"";
            f.answer = ui.msgbox(&f.msg_text, 8483, &s(__content.string(11972)));
            g.broker_fall_warn_stock = target_id;
            if f.answer != 6 {
                return;
            }
        }
    }
    if 1580 < target_id && g.industry_num[target_id] == 71 && f.etf_buyer == 0 {
        f.msg_text = s(__content.string(9397))
            + &g.ent_name[target_id]
            + __content.string(11973)
            + __content.string(11974)
            + __content.string(11975)
            + __content.string(11976);
        update_corp_control(g, ui, rng, target_id);
        group_etf_pct = 0;
        etf_room_pct = 15;
        f.i = g.cur_player;
        while f.i <= 1600 {
            let i = f.i;
            if (i <= 10 || g.entity_status[i] == 1) && (g.cur_player == i || g.cur_player == g.corp_controller[i]) {
                let etf_hold_slot = find_stock_holding(g, i, target_id);
                if 0 < etf_hold_slot {
                    group_etf_pct = g.hold_stock_pct[(etf_hold_slot, i)].wrapping_add(group_etf_pct);
                }
            }
            f.i += 1;
        }
        f.msg_text = f.msg_text.clone() + "." + "\r\n" + "\r\n" + __content.string(11977);
        if group_etf_pct == 0 {
            f.msg_text = f.msg_text.clone() + __content.string(11978) + __content.string(11979);
        } else {
            f.msg_text = f.msg_text.clone()
                + __content.string(11980)
                + &str7(cast::<Ext, _>(group_etf_pct))
                + "% of "
                + __content.string(11981);
            if group_etf_pct == 15 {
                f.msg_text = f.msg_text.clone()
                    + __content.string(11982)
                    + __content.string(11983);
            } else {
                etf_room_pct = 15i16.wrapping_sub(group_etf_pct);
                f.msg_text = f.msg_text.clone()
                    + __content.string(11984)
                    + &str7(cast::<Ext, _>(etf_room_pct))
                    + __content.string(11985)
                    + __content.string(11986)
                    + &g.ent_name[target_id];
            }
        }
        show_quick_message(g, ui, &mut f.msg_text);
    } else if 0 < f.etf_buyer {
        f.msg_text = s(__content.string(11987))
            + &f.buyer_name
            + __content.string(11988)
            + __content.string(11989);
        show_quick_message(g, ui, &mut f.msg_text);
    } else if 10 < buyer_id && g.cur_player == g.corp_controller[target_id] {
        let cross_slot = find_stock_holding(g, target_id, buyer_id);
        if 0 < cross_slot && 10 < g.hold_stock_pct[(cross_slot, target_id)] {
            f.msg_text = s(__content.string(11990))
                + &g.ent_name[target_id]
                + __content.string(11991)
                + __content.string(11992)
                + &g.ent_name[target_id]
                + " or "
                + &f.buyer_name
                + __content.string(11993)
                + __content.string(11994);
            show_quick_message(g, ui, &mut f.msg_text);
        }
    } else if g.country_idx[target_id] == 26
        && g.japan_takeover_warn_stock != target_id
        && g.cheat_japan_target == 0
        && Ext::from_f64(1.0) < Ext::from_f64(3.0) * rng.rnd()
    {
        update_corp_control(g, ui, rng, target_id);
        if 11 <= g.voting_controller
            && g.country_idx[g.voting_controller] == 26
            && !(10 < buyer_id && g.country_idx[buyer_id] == 26)
        {
            f.msg_text = s(__content.string(9397))
                + &g.ent_name[target_id]
                + __content.string(11995)
                + __content.string(11996)
                + &f.buyer_name;
            if 10 < buyer_id {
                f.msg_text = replace(f.msg_text.as_bytes(), __content.bytes(11997), __content.bytes(11998));
            }
            show_quick_message(g, ui, &mut f.msg_text);
            g.japan_takeover_warn_stock = target_id;
        }
    }
    saved_market_cap = g.market_cap[target_id];
    if buyer_id < 11 {
        f.tmp_slot = find_holding_slot(g, buyer_id, target_id);
        if 0 < f.tmp_slot
            && cast::<i32, _>(g.hold_stock_id[(f.tmp_slot, buyer_id)]) == target_id
            && g.hold_stock_pct[(f.tmp_slot, buyer_id)] < 0
        {
            let mut m = s(__content.string(11999))
                + &g.ent_name[target_id]
                + __content.string(12000);
            show_quick_message(g, ui, &mut m);
            cover_short_position(g, ui, rng);
            return;
        }
    } else if f.etf_buyer == 0 {
        let cp = g.cur_player;
        f.tmp_slot = find_holding_slot(g, cp, target_id);
        if 0 < f.tmp_slot && cast::<i32, _>(g.hold_stock_id[(f.tmp_slot, cp)]) == target_id && g.hold_stock_pct[(f.tmp_slot, cp)] < 0
        {
            f.msg_text = s(__content.string(12001))
                + &g.ent_name[target_id]
                + __content.string(12002)
                + __content.string(12003)
                + __content.string(12004)
                + "\r"
                + "\r"
                + __content.string(12005);
            f.answer = ui.msgbox(&f.msg_text, 8483, &s(__content.string(12006)));
            if f.answer != 6 {
                return;
            }
            f.msg_text = PbStr::new();
        }
    }
    update_corp_control(g, ui, rng, target_id);
    f.avail_pct = g.public_pct;
    prev_controller = g.voting_controller;
    if 0 < f.offer_seller_id {
        f.seller_id = f.offer_seller_id;
        seller_count = 1;
        f.tmp_slot = find_stock_holding(g, f.offer_seller_id, target_id);
        if f.tmp_slot <= 0 {
            let mut m = g.ent_name[f.offer_seller_id].clone()
                + __content.string(12007)
                + &g.ent_name[target_id]
                + __content.string(12008);
            show_quick_message(g, ui, &mut m);
            add_remove_asset_sale_offer(g, &PbStr::new(), sale_offer_idx);
            return;
        }
        seller_slot = cast::<i16, _>(f.tmp_slot);
        offer_avail_pct = cast::<i32, _>(g.hold_stock_pct[(cast::<i32, _>(seller_slot), f.offer_seller_id)]);
        if f.pct_to_buy < offer_avail_pct {
            offer_avail_pct = f.pct_to_buy;
        }
        if offer_avail_pct < f.pct_to_buy {
            let mut m = g.ent_name[f.offer_seller_id].clone()
                + __content.string(12009)
                + &str_s(cast::<Ext, _>(f.pct_to_buy))
                + "% of "
                + &g.ent_name[target_id]
                + __content.string(12010)
                + &str_s(cast::<Ext, _>(offer_avail_pct))
                + "%.";
            show_quick_message(g, ui, &mut m);
            f.pct_to_buy = offer_avail_pct;
        }
    } else if f.etf_buyer == 0 {
        f.i = 1;
        while f.i <= 1600 {
            let i = f.i;
            if g.entity_status[i] == 1 && buyer_id != i {
                f.j = 1;
                while f.j <= 15 {
                    let j = f.j;
                    if cast::<i32, _>(g.hold_stock_id[(j, i)]) == target_id
                        && 0 < g.hold_stock_pct[(j, i)]
                        && !(0 < i && i < 11 && g.cur_player != i)
                        && !(10 < i && get_controller_type(g, i) == 1 && g.cur_player != g.corp_controller[i])
                        && !(g.industry_num[target_id] == 71 && g.cur_player != i && g.cur_player != g.corp_controller[i])
                    {
                        seller_lines[1] = s(__content.string(2518)) + &g.ent_name[buyer_id];
                        seller_lines[2] = s(__content.string(12011))
                            + &g.ent_symbol[target_id]
                            + __content.string(12012);
                        seller_lines[3] = string_fill(72, b"-");
                        seller_count = seller_count + 1;
                        f.seller_id = i;
                        seller_slot = cast::<i16, _>(j);
                        seller_line = g.ent_name[i].clone() + &string_fill(28, b" ");
                        seller_line =
                            mid(seller_line.as_bytes(), 1, 28) + &fmt(cast::<Ext, _>(g.hold_stock_pct[(j, i)]), "###") + "%";
                        let k = seller_count + 4;
                        if 100 < k {
                            // DIM sellerLines(1 TO 100): error 9 -> ON ERROR handler
                            runtime_error_9(g, ui);
                            return;
                        }
                        seller_lines[cast::<usize, _>(k)] =
                            seller_line + &string_fill(70, b" ") + "@C" + &fmt(cast::<Ext, _>(i), "000#");
                    }
                    f.j += 1;
                }
            }
            f.i += 1;
        }
    }
    if g.public_pct == 0 {
        f.msg_text = PbStr::new();
        if cast::<i32, _>(g.wholly_owned_by) == buyer_id && f.etf_buyer == 0 {
            f.msg_text = f.buyer_name.clone()
                + __content.string(12013)
                + __content.string(4168)
                + &g.ent_name[target_id]
                + "."
                + "\r"
                + "\r"
                + __content.string(12014);
        } else if seller_count == 0 || 0 < f.etf_buyer {
            f.msg_text = s(__content.string(12015))
                + &g.ent_name[target_id]
                + " "
                + __content.string(12016)
                + __content.string(12017);
            if 10 < buyer_id {
                f.msg_text = replace(f.msg_text.as_bytes(), b"you", f.buyer_name.as_bytes());
            }
            if 0 < g.bonds_issued[target_id]
                && 0 < instr(1, g.bond_issue_info[target_id].as_bytes(), b"cv")
                && f.etf_buyer == 0
            {
                calc_entity_financials(g, ui, rng, target_id);
                if 0 < g.bonds_public_held {
                    f.msg_text = f.msg_text.clone()
                        + "\r"
                        + "\r"
                        + __content.string(12018)
                        + "of "
                        + &g.ent_name[target_id]
                        + __content.string(12019);
                }
            }
        }
        if !f.msg_text.is_empty() {
            ui.msgbox(&f.msg_text, 8256, &s(__content.string(12020)));
            return;
        }
    }
    if f.etf_buyer == 0 && insider_conflict_check(g, ui, rng, &mut f) {
        return;
    }
    num = 0;
    if f.offer_seller_id == 0 {
        let cp = g.cur_player;
        num = notify_discount_offer(g, ui, cp, 1, target_id, 0);
    }
    if 0 < num {
        f.msg_text = s(__content.string(12021))
            + __content.string(12022)
            + &g.ent_symbol[target_id]
            + __content.string(12023)
            + &g.ent_name[num]
            + __content.string(12024)
            + __content.string(12025);
        f.answer = ui.msgbox(&f.msg_text, 8227, &s(__content.string(12026)));
        if f.answer != 7 {
            return;
        }
    }
    f.answer = 7;
    if 0 < seller_count && f.etf_buyer == 0 {
        f.aux_text = s("Does ") + &g.ent_symbol[buyer_id];
        if len(g.ent_name[target_id].as_bytes()) + len(g.ent_name[buyer_id].as_bytes()) < 32 {
            f.aux_text = s("Does ") + &g.ent_name[buyer_id];
        }
        if g.cur_player == buyer_id {
            f.aux_text = s("Do you");
        }
        input_text = f.aux_text.clone() + __content.string(12027) + &g.ent_name[target_id] + __content.string(12028);
        if seller_count == 1 {
            seller_text = g.ent_name[f.seller_id].clone();
            if 0 < f.offer_seller_id {
                input_text = replace(input_text.as_bytes(), b"held", b"offered");
            }
        } else {
            seller_text = s(__content.string(12029));
            if g.industry_num[target_id] != 71 {
                seller_text = seller_text + __content.string(12030);
            }
            if g.cur_player != buyer_id {
                let cp = g.cur_player;
                if 0 < find_stock_holding(g, cp, target_id) {
                    seller_text = s("you or ") + &seller_text;
                }
            }
        }
        f.aux_text = s(__content.string(12031));
        if g.industry_num[target_id] != 71 {
            f.aux_text = replace(f.aux_text.as_bytes(), b"Entity", __content.bytes(12032));
        }
        let m = input_text.clone() + &seller_text + "?";
        f.answer = ui.msgbox(&m, 8483, &f.aux_text);
        if f.answer == 2 {
            return;
        }
        if f.answer == 7 && 0 < f.offer_seller_id {
            return;
        }
        if f.answer == 7 {
            seller_count = 0;
            f.seller_id = 0;
        }
    }
    if f.answer == 6 && 1 < seller_count && f.etf_buyer == 0 {
        f.seller_id = 0;
        let k = seller_count + 6;
        if 100 < k {
            runtime_error_9(g, ui);
            return;
        }
        seller_lines[cast::<usize, _>(k)] = s("@END");
        for k in 1..=90 {
            g.text_lines[k] = seller_lines[cast::<usize, _>(k)].clone();
        }
        g.text_box_title = s(__content.string(12033))
            + &g.ent_name[target_id]
            + " ("
            + &g.ent_symbol[target_id]
            + ")  ===";
        if 10 < buyer_id {
            g.text_box_title = replace(g.text_box_title.as_bytes(), b"You", g.ent_symbol[buyer_id].as_bytes());
        }
        g.entity_pick_mode = s("GetCorp");
        show_text_report(g, ui, rng);
        f.seller_id = g.target_corp;
        g.target_corp = 0;
        if f.seller_id == 0 {
            return;
        }
    }
    update_corp_control(g, ui, rng, target_id);
    if g.cheat_mode == "Y"
        && g.cheat_japan_target != -1
        && 2 < g.difficulty_level
        && g.country_idx[target_id] == 26
        && 0 < g.public_pct
        && f.seller_id == 0
        && f.etf_buyer == 0
    {
        g.cheat_japan_target = cast::<i16, _>(target_id);
        if 10 < buyer_id && g.country_idx[buyer_id] == 26 {
            g.cheat_japan_target = 0;
        }
        g.voting_controller = g.corp_controller[target_id];
        if 10 < g.voting_controller && g.country_idx[g.voting_controller] != 26 {
            g.cheat_japan_target = 0;
        }
        if g.voting_controller < 11 {
            g.cheat_japan_target = 0;
        }
        if g.scenario_num != 0 {
            g.cheat_japan_target = 0;
        }
        if g.event_status[104] != 0 {
            g.cheat_japan_target = 0;
        }
        if 0 < (target_id + g.cur_year) % 4 {
            g.cheat_japan_target = 0;
        }
    }
    held_pct = 0;
    buyer_slot = find_holding_slot(g, buyer_id, target_id);
    if 0 < buyer_slot {
        held_pct = cast::<i32, _>(g.hold_stock_pct[(buyer_slot, buyer_id)]);
    } else if buyer_slot == 0 {
        max15_holdings_msg(g, ui, &mut f);
        return;
    }
    if 0 < f.etf_buyer && 10 <= held_pct {
        f.msg_text = f.buyer_name.clone()
            + __content.string(12034)
            + &g.ent_name[target_id]
            + ", "
            + __content.string(12035)
            + "\r\n"
            + "\r\n"
            + __content.string(11938);
        show_quick_message(g, ui, &mut f.msg_text);
        return;
    }
    purchase_ready = 0;
    calc_entity_financials(g, ui, rng, buyer_id);
    if 0 < f.seller_id && f.offer_seller_id == 0 && f.etf_buyer == 0 {
        let seller = f.seller_id;
        f.i = 1;
        while f.i <= 15 {
            let i = f.i;
            if cast::<i32, _>(g.hold_stock_id[(i, seller)]) == target_id && 0 < g.hold_stock_pct[(i, seller)] {
                g.public_pct = g.hold_stock_pct[(i, seller)];
                seller_slot = cast::<i16, _>(i);
                break;
            }
            f.i += 1;
        }
        if g.cur_player == g.corp_controller[seller] {
            let ss = cast::<i32, _>(seller_slot);
            if g.hold_stock_basis[(ss, seller)] > cast::<Ext, _>(g.hold_stock_pct[(ss, seller)]) * g.market_cap[target_id] {
                f.msg_text = s(__content.string(12036))
                    + &g.ent_name[seller]
                    + __content.string(12037)
                    + __content.string(12038)
                    + __content.string(12039)
                    + "\""
                    + __content.string(12040)
                    + "\""
                    + "\r"
                    + "\r"
                    + __content.string(12041);
                f.answer = ui.msgbox(&f.msg_text, 8227, &s(__content.string(12042)));
                if f.answer != 6 {
                    return;
                }
            }
        }
    } else if 0 < f.offer_seller_id && f.etf_buyer == 0 {
        g.public_pct = cast::<i16, _>(f.pct_to_buy);
    }
    if g.public_pct == 0 {
        let mut m = s(__content.string(12043));
        show_quick_message(g, ui, &mut m);
        return;
    }
    g.valuation_mode = 0;
    calc_stock_price(g, ui, rng, target_id, 0); // mode 0 does not use TIME$
    f.max_pct = g.public_pct;
    if 0 < f.etf_buyer {
        if 10 - held_pct < cast::<i32, _>(f.max_pct) {
            f.max_pct = cast::<i16, _>(10 - held_pct);
        }
        f.seller_id = 0;
    }
    if 0 < f.seller_id
        && g.cur_player != f.seller_id
        && g.cur_player != g.corp_controller[f.seller_id]
        && f.offer_seller_id == 0
    {
        f.is_private_tender = 1;
        if g.market_cap[target_id] > f.buying_power * sf(1.11f32) {
            f.max_pct = 0;
        }
    } else if 0 < f.etf_buyer {
        limit_by_funds(g, &mut f);
    } else {
        calc_price(g, ui, rng, &mut f);
    }
    if f.max_pct == 0 {
        f.msg_text = f.buyer_name.clone()
            + __content.string(12044)
            + &g.ent_name[target_id]
            + __content.string(12045);
        ui.msgbox(&f.msg_text, 8256, &s(__content.string(12046)));
        return;
    }
    // The planned tender-offer premium for a private purchase from an uncontrolled holder.
    while f.is_private_tender == 1 {
        f.msg_text = s(__content.string(12047))
            + &g.ent_name[target_id]
            + __content.string(12048)
            + __content.string(12049);
        input_text = ui.inputbox(&f.msg_text, &s(__content.string(12050)), &s("10%"));
        premium_pct = to_long(val(remove_any(input_text.as_bytes(), b"$ ,%").as_bytes()));
        if premium_pct <= 0 {
            let mut m = s(__content.string(12051));
            show_quick_message(g, ui, &mut m);
            return;
        }
        if premium_pct < 10 {
            let mut m = s(__content.string(12052))
                + __content.string(12053)
                + __content.string(12054)
                + &g.ent_name[f.seller_id];
            show_quick_message(g, ui, &mut m);
            continue;
        }
        if 100 < premium_pct {
            let mut m = s(__content.string(12055));
            show_quick_message(g, ui, &mut m);
            continue;
        }
        f.price_per_pct = (sf(0.01f32) * cast::<Ext, _>(premium_pct) + Ext::from_f64(1.0)) * g.market_cap[target_id] * sf(1.01f32);
        if f.price_per_pct > Ext::from_f64(0.0) {
            affordable_ratio = f.buying_power / f.price_per_pct;
        }
        if !(affordable_ratio >= sf(1.01f32)) {
            let mut m = s(__content.string(12056))
                + &trim(g.currency_symbol.as_bytes())
                + &fmt_trim(f.buying_power, "* ###,00")
                + &g.currency_suffix
                + __content.string(12057)
                + __content.string(12058)
                + &g.ent_name[target_id]
                + __content.string(12059);
            show_quick_message(g, ui, &mut m);
            return;
        }
        num = to_long(affordable_ratio.trunc());
        if 10 < num {
            num = num - 1;
        }
        if (cast::<i32, _>(g.public_pct)) > num {
            f.msg_text = s(__content.string(12060))
                + &number_to_words(premium_pct, 0, 1)
                + __content.string(9826)
                + &g.ent_name[target_id]
                + __content.string(12061)
                + &number_to_words(num, 0, 1)
                + " "
                + __content.string(12062)
                + &g.ent_name[target_id]
                + "\r"
                + "\r"
                + __content.string(12063)
                + __content.string(12064)
                + "\r"
                + "\r"
                + __content.string(12065);
            if 1 < num {
                f.msg_text = f.msg_text.clone()
                    + __content.string(12066)
                    + &str_s(cast::<Ext, _>(num))
                    + "% of "
                    + &g.ent_name[target_id]
                    + "'s "
                    + __content.string(12067)
                    + &str_s(cast::<Ext, _>(premium_pct))
                    + "%.";
            } else {
                f.msg_text = f.msg_text.clone() + ".";
            }
            if 10 < buyer_id {
                let with = f.buyer_name.clone() + "'s";
                f.msg_text = replace(f.msg_text.as_bytes(), b"Your", with.as_bytes());
                f.msg_text = replace(f.msg_text.as_bytes(), b"you", f.buyer_name.as_bytes());
            }
            ui.msgbox(&f.msg_text, 8256, &s(__content.string(12068)));
        } else {
            let mut m = s(__content.string(12069))
                + &str_s(cast::<Ext, _>(premium_pct))
                + __content.string(12070)
                + __content.string(12071)
                + &g.ent_symbol[target_id]
                + " "
                + __content.string(12072)
                + &g.ent_name[f.seller_id]
                + __content.string(12073);
            show_quick_message(g, ui, &mut m);
        }
        break;
    }
    // The percentage to buy.
    loop {
        if 0 < f.seller_id {
            seller_text = s(" from ") + &g.ent_symbol[f.seller_id] + ".";
            if f.offer_seller_id == f.seller_id && g.cur_player != g.corp_controller[f.offer_seller_id] {
                seller_text = s(__content.string(12074)) + &seller_text;
            }
        } else if g.industry_num[target_id] == 71 || 0 < f.etf_buyer {
            seller_text = s(__content.string(12075));
        } else {
            seller_text = s(__content.string(12076));
            if 5 <= f.group_stake_pct {
                seller_text = s(__content.string(12077));
            }
            seller_text = seller_text + __content.string(12078);
        }
        f.msg_text = s(__content.string(4144))
            + &g.ent_symbol[target_id]
            + __content.string(12079)
            + &seller_text
            + "\r\n"
            + "\r\n"
            + "("
            + &g.ent_name[target_id]
            + __content.string(12080)
            + &g.stock_price_str
            + ")."
            + "\r\n"
            + "\r";
        if f.max_pct < g.public_pct {
            f.msg_text = f.msg_text.clone() + __content.string(12081) + &str7(cast::<Ext, _>(f.max_pct)) + "%):";
        } else {
            f.msg_text = f.msg_text.clone() + "(" + &trim(str7(cast::<Ext, _>(g.public_pct)).as_bytes()) + __content.string(12082);
            if 0 < f.offer_seller_id {
                f.msg_text = replace(f.msg_text.as_bytes(), __content.bytes(12083), b"offered");
            }
        }
        f.aux_text = str7(cast::<Ext, _>(g.public_pct));
        if f.max_pct < g.public_pct {
            f.aux_text = str7(cast::<Ext, _>(f.max_pct));
        }
        if group_etf_pct == 15 {
            etf_room_pct = 0;
        }
        if 0 < etf_room_pct && (cast::<Ext, _>(etf_room_pct)) < val(f.aux_text.as_bytes()) {
            f.aux_text = str7(cast::<Ext, _>(etf_room_pct));
        }
        let title = g.ent_symbol[buyer_id].clone() + __content.string(12084);
        let default = trim(f.aux_text.as_bytes()) + "%";
        input_text = ui.inputbox(&f.msg_text, &title, &default);
        if target_id == buyer_id {
            let mut m = f.buyer_name.clone() + __content.string(12085);
            show_quick_message(g, ui, &mut m);
            return;
        }
        f.pct_to_buy = to_long(val(remove_any(input_text.as_bytes(), b"$ ,").as_bytes()));
        if 0 < f.etf_buyer {
            if (cast::<i32, _>(f.max_pct)) < f.pct_to_buy {
                continue;
            }
            if f.pct_to_buy < 1 {
                return;
            }
            etf_purchase(g, ui, rng, &mut f, buyer_slot);
            return;
        }
        if 10 < target_id && 10 < buyer_id && f.etf_buyer == 0 {
            let ae = g.active_entity;
            let circular_slot = find_stock_holding(g, target_id, ae);
            if 0 < circular_slot && 19 < g.hold_stock_pct[(circular_slot, target_id)] && 19 < held_pct + f.pct_to_buy {
                let mut m = s(__content.string(12086))
                    + &g.ent_name[target_id]
                    + __content.string(12087)
                    + &g.active_entity_name
                    + ", "
                    + __content.string(12088)
                    + &g.active_entity_name
                    + __content.string(12089)
                    + __content.string(12090)
                    + &g.ent_name[target_id]
                    + ".";
                show_quick_message(g, ui, &mut m);
                return;
            }
        }
        if f.is_private_tender == 1 && f.pct_to_buy <= cast::<i32, _>(g.public_pct) && 0 < f.pct_to_buy {
            if premium_offer(g, ui, rng, &mut f, now) {
                return;
            }
            if f.max_pct < 1 {
                let mut m = s(__content.string(12091))
                    + &g.ent_symbol[target_id]
                    + __content.string(12092);
                show_quick_message(g, ui, &mut m);
                return;
            }
            if (cast::<i32, _>(f.max_pct)) < f.pct_to_buy {
                f.msg_text = s(__content.string(12093))
                    + &str_s(cast::<Ext, _>(f.pct_to_buy))
                    + __content.string(12094)
                    + "\r"
                    + "\r"
                    + __content.string(12095);
                f.answer = ui.msgbox(&f.msg_text, 8227, &s(__content.string(12096)));
                if f.answer == 6 {
                    continue;
                }
                charge_legal_fees(g, ui, &mut f);
                return;
            }
        }
        if (cast::<i32, _>(g.public_pct)) < f.pct_to_buy {
            seller_text = s("PUBLIC");
            if 0 < f.seller_id {
                seller_text = ucase(g.ent_symbol[f.seller_id].as_bytes(), CaseMode::Ansi) + "'S";
            }
            f.msg_text = g.ent_name[target_id].clone()
                + __content.string(4149)
                + &trim(str7(cast::<Ext, _>(g.public_pct)).as_bytes())
                + __content.string(12097)
                + &seller_text
                + __content.string(12098);
            if 0 < f.offer_seller_id {
                f.msg_text = ucase(g.ent_symbol[f.offer_seller_id].as_bytes(), CaseMode::Ansi)
                    + __content.string(12099)
                    + &str7(cast::<Ext, _>(g.public_pct))
                    + __content.string(2163)
                    + &g.ent_symbol[target_id]
                    + "...";
            }
            let m = s(__content.string(4148)) + &f.msg_text + "!";
            let title = g.app_title.clone();
            ui.msgbox(&m, 8208, &title);
            continue;
        }
        if f.pct_to_buy < 1 {
            let mut m = s(__content.string(12014));
            show_quick_message(g, ui, &mut m);
            charge_legal_fees(g, ui, &mut f);
            return;
        }
        if f.seller_id == 0 {
            f.price_per_pct = g.market_cap[target_id];
            if 5 < f.group_stake_pct + f.pct_to_buy && g.industry_num[target_id] != 71 {
                let ae = g.active_entity;
                f.price_per_pct = calc_takeover_premium_price(g, ae, target_id, f.pct_to_buy, f.avail_pct);
            } else if g.industry_num[target_id] == 71 {
                f.price_per_pct = sf(1.01f32) * f.price_per_pct;
            } else {
                let factor = sf(0.33f32) * sf(0.005f32) * cast::<Ext, _>(g.difficulty_level) * cast::<Ext, _>(f.pct_to_buy) + Ext::from_f64(1.0);
                f.price_per_pct = factor * f.price_per_pct;
            }
        }
        f.total_cost = cast::<Ext, _>(f.pct_to_buy) * f.price_per_pct;
        if g.liquid_funds < f.total_cost {
            let mut m = s(__content.string(4148)) + &g.active_entity_name + __content.string(12100);
            show_quick_message(g, ui, &mut m);
            continue;
        }
        let ae = g.active_entity;
        borrow_needed = f.total_cost - (g.t_bills[ae] + g.cash[ae]);
        if Ext::from_f64(0.0) < borrow_needed {
            premium_cost = sf(0.5f32) * (f.total_cost - cast::<Ext, _>(f.pct_to_buy) * g.market_cap[target_id]);
            if borrow_to_cover_purchase(g, ui, &mut borrow_needed, premium_cost, 0) == 7 {
                charge_legal_fees(g, ui, &mut f);
                return;
            }
        }
        purchase_ready = 1;
        if purchase_ready == 1 {
            break;
        }
    }
    let ae = g.active_entity;
    if f.seller_id != 0 {
        // A private purchase from the chosen holder.
        let seller = f.seller_id;
        if g.cur_player != g.corp_controller[target_id] {
            // Would the purchase give the current player control?
            if held_pct == 0 {
                g.hold_stock_id[(buyer_slot, ae)] = cast::<i16, _>(target_id);
            }
            g.hold_stock_pct[(buyer_slot, ae)] = cast::<i16, _>(f.pct_to_buy + held_pct);
            let ss = cast::<i32, _>(seller_slot);
            g.hold_stock_pct[(ss, seller)] = cast::<i16, _>(cast::<i32, _>(g.hold_stock_pct[(ss, seller)]) - f.pct_to_buy);
            if g.hold_stock_pct[(ss, seller)] == 0 {
                g.hold_stock_id[(ss, seller)] = 0;
            }
            g.ctrl_change_msg_mode = 1;
            update_corp_control(g, ui, rng, target_id);
            g.hold_stock_pct[(buyer_slot, ae)] = cast::<i16, _>(held_pct);
            if held_pct == 0 {
                g.hold_stock_id[(buyer_slot, ae)] = 0;
            }
            seller_slot = cast::<i16, _>(find_holding_slot(g, seller, target_id));
            let ss = cast::<i32, _>(seller_slot);
            g.hold_stock_id[(ss, seller)] = cast::<i16, _>(target_id);
            g.hold_stock_pct[(ss, seller)] = (cast::<i16, _>(f.pct_to_buy)).wrapping_add(g.hold_stock_pct[(ss, seller)]);
            purchase_ready = 1;
            if g.cur_player == g.corp_controller[target_id] {
                purchase_ready = 0;
            }
            g.ctrl_change_msg_mode = 1;
            update_corp_control(g, ui, rng, target_id);
            if purchase_ready == 0 {
                g.tmp_ext1 = cast::<Ext, _>(ae);
                g.tmp_ext2 = cast::<Ext, _>(target_id);
                g.tmp_ext3 = Ext::from_f64(0.0);
                if antitrust_review_acquisition(g, ui, rng, 2) != 1 {
                    charge_legal_fees(g, ui, &mut f);
                    let mut m = s(__content.string(12101));
                    show_quick_message(g, ui, &mut m);
                    return;
                }
            }
        }
        let ss = cast::<i32, _>(seller_slot);
        g.cash[ae] = g.cash[ae] - f.total_cost;
        if g.hold_stock_id[(buyer_slot, ae)] == 0 {
            g.hold_stock_id[(buyer_slot, ae)] = cast::<i16, _>(target_id);
        }
        g.hold_stock_pct[(buyer_slot, ae)] = (cast::<i16, _>(f.pct_to_buy)).wrapping_add(g.hold_stock_pct[(buyer_slot, ae)]);
        let seller_pct = cast::<i64, _>(g.hold_stock_pct[(ss, seller)]);
        if 0 < seller_pct {
            sold_basis = g.hold_stock_basis[(ss, seller)] * (cast::<Ext, _>(f.pct_to_buy) / cast::<Ext, _>(seller_pct));
        }
        if cast::<Ext, _>(seller_pct) == cast::<Ext, _>(f.pct_to_buy) {
            sold_basis = g.hold_stock_basis[(ss, seller)];
        }
        g.hold_stock_basis[(ss, seller)] = g.hold_stock_basis[(ss, seller)] - sold_basis;
        if sold_basis > f.total_cost {
            if g.cur_player == g.corp_controller[seller] {
                g.hold_stock_basis[(buyer_slot, ae)] = sold_basis + g.hold_stock_basis[(buyer_slot, ae)];
            } else {
                if 10 < seller {
                    g.extraordinary_pl[seller] =
                        to_quad((f.total_cost - sold_basis) + cast::<Ext, _>(g.extraordinary_pl[seller]));
                } else {
                    g.player_cap_gains[seller] = (f.total_cost - sold_basis) + g.player_cap_gains[seller];
                }
                g.hold_stock_basis[(buyer_slot, ae)] = f.total_cost + g.hold_stock_basis[(buyer_slot, ae)];
            }
        }
        if !(f.total_cost < sold_basis) {
            if g.num_players >= seller {
                g.player_cap_gains[seller] = (f.total_cost - sold_basis) + g.player_cap_gains[seller];
            } else {
                g.extraordinary_pl[seller] = to_quad((f.total_cost - sold_basis) + cast::<Ext, _>(g.extraordinary_pl[seller]));
            }
            g.hold_stock_basis[(buyer_slot, ae)] = f.total_cost + g.hold_stock_basis[(buyer_slot, ae)];
        }
        g.cash[seller] = f.total_cost + g.cash[seller];
        g.hold_stock_pct[(ss, seller)] = cast::<i16, _>(cast::<i32, _>(g.hold_stock_pct[(ss, seller)]) - f.pct_to_buy);
        if g.hold_stock_pct[(ss, seller)] < 80 && g.hold_stock_basis[(ss, seller)] < Ext::from_f64(0.0) && 10 < seller {
            recapture_excess_loss(g, ui, target_id, seller);
        }
        if g.hold_stock_pct[(ss, seller)] == 0 {
            g.hold_stock_id[(ss, seller)] = 0;
        }
        g.news_msg = g.ent_symbol[ae].clone()
            + " buys"
            + &str_s(cast::<Ext, _>(f.pct_to_buy))
            + "% of "
            + &g.ent_symbol[target_id]
            + " from "
            + &g.ent_symbol[seller];
        f.msg_text = s(__content.string(12102))
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(f.total_cost, "* ###,")
            + " "
            + &g.million_prefix_lc
            + "il.";
        if len(f.msg_text.as_bytes()) + len(g.news_msg.as_bytes()) < 75 {
            g.news_msg = g.news_msg.clone() + &f.msg_text;
        }
        f.aux_text = g.news_msg.clone()
            + __content.string(12103)
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(f.price_per_pct / g.shares_outstanding[target_id], "* #####,.00")
            + __content.string(10639);
        if 0 < f.offer_seller_id && g.cur_player != g.corp_controller[f.offer_seller_id] {
            f.aux_text = replace(f.aux_text.as_bytes(), b"price", __content.bytes(12104));
            f.aux_text = f.aux_text.clone() + __content.string(12105);
        }
        post_news(g, ui, 0, 0);
        g.turn_action_count = g.turn_action_count + 1;
        if 10 < g.active_entity && g.active_entity < 86 && g.industry_num[target_id] == 1 && 0 <= g.bank_merger_player {
            g.bank_merger_player = g.cur_player;
        }
        if 0 < f.offer_seller_id {
            reduce_asset_sale_offer(g, ui, sale_offer_idx, f.pct_to_buy, 0);
            g.investor_sentiment[target_id] = -10 + g.investor_sentiment[target_id];
        }
        if sold_basis > f.total_cost && g.cur_player == g.corp_controller[seller] {
            f.aux_text = f.aux_text.clone()
                + "\r\n"
                + "\r\n"
                + "But "
                + &g.ent_name[seller]
                + __content.string(12106)
                + __content.string(12107)
                + __content.string(12108)
                + &g.ent_name[target_id]
                + __content.string(8950);
            if 0 < f.legal_fees && f.is_private_tender == 1 {
                f.aux_text = f.aux_text.clone()
                    + "\r\n"
                    + "\r\n"
                    + __content.string(12109)
                    + &trim(g.currency_symbol.as_bytes())
                    + &str_s(cast::<Ext, _>(f.legal_fees))
                    + " "
                    + &g.million_prefix_lc
                    + "illion"
                    + &g.currency_suffix
                    + ".";
            }
        }
        show_quick_message(g, ui, &mut f.aux_text);
        if f.is_private_tender == 1 {
            g.cash[ae] = g.cash[ae] - cast::<Ext, _>(f.legal_fees);
        }
        let ss = cast::<i32, _>(seller_slot);
        if g.hold_stock_pct[(ss, seller)] < 0 {
            let m = s(__content.string(12110)) + &g.ent_symbol[seller] + " owned!";
            let title = g.app_title.clone();
            ui.msgbox(&m, 8208, &title);
            g.hold_stock_id[(ss, seller)] = 0;
            g.hold_stock_pct[(ss, seller)] = 0;
            g.hold_stock_basis[(ss, seller)] = Ext::from_f64(0.0);
        }
    } else {
        if f.offer_seller_id == 0 {
            let cp = g.cur_player;
            f.tmp_slot = calc_group_stake_pct(g, cp, target_id);
            g.tmp_ext1 = Ext::from_f64(0.0);
            let title;
            if 5 < f.pct_to_buy + f.tmp_slot && g.industry_num[target_id] != 71 {
                f.msg_text = s(__content.string(12111))
                    + &fmt_trim(f.price_per_pct / g.shares_outstanding[target_id], "* ####,.00")
                    + __content.string(12112)
                    + __content.string(12113)
                    + &str_s(cast::<Ext, _>(f.pct_to_buy))
                    + __content.string(2163)
                    + &g.ent_name[target_id]
                    + __content.string(12114)
                    + __content.string(12115)
                    + &fmt_trim(g.market_cap[target_id] / g.shares_outstanding[target_id], "* ####,.00")
                    + " per "
                    + "share.)"
                    + "\r"
                    + "\r"
                    + __content.string(12116);
                title = s(__content.string(12117));
            } else if g.industry_num[target_id] == 71 {
                f.msg_text = s(__content.string(12118))
                    + &fmt_trim(g.market_cap[target_id] / g.shares_outstanding[target_id], "* ####,.00")
                    + __content.string(12119)
                    + __content.string(12120);
                title = s(__content.string(12121));
            } else {
                f.msg_text = s(__content.string(12122))
                    + __content.string(12123)
                    + &fmt_trim(f.price_per_pct / g.shares_outstanding[target_id], "* ####,.00")
                    + __content.string(12124)
                    + __content.string(12125)
                    + __content.string(12126)
                    + "\r"
                    + "\r"
                    + __content.string(12127)
                    + &fmt_trim(g.market_cap[target_id] / g.shares_outstanding[target_id], "* ####,.00")
                    + "."
                    + "\r"
                    + "\r"
                    + __content.string(12128);
                title = s(__content.string(12129));
            }
            f.msg_text = f.msg_text.clone()
                + "\r"
                + "\r"
                + __content.string(12130)
                + &trim(g.currency_symbol.as_bytes())
                + &fmt_trim(f.total_cost, "* #########,.##")
                + " "
                + &g.million_prefix_lc
                + "illion"
                + &g.currency_suffix
                + ".)";
            f.answer = ui.msgbox(&f.msg_text, 8483, &(s(__content.string(12131)) + &title));
            if f.answer != 6 {
                return;
            }
        }
        g.tmp_ext1 = cast::<Ext, _>(f.pct_to_buy);
        g.tmp_ext2 = Ext::from_f64(0.0);
        buy_stock(g, ui, rng, ae, target_id, now.time_secs());
        let ae = g.active_entity;
        if cast::<i32, _>(g.hold_stock_pct[(buyer_slot, ae)]) == held_pct {
            // Nothing was bought: undo the price change and the borrowing.
            g.market_cap[target_id] = saved_market_cap;
            if Ext::from_f64(0.0) < borrow_needed {
                g.cash[ae] = g.cash[ae] - borrow_needed;
                g.bank_debt[ae] = g.bank_debt[ae] - borrow_needed;
                let bank = cast::<i32, _>(g.lending_bank[ae]);
                g.cash[bank] = borrow_needed + g.cash[bank];
            }
            return;
        }
        if 10 < ae && ae < 86 && g.industry_num[target_id] == 1 && 0 <= g.bank_merger_player {
            g.bank_merger_player = g.cur_player;
        }
        calc_stock_price(g, ui, rng, target_id, now.time_secs());
        if Ext::from_f64(1.0) < Ext::from_f64(4.0) * rng.rnd() {
            let drop = (cast::<Ext, _>(g.difficulty_level) * Ext::from_f64(15.0)).abs();
            g.investor_sentiment[target_id] = cast::<i32, _>(to_quad(cast::<Ext, _>(g.investor_sentiment[target_id]) - drop));
        }
    }
    // Common tail.
    let ae = g.active_entity;
    if 10 < ae {
        calc_projected_earnings(g, ui, rng, ae);
    }
    g.target_corp = 0;
    g.lookup_entity_name = PbStr::new();
    g.tmp_symbol = PbStr::new();
    update_corp_control(g, ui, rng, ae);
    update_corp_control(g, ui, rng, target_id);
    if cast::<i32, _>(g.cheat_japan_target) == target_id && g.event_status[104] == 0 {
        if g.cur_player == g.corp_controller[target_id] {
            g.scenario_corp = cast::<i16, _>(target_id);
            g.event_player = cast::<i16, _>(g.cur_player);
            g.scenario_stage = to_integer(Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(25.0));
            g.scenario_num = 104;
            g.event_status[104] = 1;
            g.cheat_japan_target = -1;
            g.scenario_headline = s(__content.string(12132))
                + &g.ent_symbol[target_id]
                + __content.string(12133);
            g.profit_margin[target_id] = cast::<i16, _>(to_quad(Ext::from_f64(5.0) * rng.rnd() + cast::<Ext, _>(g.profit_margin[target_id])));
        } else {
            g.cheat_japan_target = 0;
        }
    }
    if target_id == g.sel_entity {
        g.sel_entity = 0;
        g.sel_corp_name = PbStr::new();
        g.tmp_symbol = g.ent_symbol[target_id].clone();
        g.target_corp = target_id;
        select_target_corp(g, ui, rng, now.time_secs());
    }
    if g.cur_player == prev_controller {
        check_cross_ownership(g, ui, rng, target_id, 1);
        return;
    }
    f.ent_idx = 11;
    while f.ent_idx <= 1600 {
        let e = f.ent_idx;
        if g.entity_status[e] == 1 {
            let ctl = g.corp_controller[e];
            if g.cur_player == ctl || target_id == ctl || prev_controller == ctl {
                if prev_controller == ctl && 0 < prev_controller && target_id != prev_controller {
                    update_corp_control(g, ui, rng, e);
                } else if target_id == ctl && g.cur_player == g.corp_controller[target_id] {
                    g.corp_controller[e] = g.cur_player;
                }
            }
        }
        f.ent_idx += 1;
    }
    check_cross_ownership(g, ui, rng, target_id, 1);
    update_control_chain(g, ui, rng, target_id);
    let ae = g.active_entity;
    update_control_chain(g, ui, rng, ae);
    force_regulated_call_sales(g, ui, rng);
    if g.cur_player == g.corp_controller[target_id] {
        let cp = g.cur_player;
        force_close_excess_naked_options(g, ui, rng, cp, target_id);
    }
}

/// `STR$` of an INTEGER (`PB_StrNum7`, 7 digits).
fn str7(x: Ext) -> PbStr {
    PbStr::from(str_num(x, 7).as_str())
}

/// The ON ERROR handler (0x5958fc) for error 9: `ShowRuntimeError(ERR, "FrontBuyStock", "", 0)`, then
/// EXIT SUB.
fn runtime_error_9(g: &mut Globals, ui: &mut dyn Ui) {
    let __content = crate::content::current();
    show_runtime_error(g, ui, 9, &s(__content.string(12134)), &PbStr::new(), 0);
}

/// The ETF branch of the percentage loop: the fund buys `pct_to_buy` percent at `price_per_pct` (market
/// price + 1%), with the confirmation box; [runtime resource wording] is covered from T-bills
/// (`SellTBillsToCash`) and then from the credit line (as far as it goes; the lending bank is not
/// charged: BUGS.md #243).
fn etf_purchase(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Bsm, buyer_slot: i32) {
    let __content = crate::content::current();
    let buyer_id = f.buyer_id;
    let target_id = f.target_id;
    f.total_cost = f.price_per_pct * cast::<Ext, _>(f.pct_to_buy);
    g.cash[buyer_id] = g.cash[buyer_id] - f.total_cost;
    if g.hold_stock_id[(buyer_slot, buyer_id)] == 0 {
        g.hold_stock_id[(buyer_slot, buyer_id)] = cast::<i16, _>(target_id);
    }
    g.hold_stock_pct[(buyer_slot, buyer_id)] =
        (cast::<i16, _>(f.pct_to_buy)).wrapping_add(g.hold_stock_pct[(buyer_slot, buyer_id)]);
    g.hold_stock_basis[(buyer_slot, buyer_id)] = f.total_cost + g.hold_stock_basis[(buyer_slot, buyer_id)];
    f.msg_text = s(__content.string(12135))
        + "\r"
        + "\r"
        + &f.buyer_name
        + " ("
        + &g.ent_symbol[buyer_id]
        + __content.string(12136)
        + __content.string(12137)
        + &fmt_trim(cast::<Ext, _>(f.pct_to_buy) * g.shares_outstanding[target_id], "* ##########,.00")
        + " "
        + &g.million_prefix_lc
        + __content.string(12138)
        + __content.string(12139)
        + &str_s(cast::<Ext, _>(f.pct_to_buy))
        + __content.string(12140)
        + &g.ent_name[target_id]
        + " "
        + "("
        + &g.ent_symbol[target_id]
        + __content.string(12141)
        + &trim(g.currency_symbol.as_bytes())
        + &fmt_trim(f.total_cost, "* ###,")
        + " "
        + &g.million_prefix_lc
        + "illion"
        + &g.currency_suffix
        + ".";
    ui.msgbox(&f.msg_text, 8256, &s("CONFIRMATION"));
    if 0 < g.fund_bonus_year[buyer_id] {
        g.fund_bonus_year[buyer_id] = g.cur_year;
    }
    g.turn_action_count = g.turn_action_count + 1;
    if g.cash[buyer_id] < Ext::from_f64(0.0) {
        let amount = g.cash[buyer_id].abs();
        sell_t_bills_to_cash(g, buyer_id, amount);
        if g.cash[buyer_id] < Ext::from_f64(0.0) {
            calc_entity_financials(g, ui, rng, buyer_id);
            let borrow_needed = g.cash[buyer_id].abs();
            if Ext::from_f64(0.0) < borrow_needed {
                if !(g.unused_credit_line < borrow_needed) {
                    g.bank_debt[buyer_id] = borrow_needed + g.bank_debt[buyer_id];
                    g.cash[buyer_id] = Ext::from_f64(0.0);
                } else if Ext::from_f64(0.0) < g.unused_credit_line {
                    g.bank_debt[buyer_id] = g.unused_credit_line + g.bank_debt[buyer_id];
                    g.cash[buyer_id] = g.unused_credit_line + g.cash[buyer_id];
                }
            }
        }
    }
}

/// Port of BuyStockMenu_CalcPrice (0x594362), GOSUB of BuyStockMenu.
/// The price per percent for buying up to `max_pct` from the market (or from the chosen holder): for an
/// open-market purchase that takes the player's group over 5% of a non-fund, the takeover premium
/// ([`calc_takeover_premium_price`]) with the " [runtime resource wording] " box (once; the
/// regulator is the SEC, or the target's country for non-U.S. currencies); else the market price plus
/// `0.33 * 0.005 * DifficultyLevel * max_pct` (funds +1%); from an offer of an uncontrolled seller 95% of
/// the market price. `max_pct` is lowered until `LiquidFunds` (at least 0) cover the cost.
fn calc_price(g: &mut Globals, ui: &mut dyn Ui, _rng: &mut Rng, f: &mut Bsm) {
    let __content = crate::content::current();
    let target_id = f.target_id;
    let cp = g.cur_player;
    f.group_stake_pct = calc_group_stake_pct(g, cp, target_id);
    f.sec_notice_shown = 0;
    loop {
        f.price_per_pct = g.market_cap[target_id];
        if 5 < f.group_stake_pct + cast::<i32, _>(f.max_pct) && f.seller_id == 0 && g.industry_num[target_id] != 71 {
            let ae = g.active_entity;
            let pct_arg = cast::<i32, _>(f.max_pct);
            f.price_per_pct = calc_takeover_premium_price(g, ae, target_id, pct_arg, f.avail_pct);
            let mut aux = s(__content.string(12142));
            if 5 < f.group_stake_pct {
                aux = s(__content.string(12143));
            }
            f.msg_text = aux
                + __content.string(12144)
                + &g.ent_name[target_id]
                + ", "
                + __content.string(12145)
                + &g.active_entity_name
                + __content.string(12146)
                + __content.string(12147)
                + &g.ent_symbol[target_id]
                + __content.string(12148)
                + "to "
                + &fmt_trim(f.price_per_pct / g.shares_outstanding[target_id], "* ###,.00")
                + __content.string(12149)
                + __content.string(12150)
                + "\r"
                + "\r";
            if 10 < g.active_entity {
                f.msg_text = replace(f.msg_text.as_bytes(), b"wish", b"wish it");
            }
            let mut aux = s(__content.string(12151))
                + &g.ent_name[target_id]
                + __content.string(12152)
                + &str_s(cast::<Ext, _>(f.group_stake_pct))
                + __content.string(12153);
            if f.group_stake_pct < 5 {
                if f.group_stake_pct == 0 {
                    aux = s(__content.string(12154)) + &g.ent_name[target_id] + __content.string(12155);
                }
                f.msg_text = f.msg_text.clone()
                    + __content.string(12156)
                    + __content.string(12157)
                    + &aux;
            } else {
                f.msg_text = f.msg_text.clone() + &aux;
            }
            f.msg_text = f.msg_text.clone()
                + "\r"
                + "\r"
                + __content.string(12158)
                + &g.ent_symbol[target_id]
                + " is "
                + &fmt_trim(g.market_cap[target_id] / g.shares_outstanding[target_id], "* ###,.00")
                + __content.string(1071);
            f.msg_text = s(__content.string(12159))
                + "\r"
                + "\r"
                + __content.string(12160)
                + "\r"
                + "\r"
                + &f.msg_text;
            f.aux_text = s(__content.string(12161));
            if !(g.currency_code == "USA") {
                if 10 < g.active_entity && g.country_idx[g.active_entity] == 0 {
                    f.us_corp_id = g.active_entity;
                } else if g.country_idx[target_id] == 0 {
                    f.us_corp_id = target_id;
                } else {
                    f.aux_text = s(__content.string(4801))
                        + &mcase(g.country_name[g.country_idx[target_id]].as_bytes(), CaseMode::Ansi)
                        + __content.string(12162);
                }
            }
            if 10 < f.us_corp_id {
                f.msg_text = f.msg_text.clone()
                    + "\r"
                    + "\r"
                    + "("
                    + &g.ent_name[f.us_corp_id]
                    + __content.string(12163);
            }
            if f.sec_notice_shown == 0 {
                let title = f.aux_text.clone() + __content.string(12164);
                ui.msgbox(&f.msg_text, 8192, &title);
            }
            f.sec_notice_shown = 1;
        } else if f.seller_id == 0 {
            if g.industry_num[target_id] == 71 {
                f.price_per_pct = sf(1.01f32) * f.price_per_pct;
            } else {
                let factor = sf(0.33f32) * sf(0.005f32) * cast::<Ext, _>(g.difficulty_level) * cast::<Ext, _>(f.max_pct) + Ext::from_f64(1.0);
                f.price_per_pct = factor * f.price_per_pct;
            }
        } else if 0 < f.seller_id {
            f.price_per_pct = g.market_cap[target_id];
            if f.seller_id == f.offer_seller_id && g.cur_player != g.corp_controller[f.seller_id] {
                f.price_per_pct = sf(0.95f32) * g.market_cap[target_id];
            }
        }
        f.total_cost = cast::<Ext, _>(f.max_pct) * f.price_per_pct;
        if g.liquid_funds < Ext::from_f64(0.0) {
            g.liquid_funds = Ext::from_f64(0.0);
        }
        if f.total_cost > g.liquid_funds {
            f.max_pct = f.max_pct.wrapping_sub(1);
        }
        if !(f.total_cost > g.liquid_funds) {
            return;
        }
    }
}

/// Port of BuyStockMenu_LimitByFunds (0x594c07), GOSUB of BuyStockMenu: an ETF buys at the market price
/// + 1%; `max_pct` is lowered until `LiquidFunds` (at least 0) cover it.
fn limit_by_funds(g: &mut Globals, f: &mut Bsm) {
    f.price_per_pct = sf(1.01f32) * g.market_cap[f.target_id];
    loop {
        f.total_cost = cast::<Ext, _>(f.max_pct) * f.price_per_pct;
        if g.liquid_funds < Ext::from_f64(0.0) {
            g.liquid_funds = Ext::from_f64(0.0);
        }
        if f.total_cost > g.liquid_funds {
            f.max_pct = f.max_pct.wrapping_sub(1);
        }
        if !(f.total_cost > g.liquid_funds) {
            return;
        }
    }
}

/// Port of BuyStockMenu_PremiumOffer (0x594c8b), GOSUB of BuyStockMenu. The first time, the seller's
/// premium is negotiated ([`negotiate_premium_buyout`]; legal fees = 1% of the cost). No offer (0) ends
/// the menu; a rejected offer (5) charges the legal fees and ends it. Otherwise the price is the market
/// price + premium and `max_pct` is lowered while `LiquidFunds` (at least 0) do not cover it; the loop
/// runs until the cost is within the buying power (the original loops forever if the liquid funds exceed
/// the buying power while the cost lies between them, BUGS.md #244; the port stops there). Returns true for EXIT SUB.
fn premium_offer(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Bsm, now: &LocalTime) -> bool {
    let target_id = f.target_id;
    if f.premium_negotiated == 0 {
        let ae = g.active_entity;
        f.negotiated_premium = negotiate_premium_buyout(g, ui, rng, target_id, f.seller_id, ae, f.pct_to_buy, now);
    }
    if f.negotiated_premium == 0 {
        return true;
    }
    let premium_factor = sf(0.01f32) * cast::<Ext, _>(f.negotiated_premium) + Ext::from_f64(1.0);
    f.price_per_pct = premium_factor * g.market_cap[target_id];
    f.total_cost = cast::<Ext, _>(f.pct_to_buy) * f.price_per_pct;
    if f.premium_negotiated == 0 {
        f.legal_fees = to_quad(f.total_cost * sf(0.01f32));
    }
    if f.negotiated_premium == 5 {
        charge_legal_fees(g, ui, f);
        return true;
    }
    let ae = g.active_entity;
    calc_entity_financials(g, ui, rng, ae);
    if g.liquid_funds < Ext::from_f64(0.0) {
        g.liquid_funds = Ext::from_f64(0.0);
    }
    f.premium_negotiated = 1;
    loop {
        f.total_cost = cast::<Ext, _>(f.max_pct) * f.price_per_pct;
        let mut lowered = false;
        if f.total_cost > g.liquid_funds {
            f.max_pct = f.max_pct.wrapping_sub(1);
            lowered = true;
        }
        if !(f.total_cost > f.buying_power) {
            return false;
        }
        if !lowered {
            // The original would loop forever here.
            return false;
        }
    }
}

/// Port of BuyStockMenu_ChargeLegalFees (0x594dc3), GOSUB of BuyStockMenu: for a private tender the
/// legal fees are paid (cash, and for a company its accrued P/L) with a message when positive.
fn charge_legal_fees(g: &mut Globals, ui: &mut dyn Ui, f: &mut Bsm) {
    let __content = crate::content::current();
    if f.is_private_tender != 1 {
        return;
    }
    let ae = g.active_entity;
    g.cash[ae] = g.cash[ae] - cast::<Ext, _>(f.legal_fees);
    if 10 < ae {
        g.accrued_pl[ae] = g.accrued_pl[ae] - cast::<Ext, _>(f.legal_fees);
    }
    if 0 < f.legal_fees {
        let mut m = g.active_entity_name.clone()
            + __content.string(9867)
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(cast::<Ext, _>(f.legal_fees), "* ###,")
            + " "
            + &g.million_prefix_lc
            + "illion "
            + __content.string(12165);
        show_quick_message(g, ui, &mut m);
    }
}

/// Port of BuyStockMenu_InsiderConflictCheck (0x594f20), GOSUB of BuyStockMenu.
/// When the target is not the player's (and its largest holder has at most 49%) and holds stock, and the
/// player has short positions: if the group's stake plus the public float could give control (at least
/// 20% and more than the largest holder), each shorted company whose controller also controls the target
/// is checked — the target controls it, holds 20%+ of it, or (simulated: the stake raised, controls
/// recomputed 5 rounds, then restored) the player would gain control of it. Then the player is warned
/// (YES/NO/CANCEL); anything but YES ends the menu (returns true).
fn insider_conflict_check(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Bsm) -> bool {
    let __content = crate::content::current();
    let target_id = f.target_id;
    if g.cur_player == g.corp_controller[target_id] || 49 < g.largest_holder_pct {
        return false;
    }
    if g.hold_stock_id[(0, target_id)] == 0 {
        return false;
    }
    let cp = g.cur_player;
    f.i = 1;
    while f.i <= 15 {
        if g.hold_stock_pct[(f.i, cp)] < 0 {
            f.short_corp_id = cast::<i32, _>(g.hold_stock_id[(f.i, cp)]);
        }
        f.i += 1;
    }
    if f.short_corp_id == 0 {
        return false;
    }
    f.group_pct = 0;
    let ae = g.active_entity;
    let mut ctl = find_holding_slot(g, ae, target_id);
    if ctl == 0 {
        return false;
    }
    f.group_pct = cast::<i32, _>(g.hold_stock_pct[(ctl, ae)]);
    if f.group_pct < 0 {
        f.group_pct = 0;
        return false;
    }
    f.orig_pct = f.group_pct;
    f.saved_slot = ctl;
    f.i = 11;
    while f.i <= 1600 {
        let i = f.i;
        if g.entity_status[i] == 1 && ae != i && g.cur_player == g.corp_controller[i] {
            f.j = 1;
            while f.j <= 15 {
                if cast::<i32, _>(g.hold_stock_id[(f.j, i)]) == target_id {
                    f.group_pct = f.group_pct + cast::<i32, _>(g.hold_stock_pct[(f.j, i)]);
                }
                f.j += 1;
            }
        }
        f.i += 1;
    }
    f.group_pct = f.group_pct + cast::<i32, _>(g.public_pct);
    if cast::<i32, _>(g.largest_holder_pct) >= f.group_pct || f.group_pct < 20 {
        return false;
    }
    f.i = 1;
    while f.i <= 15 {
        f.short_corp_id = 0;
        f.msg_text = PbStr::new();
        let cp = g.cur_player;
        if g.hold_stock_pct[(f.i, cp)] < 0 {
            f.short_corp_id = cast::<i32, _>(g.hold_stock_id[(f.i, cp)]);
            ctl = g.corp_controller[f.short_corp_id];
            if g.cur_player != ctl {
                if target_id == ctl {
                    f.msg_text = s("Y");
                }
                if 0 < ctl && g.corp_controller[target_id] == ctl && f.msg_text.is_empty() {
                    let sc = f.short_corp_id;
                    update_corp_control(g, ui, rng, sc);
                    if cast::<i32, _>(g.largest_holder) == target_id {
                        f.j = 1;
                        while f.j <= 15 {
                            if cast::<i32, _>(g.hold_stock_id[(f.j, target_id)]) == sc && 19 < g.hold_stock_pct[(f.j, target_id)] {
                                f.msg_text = s("Y");
                            }
                            f.j += 1;
                        }
                    } else {
                        let ae = g.active_entity;
                        g.hold_stock_id[(f.saved_slot, ae)] = cast::<i16, _>(target_id);
                        g.hold_stock_pct[(f.saved_slot, ae)] = cast::<i16, _>(f.group_pct);
                        g.corp_controller[target_id] = g.cur_player;
                        f.ent_idx = 5;
                        loop {
                            f.j = 11;
                            while f.j <= 1600 {
                                let j = f.j;
                                if g.entity_status[j] == 1 && ctl == g.corp_controller[j] {
                                    update_corp_control(g, ui, rng, j);
                                }
                                f.j += 1;
                            }
                            f.ent_idx -= 1;
                            if f.ent_idx == 0 {
                                break;
                            }
                        }
                        update_corp_control(g, ui, rng, sc);
                        if g.cur_player == g.voting_controller {
                            f.msg_text = s("Y");
                        }
                        g.corp_controller[target_id] = ctl;
                        g.corp_controller[sc] = ctl;
                        g.hold_stock_pct[(f.saved_slot, ae)] = cast::<i16, _>(f.orig_pct);
                        if f.orig_pct == 0 {
                            g.hold_stock_id[(f.saved_slot, ae)] = 0;
                        }
                        f.ent_idx = 5;
                        loop {
                            f.j = 11;
                            while f.j <= 1600 {
                                let j = f.j;
                                if g.entity_status[j] == 1 && g.cur_player == g.corp_controller[j] {
                                    update_corp_control(g, ui, rng, j);
                                }
                                f.j += 1;
                            }
                            f.ent_idx -= 1;
                            if f.ent_idx == 0 {
                                break;
                            }
                        }
                        update_corp_control(g, ui, rng, sc);
                        if g.cur_player == g.voting_controller {
                            g.corp_controller[sc] = ctl;
                        }
                    }
                    update_corp_control(g, ui, rng, target_id);
                }
                if !f.msg_text.is_empty() {
                    f.msg_text = s(__content.string(12166))
                        + &g.ent_name[target_id]
                        + __content.string(12167)
                        + __content.string(12168)
                        + "as "
                        + &g.ent_name[f.short_corp_id]
                        + __content.string(12169)
                        + __content.string(12170)
                        + "\r"
                        + "\r"
                        + __content.string(12171);
                    f.answer = ui.msgbox(&f.msg_text, 8483, &s(__content.string(12172)));
                    return f.answer != 6;
                }
            }
        }
        f.i += 1;
    }
    false
}

/// Port of BuyStockMenu_Max15HoldingsMsg (0x595811), GOSUB of BuyStockMenu: "limited to holding 15
/// stocks" (with advice unless an ETF buys).
fn max15_holdings_msg(g: &mut Globals, ui: &mut dyn Ui, f: &mut Bsm) {
    let __content = crate::content::current();
    let mut aux = s("your");
    if 10 < f.buyer_id {
        aux = s("its");
    }
    f.msg_text = s(__content.string(11818))
        + &g.ent_symbol[f.buyer_id]
        + __content.string(12173);
    if f.etf_buyer == 0 {
        f.msg_text = f.msg_text.clone()
            + __content.string(12174)
            + &aux
            + __content.string(12175);
    }
    show_quick_message(g, ui, &mut f.msg_text);
}
